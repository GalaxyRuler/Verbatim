package com.galaxyruler.verbatim

import android.Manifest
import android.accessibilityservice.AccessibilityServiceInfo
import android.app.Activity
import android.content.ComponentName
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import android.speech.SpeechRecognizer
import android.view.accessibility.AccessibilityManager
import androidx.core.content.ContextCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class PingArgs {
  var value: String? = null
}

/**
 * The real Tauri plugin for Verbatim's Android native surface. It lives in the APP module
 * (com.galaxyruler.verbatim) — not the plugin's android library — so it can reach the app's
 * services (FloatingBubbleService, VerbatimAccessibilityService, AndroidSpeechSupport).
 * Registered from Rust via register_android_plugin("com.galaxyruler.verbatim", "VerbatimAndroidPlugin").
 *
 * Phase 1 / T-CUTOVER: this replaces the raw `window.VerbatimAndroid` @JavascriptInterface bridge,
 * one command at a time. The bridge stays as a rollback path until parity is complete.
 */
@TauriPlugin
class VerbatimAndroidPlugin(private val activity: Activity) : Plugin(activity) {

  @Command
  fun ping(invoke: Invoke) {
    val args = invoke.parseArgs(PingArgs::class.java)
    val ret = JSObject()
    ret.put("value", args.value ?: "")
    invoke.resolve(ret)
  }

  /** Mirror of the legacy AndroidBridge.permissionSnapshot() JSON shape. */
  @Command
  fun permissionSnapshot(invoke: Invoke) {
    val ret = JSObject()
      .put("microphone", hasMicrophonePermission())
      .put("overlay", Settings.canDrawOverlays(activity))
      .put("accessibility", isAccessibilityEnabled())
      .put("bubbleRunning", FloatingBubbleService.isRunning)
      .put("bubbleVisible", FloatingBubbleService.isVisible)
      .put("speechRecognizerAvailable", SpeechRecognizer.isRecognitionAvailable(activity))
      .put(
        "onDeviceSpeechRecognizerAvailable",
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
          SpeechRecognizer.isOnDeviceRecognitionAvailable(activity),
      )
      .put("onDeviceSpeechLanguageAvailable", AndroidSpeechSupport.isLanguageAvailable(activity))
      .put("onDeviceSpeechModelStatus", AndroidSpeechSupport.currentStatus(activity))
    invoke.resolve(ret)
  }

  private fun hasMicrophonePermission(): Boolean =
    ContextCompat.checkSelfPermission(activity, Manifest.permission.RECORD_AUDIO) ==
      PackageManager.PERMISSION_GRANTED

  private fun isAccessibilityEnabled(): Boolean {
    if (VerbatimAccessibilityService.isEnabled()) {
      return true
    }
    val component = ComponentName(activity, VerbatimAccessibilityService::class.java)
    val byManager = activity.getSystemService(AccessibilityManager::class.java)
      ?.getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_ALL_MASK)
      ?.any {
        val serviceInfo = it.resolveInfo.serviceInfo
        serviceInfo.packageName == component.packageName &&
          serviceInfo.name == component.className
      } == true
    if (byManager) {
      return true
    }
    val expected = component.flattenToString()
    val shortExpected = component.flattenToShortString()
    val enabled = Settings.Secure.getString(
      activity.contentResolver,
      Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
    ) ?: return false
    return enabled.split(':').any {
      it.equals(expected, ignoreCase = true) || it.equals(shortExpected, ignoreCase = true)
    }
  }
}
