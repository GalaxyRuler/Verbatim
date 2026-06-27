package com.galaxyruler.verbatim

import android.content.Context

object EngineModelSelectionStore {
  private const val PREFS_NAME = "verbatim_android"
  private const val ENGINE_MODEL_ID_KEY = "native_engine_model_id"
  private const val DEFAULT_ENGINE_MODEL_ID = "default"

  fun engineModelId(context: Context): String =
    context
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .getString(ENGINE_MODEL_ID_KEY, DEFAULT_ENGINE_MODEL_ID)
      ?: DEFAULT_ENGINE_MODEL_ID

  fun setEngineModelId(context: Context, modelId: String): String {
    val normalized = modelId.trim().ifBlank { DEFAULT_ENGINE_MODEL_ID }
    context
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .edit()
      .putString(ENGINE_MODEL_ID_KEY, normalized)
      .apply()
    return normalized
  }
}
