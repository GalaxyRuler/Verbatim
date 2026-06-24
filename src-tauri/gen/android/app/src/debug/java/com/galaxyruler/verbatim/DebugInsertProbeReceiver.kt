package com.galaxyruler.verbatim

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

class DebugInsertProbeReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent?) {
    if (intent?.action != ACTION_DEBUG_INSERT_PROBE) {
      return
    }

    FloatingBubbleService.startDebugInsertionProbe(context)
  }

  companion object {
    private const val ACTION_DEBUG_INSERT_PROBE =
      "com.galaxyruler.verbatim.action.DEBUG_INSERT_PROBE"
  }
}
