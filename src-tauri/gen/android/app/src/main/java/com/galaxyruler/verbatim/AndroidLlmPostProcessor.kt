package com.galaxyruler.verbatim

import android.content.Context
import android.util.Log
import java.io.File
import java.lang.reflect.InvocationTargetException
import java.util.concurrent.CancellationException
import java.util.Locale
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException
import kotlin.math.max

object AndroidLlmPostProcessor {
  private const val TAG = "VerbatimLLM"
  private const val MAX_INPUT_CHARS = 4000
  private const val MAX_OUTPUT_TOKENS = 256
  const val CLEANUP_TIMEOUT_MS = 4500L

  private val executor = Executors.newSingleThreadExecutor { runnable ->
    Thread(runnable, "verbatim-llm-cleanup").apply { isDaemon = true }
  }
  private val engineLock = Any()
  private var engine: Any? = null
  private var engineModelPath: String? = null
  private var warmupInFlightPath: String? = null
  private var engineEpoch: Long = 0
  private var runtime: AndroidLlmEngineRuntime = LiteRtLmRuntime

  fun cleanUpOrNull(
    context: Context,
    rawText: String,
    modelPath: String,
    timeoutMs: Long = CLEANUP_TIMEOUT_MS,
  ): String? {
    val input = rawText.trim()
    if (input.isBlank() || input.length > MAX_INPUT_CHARS || modelPath.isBlank()) {
      return null
    }

    if (!isEngineWarm(modelPath)) {
      warmUp(context, modelPath)
      return null
    }

    val future = executor.submit<String?> {
      generateCleanup(modelPath, input)
    }

    return try {
      val output = future.get(timeoutMs, TimeUnit.MILLISECONDS)
      normalizeModelOutput(input, output)
    } catch (_: TimeoutException) {
      future.cancel(true)
      resetEngine()
      null
    } catch (_: InterruptedException) {
      future.cancel(true)
      Thread.currentThread().interrupt()
      null
    } catch (_: Exception) {
      null
    }
  }

  fun warmUp(context: Context, modelPath: String): Boolean {
    val normalizedModelPath = modelPath.trim()
    if (normalizedModelPath.isBlank()) {
      return false
    }

    val appContext = context.applicationContext ?: context
    synchronized(engineLock) {
      if (isEngineWarmLocked(normalizedModelPath) || warmupInFlightPath == normalizedModelPath) {
        return false
      }
      warmupInFlightPath = normalizedModelPath
    }

    executor.execute {
      try {
        ensureEngine(appContext, normalizedModelPath)
        logDebug("LLM cleanup warm-up complete")
      } catch (error: Exception) {
        logDebug("LLM cleanup warm-up failed: ${error.javaClass.simpleName}")
      } finally {
        synchronized(engineLock) {
          if (warmupInFlightPath == normalizedModelPath) {
            warmupInFlightPath = null
          }
        }
      }
    }
    return true
  }

  fun resetEngine() {
    val previous: Any?
    synchronized(engineLock) {
      previous = engine
      engine = null
      engineModelPath = null
      warmupInFlightPath = null
      engineEpoch += 1
    }
    try {
      previous?.let { runtime.close(it) }
    } catch (_: Exception) {
    }
  }

  private fun generateCleanup(modelPath: String, rawText: String): String? {
    val engine = warmEngine(modelPath) ?: return null
    return runtime.generateContent(engine, buildCleanupPrompt(rawText))
  }

  private fun ensureEngine(context: Context, modelPath: String): Any {
    val previous: Any?
    val startEpoch: Long
    synchronized(engineLock) {
      warmEngineLocked(modelPath)?.let { return it }
      previous = engine
      engine = null
      engineModelPath = null
      startEpoch = engineEpoch
    }

    try {
      previous?.let { runtime.close(it) }
    } catch (_: Exception) {
    }

    val cacheDir = File(context.cacheDir, "litert-lm").apply { mkdirs() }
    val next = runtime.createEngine(
      modelPath = modelPath,
      maxOutputTokens = MAX_OUTPUT_TOKENS,
      cacheDir = cacheDir,
    )

    synchronized(engineLock) {
      if (engineEpoch == startEpoch) {
        engine = next
        engineModelPath = modelPath
        return next
      }
    }

    try {
      runtime.close(next)
    } catch (_: Exception) {
    }
    throw CancellationException("LLM engine warm-up was reset")
  }

  private fun isEngineWarm(modelPath: String): Boolean =
    synchronized(engineLock) { isEngineWarmLocked(modelPath) }

