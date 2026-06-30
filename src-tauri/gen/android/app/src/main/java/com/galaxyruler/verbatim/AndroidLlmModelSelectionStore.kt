package com.galaxyruler.verbatim

import android.content.Context
import java.io.File

object AndroidLlmModelSelectionStore {
  private const val PREFS_NAME = "verbatim_android"
  private const val LLM_MODEL_ID_KEY = "native_llm_model_id"
  private const val DEFAULT_LLM_MODEL_ID = "default"
  private const val MODELS_SUBDIR = "models/android-llm-postproc"

  fun llmModelId(context: Context): String =
    context
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .getString(LLM_MODEL_ID_KEY, DEFAULT_LLM_MODEL_ID)
      ?: DEFAULT_LLM_MODEL_ID

  fun llmModelDir(context: Context): String =
    resolveLlmModelDir(context, llmModelId(context)).absolutePath

  fun llmModelPath(context: Context, requiredFiles: Array<String>): String {
    val modelDir = File(llmModelDir(context))
    return firstExistingModelFile(modelDir, requiredFiles)?.absolutePath
      ?: firstTaskFile(modelDir)?.absolutePath
      ?: requiredFiles.firstOrNull()?.let { File(modelDir, it).absolutePath }
      ?: modelDir.absolutePath
  }

  fun isLlmModelInstalled(context: Context, requiredFiles: Array<String>): Boolean {
    val modelDir = File(llmModelDir(context))
    return firstExistingModelFile(modelDir, requiredFiles) != null || firstTaskFile(modelDir) != null
  }

  fun setLlmModelId(context: Context, modelId: String): String {
    val normalized = modelId.trim().ifBlank { DEFAULT_LLM_MODEL_ID }
    context
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .edit()
      .putString(LLM_MODEL_ID_KEY, normalized)
      .apply()
    return normalized
  }

  private fun resolveLlmModelDir(context: Context, modelId: String): File {
    val normalized = modelId.trim().ifBlank { DEFAULT_LLM_MODEL_ID }
    val modelPath = File(normalized)
    if (modelPath.isAbsolute) {
      return modelPath
    }

    return File(File(context.applicationInfo.dataDir), "$MODELS_SUBDIR/$normalized")
  }

  private fun firstExistingModelFile(modelDir: File, requiredFiles: Array<String>): File? =
    requiredFiles
      .map { relativePath -> File(modelDir, relativePath) }
      .firstOrNull { file -> file.isFile }

  private fun firstTaskFile(modelDir: File): File? =
    modelDir
      .listFiles { file -> file.isFile && file.name.endsWith(".task") }
      ?.sortedBy { file -> file.name }
      ?.firstOrNull()
}
