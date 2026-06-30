package com.galaxyruler.verbatim

import android.content.Context
import java.io.File

object EngineModelSelectionStore {
  private const val PREFS_NAME = "verbatim_android"
  private const val ENGINE_MODEL_ID_KEY = "native_engine_model_id"
  private const val DEFAULT_ENGINE_MODEL_ID = "default"
  private const val MODELS_SUBDIR = "models/android-asr"
  private const val SENSEVOICE_PACK_ID = "sensevoice-multilingual-zh-en-ja-ko-yue"
  private const val CANARY_PACK_ID = "canary-180m-flash-en-es-de-fr"
  private const val MOONSHINE_PACK_ID = "moonshine-tiny-en-int8"
  private val ZIPFORMER_WHISPER_REQUIRED_FILES = arrayOf(
    "streaming/encoder.onnx",
    "streaming/decoder.onnx",
    "streaming/joiner.onnx",
    "streaming/tokens.txt",
    "whisper/encoder.onnx",
    "whisper/decoder.onnx",
    "whisper/tokens.txt",
    "silero_vad_v4.onnx",
  )
  private val SENSEVOICE_REQUIRED_FILES = arrayOf(
    "sense_voice/model.onnx",
    "sense_voice/tokens.txt",
    "silero_vad_v4.onnx",
  )
  private val CANARY_REQUIRED_FILES = arrayOf(
    "canary/encoder.onnx",
    "canary/decoder.onnx",
    "canary/tokens.txt",
    "silero_vad_v4.onnx",
  )
  private val MOONSHINE_REQUIRED_FILES = arrayOf(
    "moonshine/preprocess.onnx",
    "moonshine/encode.int8.onnx",
    "moonshine/uncached_decode.int8.onnx",
    "moonshine/cached_decode.int8.onnx",
    "moonshine/tokens.txt",
    "silero_vad_v4.onnx",
  )

  fun engineModelId(context: Context): String =
    context
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .getString(ENGINE_MODEL_ID_KEY, DEFAULT_ENGINE_MODEL_ID)
      ?: DEFAULT_ENGINE_MODEL_ID

  fun engineModelDir(context: Context): String =
    resolveEngineModelDir(context, engineModelId(context)).absolutePath

  fun isEngineModelInstalled(context: Context, requiredFiles: Array<String>): Boolean {
    val modelDir = File(engineModelDir(context))
    return requiredFiles.all { relativePath ->
      File(modelDir, relativePath).isFile
    }
  }

  fun requiredFilesForPack(context: Context): Array<String> {
    val modelId = engineModelId(context)
    val selectedPackName = File(modelId).name
    return if (modelId == SENSEVOICE_PACK_ID || selectedPackName == SENSEVOICE_PACK_ID) {
      SENSEVOICE_REQUIRED_FILES.copyOf()
    } else if (modelId == CANARY_PACK_ID || selectedPackName == CANARY_PACK_ID) {
      CANARY_REQUIRED_FILES.copyOf()
    } else if (modelId == MOONSHINE_PACK_ID || selectedPackName == MOONSHINE_PACK_ID) {
      MOONSHINE_REQUIRED_FILES.copyOf()
    } else {
      ZIPFORMER_WHISPER_REQUIRED_FILES.copyOf()
    }
  }

  fun setEngineModelId(context: Context, modelId: String): String {
    val normalized = modelId.trim().ifBlank { DEFAULT_ENGINE_MODEL_ID }
    context
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .edit()
      .putString(ENGINE_MODEL_ID_KEY, normalized)
      .apply()
    return normalized
  }

  private fun resolveEngineModelDir(context: Context, modelId: String): File {
    val normalized = modelId.trim().ifBlank { DEFAULT_ENGINE_MODEL_ID }
    val modelPath = File(normalized)
    if (modelPath.isAbsolute) {
      return modelPath
    }

    return File(File(context.applicationInfo.dataDir), "$MODELS_SUBDIR/$normalized")
  }
}
