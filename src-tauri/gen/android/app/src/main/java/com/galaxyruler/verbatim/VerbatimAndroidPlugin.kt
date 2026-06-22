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
import android.webkit.WebView
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
 * Phase 1 / T-CUTOVER: replaces the raw `window.VerbatimAndroid` @JavascriptInterface bridge.
 * State changes are PUSHED to JS via trigger("permissions", ...) on resume (ADR-1: no polling).
 */
@TauriPlugin
class VerbatimAndroidPlugin(private val activity: Activity) : Plugin(activity) {

  override fun load(webView: WebView) {
    super.load(webView)
    instance = this
  }

  override fun onResume() {
    super.onResume()
    // Returning from a permission dialog / settings screen is when state most often changes.
    // Push a fresh snapshot instead of having the webview poll every 1.2s.
    emitPermissions()
  }

  override fun onDestroy() {
    if (instance === this) {
      instance = null
    }
    super.onDestroy()
  }

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
    invoke.resolve(buildSnapshot())
  }

  fun emitPermissions() {
    trigger("permissions", buildSnapshot())
  }

  private fun buildSnapshot(): JSObject =
    JSObject()
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

  companion object {
    /** Set in load(); lets MainActivity (e.g. onRequestPermissionsResult) push a fresh snapshot. */
    @Volatile
    var instance: VerbatimAndroidPlugin? = null
      private set
  }
}
