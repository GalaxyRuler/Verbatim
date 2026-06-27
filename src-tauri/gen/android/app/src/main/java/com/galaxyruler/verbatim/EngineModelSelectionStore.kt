package com.galaxyruler.verbatim

import android.content.Context
import java.io.File

object EngineModelSelectionStore {
  private const val PREFS_NAME = "verbatim_android"
  private const val ENGINE_MODEL_ID_KEY = "native_engine_model_id"
  private const val DEFAULT_ENGINE_MODEL_ID = "default"
  private const val MODELS_SUBDIR = "models/android-asr"

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
