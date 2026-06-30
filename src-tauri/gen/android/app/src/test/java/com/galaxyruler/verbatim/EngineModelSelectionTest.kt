package com.galaxyruler.verbatim

import android.content.Context
import android.content.SharedPreferences
import android.content.pm.ApplicationInfo
import io.mockk.Runs
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class EngineModelSelectionTest {
  @Test
  fun engineModelInstallationAcceptsSenseVoiceLayoutWithoutStreamingFiles() {
    val fixture = EngineSelectionFixture("sensevoice")
    try {
      val packDir = File(
        fixture.appDataRoot,
        "models/android-asr/sensevoice-multilingual-zh-en-ja-ko-yue",
      )
      fixture.setEngineModelId("sensevoice-multilingual-zh-en-ja-ko-yue")

      assertFalse(
        EngineModelSelectionStore.isEngineModelInstalled(
          fixture.context,
          EngineModelSelectionStore.requiredFilesForPack(fixture.context),
        ),
      )

      arrayOf("sense_voice/model.onnx", "sense_voice/tokens.txt", "silero_vad_v4.onnx").forEach {
        File(packDir, it).also { file ->
          file.parentFile?.mkdirs()
          file.writeText("fixture")
        }
      }

      assertTrue(
        EngineModelSelectionStore.isEngineModelInstalled(
          fixture.context,
          EngineModelSelectionStore.requiredFilesForPack(fixture.context),
        ),
      )
    } finally {
      fixture.cleanup()
    }
  }

  @Test
  fun engineModelPathUsesDownloaderRootAndPreservesAbsoluteSelections() {
    val context = mockk<Context>()
    val prefs = mockk<SharedPreferences>()
    val editor = mockk<SharedPreferences.Editor>()
    val appDataRoot = File("build/test-data/verbatim-app-data").absoluteFile
    val appInfo = ApplicationInfo().apply {
      dataDir = appDataRoot.absolutePath
    }
    var storedModelId: String? = null

    every { context.getSharedPreferences("verbatim_android", Context.MODE_PRIVATE) } returns prefs
    every { context.applicationInfo } returns appInfo
    every { prefs.getString("native_engine_model_id", "default") } answers {
      storedModelId ?: "default"
    }
    every { prefs.edit() } returns editor
    every { editor.putString("native_engine_model_id", any()) } answers {
      storedModelId = secondArg()
      editor
    }
    every { editor.apply() } just Runs

    assertEquals(
      "g3-zipformer-whisper-tiny-en",
      EngineModelSelectionStore.setEngineModelId(context, " g3-zipformer-whisper-tiny-en "),
    )
    assertEquals("g3-zipformer-whisper-tiny-en", EngineModelSelectionStore.engineModelId(context))
    assertEquals(
      File(
        appDataRoot,
        "models/android-asr/g3-zipformer-whisper-tiny-en",
      ).absolutePath,
      EngineModelSelectionStore.engineModelDir(context),
    )

    assertEquals("default", EngineModelSelectionStore.setEngineModelId(context, " "))
    assertEquals("default", EngineModelSelectionStore.engineModelId(context))

    val absolutePackDir = File(appDataRoot, "models/android-asr/g3-zipformer-whisper-tiny-en")
    assertEquals(
      absolutePackDir.absolutePath,
      EngineModelSelectionStore.setEngineModelId(context, " ${absolutePackDir.absolutePath} "),
    )
    assertEquals(absolutePackDir.absolutePath, EngineModelSelectionStore.engineModelDir(context))

    val requiredFiles = arrayOf("streaming/encoder.onnx", "whisper/tokens.txt")
    appDataRoot.deleteRecursively()
    assertFalse(EngineModelSelectionStore.isEngineModelInstalled(context, requiredFiles))
    requiredFiles.forEach { relativePath ->
      File(absolutePackDir, relativePath).also { file ->
        file.parentFile?.mkdirs()
        file.writeText("fixture")
      }
    }
    assertTrue(EngineModelSelectionStore.isEngineModelInstalled(context, requiredFiles))
    appDataRoot.deleteRecursively()
  }

  @Test
  fun llmModelPathUsesDedicatedDownloaderRootAndPreservesAbsoluteSelections() {
    val context = mockk<Context>()
    val prefs = mockk<SharedPreferences>()
    val editor = mockk<SharedPreferences.Editor>()
    val appDataRoot = File("build/test-data/verbatim-llm-app-data").absoluteFile
    val appInfo = ApplicationInfo().apply {
      dataDir = appDataRoot.absolutePath
    }
    var storedModelId: String? = null

    every { context.getSharedPreferences("verbatim_android", Context.MODE_PRIVATE) } returns prefs
    every { context.applicationInfo } returns appInfo
    every { prefs.getString("native_llm_model_id", "default") } answers {
      storedModelId ?: "default"
    }
    every { prefs.edit() } returns editor
    every { editor.putString("native_llm_model_id", any()) } answers {
      storedModelId = secondArg()
      editor
    }
    every { editor.apply() } just Runs

    assertEquals(
      "g4-qwen2_5-0_5b-litert-q8",
      AndroidLlmModelSelectionStore.setLlmModelId(context, " g4-qwen2_5-0_5b-litert-q8 "),
    )
    assertEquals("g4-qwen2_5-0_5b-litert-q8", AndroidLlmModelSelectionStore.llmModelId(context))
    assertEquals(
      File(
        appDataRoot,
        "models/android-llm-postproc/g4-qwen2_5-0_5b-litert-q8",
      ).absolutePath,
      AndroidLlmModelSelectionStore.llmModelDir(context),
    )

    val requiredFiles = arrayOf(
      "qwen2.5-0.5b-instruct-q8.task",
      "qwen2.5-1.5b-instruct-q8.task",
    )
    val packDir = File(appDataRoot, "models/android-llm-postproc/g4-qwen2_5-0_5b-litert-q8")
    assertEquals(
      File(packDir, "qwen2.5-0.5b-instruct-q8.task").absolutePath,
      AndroidLlmModelSelectionStore.llmModelPath(context, requiredFiles),
    )
    assertFalse(AndroidLlmModelSelectionStore.isLlmModelInstalled(context, requiredFiles))
    File(packDir, requiredFiles[0]).also { file ->
      file.parentFile?.mkdirs()
      file.writeText("fixture")
    }
    assertTrue(AndroidLlmModelSelectionStore.isLlmModelInstalled(context, requiredFiles))

    val largerPackDir = File(appDataRoot, "models/android-llm-postproc/g4-qwen2_5-1_5b-litert-q8")
    assertEquals(
      largerPackDir.absolutePath,
      AndroidLlmModelSelectionStore.setLlmModelId(context, " ${largerPackDir.absolutePath} "),
    )
    assertFalse(AndroidLlmModelSelectionStore.isLlmModelInstalled(context, requiredFiles))
    File(largerPackDir, requiredFiles[1]).also { file ->
      file.parentFile?.mkdirs()
      file.writeText("fixture")
    }
    assertTrue(AndroidLlmModelSelectionStore.isLlmModelInstalled(context, requiredFiles))
    assertEquals(
      File(largerPackDir, "qwen2.5-1.5b-instruct-q8.task").absolutePath,
      AndroidLlmModelSelectionStore.llmModelPath(context, requiredFiles),
    )

    val absolutePackDir = File(appDataRoot, "custom-llm")
    assertEquals(
      absolutePackDir.absolutePath,
      AndroidLlmModelSelectionStore.setLlmModelId(context, " ${absolutePackDir.absolutePath} "),
    )
    assertEquals(absolutePackDir.absolutePath, AndroidLlmModelSelectionStore.llmModelDir(context))
    appDataRoot.deleteRecursively()
  }

  private class EngineSelectionFixture(name: String) {
    val context = mockk<Context>()
    val appDataRoot = File("build/test-data/verbatim-$name-app-data").absoluteFile
    private val prefs = mockk<SharedPreferences>()
    private val editor = mockk<SharedPreferences.Editor>()
    private var storedModelId: String? = null

    init {
      val appInfo = ApplicationInfo().apply {
        dataDir = appDataRoot.absolutePath
      }

      every { context.getSharedPreferences("verbatim_android", Context.MODE_PRIVATE) } returns prefs
      every { context.applicationInfo } returns appInfo
      every { prefs.getString("native_engine_model_id", "default") } answers {
        storedModelId ?: "default"
      }
      every { prefs.edit() } returns editor
      every { editor.putString("native_engine_model_id", any()) } answers {
        storedModelId = secondArg()
        editor
      }
      every { editor.apply() } just Runs
    }

    fun setEngineModelId(modelId: String): String =
      EngineModelSelectionStore.setEngineModelId(context, modelId)

    fun cleanup() {
      appDataRoot.deleteRecursively()
    }
  }
}