  private fun isEngineWarmLocked(modelPath: String): Boolean =
    warmEngineLocked(modelPath) != null

  private fun warmEngine(modelPath: String): Any? =
    synchronized(engineLock) { warmEngineLocked(modelPath) }

  private fun warmEngineLocked(modelPath: String): Any? {
    val existing = engine ?: return null
    if (engineModelPath != modelPath) {
      return null
    }
    return try {
      if (runtime.isInitialized(existing)) existing else null
    } catch (_: Exception) {
      null
    }
  }

  fun buildCleanupPrompt(rawText: String): String =
    listOf(
      "You clean dictated transcripts for Verbatim.",
      "Fix only punctuation, capitalization, spacing, and obvious dictation artifacts.",
      "Do not translate any text.",
      "Do not add facts, greetings, signoffs, explanations, or new content.",
      "Preserve every language and script already present in the input.",
      "Preserve names, code, numbers, URLs, emails, and mixed-language text.",
      "Return only the cleaned transcript.",
      "",
      "Transcript:",
      rawText,
    ).joinToString("\n")

  fun normalizeModelOutput(input: String, output: String?): String? {
    val cleaned = stripModelWrapper(output ?: return null)
    if (cleaned.isBlank() || cleaned == input) {
      return null
    }
    if (isExcessivelyExpanded(input, cleaned)) {
      return null
    }
    if (lostRequiredScript(input, cleaned)) {
      return null
    }
    if (lostRequiredTerms(input, cleaned)) {
      return null
    }
    return cleaned
  }

  private fun stripModelWrapper(output: String): String {
    var text = output.trim()
    if (text.startsWith("```")) {
      text = text.removePrefix("```").trim()
      val firstNewline = text.indexOf('\n')
      if (firstNewline >= 0 && text.substring(0, firstNewline).length <= 16) {
        text = text.substring(firstNewline + 1).trim()
      }
      text = text.removeSuffix("```").trim()
    }
    if (
      (text.startsWith('"') && text.endsWith('"')) ||
      (text.startsWith('“') && text.endsWith('”'))
    ) {
      text = text.substring(1, text.length - 1).trim()
    }
    return text
  }

  private fun isExcessivelyExpanded(input: String, output: String): Boolean {
    val inputLength = input.length
    val outputLength = output.length
    return inputLength > 0 && outputLength > max(inputLength * 3, inputLength + 200)
  }

  private fun lostRequiredScript(input: String, output: String): Boolean =
    ScriptKind.values().any { script ->
      containsScript(input, script) && !containsScript(output, script)
    }

  private fun lostRequiredTerms(input: String, output: String): Boolean {
    val inputTerms = normalizedTerms(input)
      .filter { it.length >= 3 && !ignoredSourceTerm(it) }
      .toSet()
    if (inputTerms.isEmpty() || inputTerms.size > 8) {
      return false
    }

    val outputTerms = normalizedTerms(output).toSet()
    return inputTerms.any { it !in outputTerms }
  }

  private fun normalizedTerms(text: String): List<String> {
    val terms = mutableListOf<String>()
    val current = StringBuilder()
    text.forEach { ch ->
      if (ch.isLetterOrDigit()) {
        current.append(ch.lowercaseChar())
      } else if (current.isNotEmpty()) {
        terms.add(normalizeTerm(current.toString()))
        current.clear()
      }
    }
    if (current.isNotEmpty()) {
      terms.add(normalizeTerm(current.toString()))
    }
    return terms
  }

  private fun normalizeTerm(term: String): String =
    term
      .filterNot(::isCombiningMark)
      .map { ch ->
        when (ch) {
          'إ', 'أ', 'آ', 'ٱ' -> 'ا'
          'ى' -> 'ي'
          'ة' -> 'ه'
          else -> ch
        }
      }
      .joinToString("")
      .lowercase(Locale.ROOT)

  private fun isCombiningMark(ch: Char): Boolean =
    ch in '\u0300'..'\u036F' ||
      ch in '\u0610'..'\u061A' ||
      ch in '\u064B'..'\u065F' ||
      ch == '\u0670' ||
      ch in '\u06D6'..'\u06ED'

  private fun ignoredSourceTerm(term: String): Boolean =
    term in setOf(
      "and",
      "are",
      "but",
      "comma",
      "dash",
      "dot",
      "mark",
      "new",
      "paragraph",
      "period",
      "question",
      "the",
      "to",
    )

