package com.galaxyruler.verbatim

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class DebugInsertProbeReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent?) {
    when (intent?.action) {
      ACTION_DEBUG_INSERT_PROBE -> FloatingBubbleService.startDebugInsertionProbe(context)
      ACTION_DEBUG_ENGINE_WAV_SMOKE ->
        FloatingBubbleService.startDebugEngineWavSmoke(
          context,
          intent.getStringExtra(EXTRA_DEBUG_WAV_PATH),
          intent.getStringExtra(EXTRA_DEBUG_LANGUAGE),
          intent.getStringExtra(EXTRA_DEBUG_MODEL_ID),
          intent.getStringExtra(EXTRA_DEBUG_MODEL_DIR),
        )
      ACTION_DEBUG_LLM_CLEANUP_SMOKE ->
        FloatingBubbleService.startDebugLlmCleanupSmoke(
          context,
          intent.getStringExtra(EXTRA_DEBUG_RAW_TEXT),
          intent.getStringExtra(EXTRA_DEBUG_MODEL_PATH),
        )
    }
  }

  companion object {
    private const val ACTION_DEBUG_INSERT_PROBE =
      "com.galaxyruler.verbatim.action.DEBUG_INSERT_PROBE"
    private const val ACTION_DEBUG_ENGINE_WAV_SMOKE =
      "com.galaxyruler.verbatim.action.DEBUG_ENGINE_WAV_SMOKE"
    private const val ACTION_DEBUG_LLM_CLEANUP_SMOKE =
      "com.galaxyruler.verbatim.action.DEBUG_LLM_CLEANUP_SMOKE"
    private const val EXTRA_DEBUG_WAV_PATH = "wav_path"
    private const val EXTRA_DEBUG_LANGUAGE = "lang"
    private const val EXTRA_DEBUG_MODEL_ID = "model_id"
    private const val EXTRA_DEBUG_MODEL_DIR = "model_dir"
    private const val EXTRA_DEBUG_RAW_TEXT = "raw_text"
    private const val EXTRA_DEBUG_MODEL_PATH = "model_path"
  }
}
