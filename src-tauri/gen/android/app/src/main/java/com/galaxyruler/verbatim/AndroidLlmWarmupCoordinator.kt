package com.galaxyruler.verbatim

import android.content.Context

object AndroidLlmWarmupCoordinator {
  fun onPostProcessingEnabledChanged(
    context: Context,
    enabled: Boolean,
    requiredModelFiles: Array<String>,
  ): Boolean {
    if (!enabled) {
      return false
    }
    return warmSelectedModel(context, requiredModelFiles)
  }

  fun onModelSelected(context: Context, requiredModelFiles: Array<String>): Boolean =
    warmSelectedModel(context, requiredModelFiles)

  private fun warmSelectedModel(context: Context, requiredModelFiles: Array<String>): Boolean {
    val support = AndroidLlmPostProcessorSupport.snapshot(context)
    if (!support.supported) {
      AndroidLlmPostProcessor.logDebug("LLM cleanup warm-up skipped reason=${support.reason}")
      return false
    }

    if (!AndroidLlmModelSelectionStore.isLlmModelInstalled(context, requiredModelFiles)) {
      AndroidLlmPostProcessor.logDebug("LLM cleanup warm-up skipped missing model")
      return false
    }

    return AndroidLlmPostProcessor.warmUp(
      context = context,
      modelPath = AndroidLlmModelSelectionStore.llmModelPath(context, requiredModelFiles),
    )
  }
}