  private fun containsScript(text: String, script: ScriptKind): Boolean =
    text.any { ch ->
      when (script) {
        ScriptKind.LATIN -> ch.isLetter() && (ch.code in 0x0041..0x024F)
        ScriptKind.ARABIC ->
          ch.code in 0x0600..0x06FF ||
            ch.code in 0x0750..0x077F ||
            ch.code in 0x08A0..0x08FF ||
            ch.code in 0xFB50..0xFDFF ||
            ch.code in 0xFE70..0xFEFF
        ScriptKind.HEBREW -> ch.code in 0x0590..0x05FF
        ScriptKind.CYRILLIC -> ch.code in 0x0400..0x052F || ch.code in 0x2DE0..0x2DFF
        ScriptKind.CJK ->
          ch.code in 0x3040..0x30FF ||
            ch.code in 0x3400..0x4DBF ||
            ch.code in 0x4E00..0x9FFF ||
            ch.code in 0xAC00..0xD7AF
      }
    }

  fun logDebug(message: String) {
    if (BuildConfig.DEBUG) {
      Log.i(TAG, message)
    }
  }

  internal fun useRuntimeForTesting(testRuntime: AndroidLlmEngineRuntime): AutoCloseable {
    resetEngine()
    val previous = runtime
    runtime = testRuntime
    return AutoCloseable {
      resetEngine()
      runtime = previous
    }
  }

  internal fun isWarmForTesting(modelPath: String): Boolean = isEngineWarm(modelPath)

  private object LiteRtLmRuntime : AndroidLlmEngineRuntime {
    private const val PACKAGE = "com.google.ai.edge.litertlm"
    private val engineClass by lazy { Class.forName("$PACKAGE.Engine") }
    private val engineConfigClass by lazy { Class.forName("$PACKAGE.EngineConfig") }
    private val backendClass by lazy { Class.forName("$PACKAGE.Backend") }
    private val cpuBackendClass by lazy { Class.forName("$PACKAGE.Backend\$CPU") }
    private val samplerConfigClass by lazy { Class.forName("$PACKAGE.SamplerConfig") }
    private val sessionConfigClass by lazy { Class.forName("$PACKAGE.SessionConfig") }
    private val loraConfigClass by lazy { Class.forName("$PACKAGE.LoraConfig") }
    private val inputTextClass by lazy { Class.forName("$PACKAGE.InputData\$Text") }

    override fun createEngine(modelPath: String, maxOutputTokens: Int, cacheDir: File): Any {
      val backend = cpuBackendClass
        .getConstructor(Integer::class.java)
        .newInstance(Integer.valueOf(4))
      val config = engineConfigClass
        .getConstructor(
          String::class.java,
          backendClass,
          backendClass,
          backendClass,
          Integer::class.java,
          Integer::class.java,
          String::class.java,
        )
        .newInstance(
          modelPath,
          backend,
          null,
          null,
          Integer.valueOf(maxOutputTokens),
          null,
          cacheDir.absolutePath,
        )
      val engine = engineClass.getConstructor(engineConfigClass).newInstance(config)
      engineClass.getMethod("initialize").invoke(engine)
      return engine
    }

    override fun generateContent(engine: Any, prompt: String): String? {
      val sampler = samplerConfigClass
        .getConstructor(
          Int::class.javaPrimitiveType,
          Double::class.javaPrimitiveType,
          Double::class.javaPrimitiveType,
          Int::class.javaPrimitiveType,
        )
        .newInstance(1, 0.0, 0.0, 4808)
      val sessionConfig = sessionConfigClass
        .getConstructor(samplerConfigClass, loraConfigClass)
        .newInstance(sampler, null)
      val session = engineClass
        .getMethod("createSession", sessionConfigClass)
        .invoke(engine, sessionConfig)
      try {
        val input = inputTextClass
          .getConstructor(String::class.java)
          .newInstance(prompt)
        return session.javaClass
          .getMethod("generateContent", List::class.java)
          .invoke(session, listOf(input)) as? String
      } finally {
        (session as? AutoCloseable)?.close()
      }
    }

    override fun isInitialized(engine: Any): Boolean =
      engineClass.getMethod("isInitialized").invoke(engine) as? Boolean ?: false

    override fun close(engine: Any) {
      try {
        (engine as? AutoCloseable)?.close()
      } catch (_: InvocationTargetException) {
      }
    }
  }

  private enum class ScriptKind {
    LATIN,
    ARABIC,
    HEBREW,
    CYRILLIC,
    CJK,
  }
}

internal interface AndroidLlmEngineRuntime {
  fun createEngine(modelPath: String, maxOutputTokens: Int, cacheDir: File): Any
  fun generateContent(engine: Any, prompt: String): String?
  fun isInitialized(engine: Any): Boolean
  fun close(engine: Any)
}
