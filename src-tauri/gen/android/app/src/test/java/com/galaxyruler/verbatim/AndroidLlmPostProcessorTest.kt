package com.galaxyruler.verbatim

import android.content.Context
import android.content.SharedPreferences
import android.content.pm.ApplicationInfo
import io.mockk.Runs
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.mockkObject
import io.mockk.unmockkObject
import java.io.File
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidLlmPostProcessorTest {
  private val requiredModelFiles = arrayOf(
    "qwen2.5-0.5b-instruct-q8.task",
    "qwen2.5-1.5b-instruct-q8.task",
  )

  @Test
  fun cleanupPromptPreservesLanguageAndForbidsTranslation() {
    val prompt = AndroidLlmPostProcessor.buildCleanupPrompt("hello comma خالد")

    assertTrue(prompt.contains("Do not translate"))
    assertTrue(prompt.contains("Preserve every language and script"))
    assertTrue(prompt.contains("Return only the cleaned transcript"))
  }

  @Test
  fun acceptsLightCleanupWhenTermsAndScriptsRemain() {
    assertEquals(
      "Hello, خالد.",
      AndroidLlmPostProcessor.normalizeModelOutput("hello comma خالد", "Hello, خالد."),
    )
  }

  @Test
  fun rejectsBlankOrUnchangedOutputForRawFallback() {
    assertNull(AndroidLlmPostProcessor.normalizeModelOutput("hello", ""))
    assertNull(AndroidLlmPostProcessor.normalizeModelOutput("hello", "hello"))
  }

  @Test
  fun rejectsOutputsThatDropSourceTerms() {
    assertNull(
      AndroidLlmPostProcessor.normalizeModelOutput(
        "email signature",
        "Regards,\nAbdullah",
      ),
    )
  }

  @Test
  fun rejectsOutputsThatLoseOriginalScript() {
    assertNull(
      AndroidLlmPostProcessor.normalizeModelOutput(
        "please keep пример as written",
        "Please keep primer as written.",
      ),
    )
  }

  @Test
  fun enablingCleanupWarmsInstalledEngineOnceAndWarmCleanupDoesNotReload() {
    val runtime = FakeLlmRuntime(output = "Hello, world.")
    val runtimeScope = AndroidLlmPostProcessor.useRuntimeForTesting(runtime)
    val fixture = TestLlmContext("warm-enabled")
    mockSupported(fixture.context)
    try {
      fixture.selectPack("g4-qwen2_5-1_5b-litert-q8", "qwen2.5-1.5b-instruct-q8.task")

      assertTrue(
        AndroidLlmWarmupCoordinator.onPostProcessingEnabledChanged(
          fixture.context,
          enabled = true,
          requiredModelFiles = requiredModelFiles,
        ),
      )
      waitUntil { runtime.createCalls.get() == 1 }
      waitUntil { AndroidLlmPostProcessor.isWarmForTesting(fixture.taskPath) }

      assertEquals(
        "Hello, world.",
        AndroidLlmPostProcessor.cleanUpOrNull(
          context = fixture.context,
          rawText = "hello world",
          modelPath = fixture.taskPath,
          timeoutMs = 1_000,
        ),
      )
      assertEquals(1, runtime.createCalls.get())
      assertEquals(1, runtime.generateCalls.get())
    } finally {
      unmockkObject(AndroidLlmPostProcessorSupport)
      runtimeScope.close()
      fixture.cleanup()
    }
  }

  @Test
  fun selectingInstalledModelWarmsOnceAndAlreadyWarmSelectionNoOps() {
    val runtime = FakeLlmRuntime(output = "Hello.")
    val runtimeScope = AndroidLlmPostProcessor.useRuntimeForTesting(runtime)
    val fixture = TestLlmContext("warm-selected")
    mockSupported(fixture.context)
    try {
      fixture.selectPack("g4-qwen2_5-0_5b-litert-q8", "qwen2.5-0.5b-instruct-q8.task")

      assertTrue(AndroidLlmWarmupCoordinator.onModelSelected(fixture.context, requiredModelFiles))
      waitUntil { runtime.createCalls.get() == 1 }
      waitUntil { AndroidLlmPostProcessor.isWarmForTesting(fixture.taskPath) }

      assertTrue(
        !AndroidLlmWarmupCoordinator.onModelSelected(fixture.context, requiredModelFiles),
      )
      Thread.sleep(50)
      assertEquals(1, runtime.createCalls.get())
    } finally {
      unmockkObject(AndroidLlmPostProcessorSupport)
      runtimeScope.close()
      fixture.cleanup()
    }
  }

  @Test
  fun coldCleanupStartsWarmupWithoutRunningGeneration() {
    val runtime = FakeLlmRuntime(output = "Hello.")
    val runtimeScope = AndroidLlmPostProcessor.useRuntimeForTesting(runtime)
    val fixture = TestLlmContext("cold-cleanup")
    try {
      fixture.selectPack("g4-qwen2_5-0_5b-litert-q8", "qwen2.5-0.5b-instruct-q8.task")

      assertNull(
        AndroidLlmPostProcessor.cleanUpOrNull(
          context = fixture.context,
          rawText = "hello",
          modelPath = fixture.taskPath,
          timeoutMs = 1,
        ),
      )
      waitUntil { runtime.createCalls.get() == 1 }
      assertEquals(0, runtime.generateCalls.get())
    } finally {
      runtimeScope.close()
      fixture.cleanup()
    }
  }

  @Test
  fun unsupportedWarmupDoesNotLoadEngine() {
    val runtime = FakeLlmRuntime(output = "Hello.")
    val runtimeScope = AndroidLlmPostProcessor.useRuntimeForTesting(runtime)
    val fixture = TestLlmContext("unsupported")
    mockkObject(AndroidLlmPostProcessorSupport)
    every { AndroidLlmPostProcessorSupport.snapshot(fixture.context) } returns
      AndroidLlmSupportSnapshot(
        supported = false,
        reason = "requiresArm64",
        totalRamMb = 0,
        availableRamMb = 0,
        minRamMb = 8192,
        hardware = "",
        socModel = "",
      )
    try {
      fixture.selectPack("g4-qwen2_5-1_5b-litert-q8", "qwen2.5-1.5b-instruct-q8.task")

      assertTrue(
        !AndroidLlmWarmupCoordinator.onPostProcessingEnabledChanged(
          fixture.context,
          enabled = true,
          requiredModelFiles = requiredModelFiles,
        ),
      )
      Thread.sleep(50)
      assertEquals(0, runtime.createCalls.get())
    } finally {
      unmockkObject(AndroidLlmPostProcessorSupport)
      runtimeScope.close()
      fixture.cleanup()
    }
  }

  private fun mockSupported(context: Context) {
    mockkObject(AndroidLlmPostProcessorSupport)
    every { AndroidLlmPostProcessorSupport.snapshot(context) } returns
      AndroidLlmSupportSnapshot(
        supported = true,
        reason = "supported",
        totalRamMb = 12_288,
        availableRamMb = 6_144,
        minRamMb = 8192,
        hardware = "qcom",
        socModel = "SM8550",
      )
  }

  private fun waitUntil(predicate: () -> Boolean) {
    val deadline = System.currentTimeMillis() + 2_000
    while (System.currentTimeMillis() < deadline) {
      if (predicate()) {
        return
      }
      Thread.sleep(10)
    }
    assertTrue("condition was not met before timeout", predicate())
  }

  private class TestLlmContext(name: String) {
    val context: Context = mockk()
    private val prefs: SharedPreferences = mockk()
    private val editor: SharedPreferences.Editor = mockk()
    private val appDataRoot = File("build/test-data/android-llm-postprocessor-$name").absoluteFile
    private val cacheRoot = File(appDataRoot, "cache")
    private val appInfo = ApplicationInfo().apply {
      dataDir = appDataRoot.absolutePath
    }
    private var storedModelId: String? = null
    var taskPath: String = ""
      private set

    init {
      appDataRoot.deleteRecursively()
      cacheRoot.mkdirs()
      every { context.applicationInfo } returns appInfo
      every { context.cacheDir } returns cacheRoot
      every { context.applicationContext } returns context
      every { context.getSharedPreferences("verbatim_android", Context.MODE_PRIVATE) } returns prefs
      every { prefs.getString("native_llm_model_id", "default") } answers {
        storedModelId ?: "default"
      }
      every { prefs.edit() } returns editor
      every { editor.putString("native_llm_model_id", any()) } answers {
        storedModelId = secondArg()
        editor
      }
      every { editor.apply() } just Runs
    }

    fun selectPack(packId: String, taskFileName: String) {
      val packDir = File(appDataRoot, "models/android-llm-postproc/$packId")
      val task = File(packDir, taskFileName)
      task.parentFile?.mkdirs()
      task.writeText("fixture")
      taskPath = task.absolutePath
      AndroidLlmModelSelectionStore.setLlmModelId(context, packDir.absolutePath)
    }

    fun cleanup() {
      appDataRoot.deleteRecursively()
    }
  }

  private class FakeLlmRuntime(private val output: String?) : AndroidLlmEngineRuntime {
    val createCalls = AtomicInteger()
    val generateCalls = AtomicInteger()
    val closeCalls = AtomicInteger()

    override fun createEngine(modelPath: String, maxOutputTokens: Int, cacheDir: File): Any {
      createCalls.incrementAndGet()
      return FakeEngine()
    }

    override fun generateContent(engine: Any, prompt: String): String? {
      generateCalls.incrementAndGet()
      return output
    }

    override fun isInitialized(engine: Any): Boolean = (engine as FakeEngine).initialized

    override fun close(engine: Any) {
      closeCalls.incrementAndGet()
      (engine as FakeEngine).initialized = false
    }
  }

  private class FakeEngine {
    var initialized = true
  }
}
