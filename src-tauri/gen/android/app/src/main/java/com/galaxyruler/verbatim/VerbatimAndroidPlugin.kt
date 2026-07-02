package com.galaxyruler.verbatim

import android.Manifest
import android.accessibilityservice.AccessibilityServiceInfo
import android.app.Activity
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.speech.SpeechRecognizer
import android.view.accessibility.AccessibilityManager
import android.webkit.WebView
import androidx.core.app.ActivityCompat
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

@InvokeArg
class SyncFormatterArgs {
  var snapshot: String = ""
}

@InvokeArg
class SetBubbleCornerArgs {
  var corner: String = ""
}

@InvokeArg
class OpenUrlArgs {
  var url: String = ""
}

@InvokeArg
class DeleteHistoryArgs {
  var id: Long = 0
}

@InvokeArg
class SetEngineDictationArgs {
  var enabled: Boolean = false
}

@InvokeArg
class SetEngineModelIdArgs {
  var modelId: String = ""
}

@InvokeArg
class SetLlmPostProcessingArgs {
  var enabled: Boolean = false
}

@InvokeArg
class SetLlmModelIdArgs {
  var modelId: String = ""
}

/**
 * The real Tauri plugin for Verbatim's Android native surface. It lives in the APP module
 * (com.galaxyruler.verbatim) — not the plugin's android library — so it can reach the app's
 * services (FloatingBubbleService, VerbatimAccessibilityService, AndroidSpeechSupport).
 * Registered from Rust via register_android_plugin("com.galaxyruler.verbatim", "VerbatimAndroidPlugin").
 *
 * Phase 1 / T-CUTOVER: this replaces the raw `window.VerbatimAndroid` @JavascriptInterface bridge.
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

  // ---- State (pull + push) ----

  /** Mirror of the legacy AndroidBridge.permissionSnapshot() JSON shape. */
  @Command
  fun permissionSnapshot(invoke: Invoke) {
    invoke.resolve(buildSnapshot())
  }

  fun emitPermissions() {
    trigger("permissions", buildSnapshot())
  }

  @Command
  fun nativeTranscriptHistory(invoke: Invoke) {
    val ret = JSObject()
    ret.put("json", FloatingBubbleService.nativeTranscriptHistory(activity))
    invoke.resolve(ret)
  }

  @Command
  fun deleteHistoryEntry(invoke: Invoke) {
    val args = invoke.parseArgs(DeleteHistoryArgs::class.java)
    val ret = JSObject()
    ret.put("json", FloatingBubbleService.deleteNativeHistoryEntry(activity, args.id))
    invoke.resolve(ret)
  }

  @Command
  fun syncTextFormatter(invoke: Invoke) {
    val args = invoke.parseArgs(SyncFormatterArgs::class.java)
    FloatingBubbleService.syncTextFormatter(activity, args.snapshot)
    invoke.resolve()
  }

  // ---- Bubble position ----

  @Command
  fun bubbleCornerSnapshot(invoke: Invoke) {
    val ret = JSObject()
    ret.put("value", FloatingBubbleService.bubbleCornerSnapshot(activity))
    invoke.resolve(ret)
  }

  @Command
  fun setBubbleCorner(invoke: Invoke) {
    val args = invoke.parseArgs(SetBubbleCornerArgs::class.java)
    val ret = JSObject()
    ret.put("value", FloatingBubbleService.setBubbleCorner(activity, args.corner))
    invoke.resolve(ret)
  }

  @Command
  fun engineDictationEnabled(invoke: Invoke) {
    val ret = JSObject()
    ret.put("value", FloatingBubbleService.isEngineDictationEnabled(activity))
    invoke.resolve(ret)
  }

  @Command
  fun setEngineDictationEnabled(invoke: Invoke) {
    val args = invoke.parseArgs(SetEngineDictationArgs::class.java)
    val ret = JSObject()
    ret.put("value", FloatingBubbleService.setEngineDictationEnabled(activity, args.enabled))
    invoke.resolve(ret)
  }

  @Command
  fun setEngineModelId(invoke: Invoke) {
    val args = invoke.parseArgs(SetEngineModelIdArgs::class.java)
    val ret = JSObject()
    ret.put("value", FloatingBubbleService.setEngineModelId(activity, args.modelId))
    invoke.resolve(ret)
  }

  @Command
  fun llmPostProcessingSupport(invoke: Invoke) {
    invoke.resolve(llmSupportSnapshot())
  }

  @Command
  fun llmPostProcessingEnabled(invoke: Invoke) {
    val ret = JSObject()
    ret.put("value", FloatingBubbleService.isLlmPostProcessingEnabled(activity))
    invoke.resolve(ret)
  }

  @Command
  fun setLlmPostProcessingEnabled(invoke: Invoke) {
    val args = invoke.parseArgs(SetLlmPostProcessingArgs::class.java)
    val ret = JSObject()
    ret.put("value", FloatingBubbleService.setLlmPostProcessingEnabled(activity, args.enabled))
    invoke.resolve(ret)
  }

  @Command
  fun setLlmModelId(invoke: Invoke) {
    val args = invoke.parseArgs(SetLlmModelIdArgs::class.java)
    val ret = JSObject()
    ret.put("value", FloatingBubbleService.setLlmModelId(activity, args.modelId))
    invoke.resolve(ret)
  }

  @Command
  fun startBubble(invoke: Invoke) {
    activity.runOnUiThread {
      if (Settings.canDrawOverlays(activity)) {
        activity.startService(Intent(activity, FloatingBubbleService::class.java))
      }
    }
    invoke.resolve()
  }

  @Command
  fun stopBubble(invoke: Invoke) {
    activity.runOnUiThread {
      activity.stopService(Intent(activity, FloatingBubbleService::class.java))
    }
    invoke.resolve()
  }

  // ---- Permission / settings entry points ----

  @Command
  fun requestMicrophone(invoke: Invoke) {
    activity.runOnUiThread {
      ActivityCompat.requestPermissions(
        activity,
        arrayOf(Manifest.permission.RECORD_AUDIO),
        MICROPHONE_REQUEST_CODE,
      )
    }
    invoke.resolve()
  }

  @Command
  fun openOverlaySettings(invoke: Invoke) {
    activity.runOnUiThread {
      activity.startActivity(
        Intent(
          Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
          Uri.parse("package:${activity.packageName}"),
        ),
      )
    }
    invoke.resolve()
  }

  @Command
  fun openAccessibilitySettings(invoke: Invoke) {
    val component = ComponentName(activity, VerbatimAccessibilityService::class.java).flattenToString()
    val args = Bundle().apply { putString(":settings:fragment_args_key", component) }
    val intent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).apply {
      putExtra(":settings:fragment_args_key", component)
      putExtra(":settings:show_fragment_args", args)
    }
    activity.runOnUiThread { activity.startActivity(intent) }
    invoke.resolve()
  }

  @Command
  fun requestSpeechModelDownload(invoke: Invoke) {
    AndroidSpeechSupport.requestModelDownload(activity)
    invoke.resolve()
  }

  @Command
  fun openExternalUrl(invoke: Invoke) {
    val args = invoke.parseArgs(OpenUrlArgs::class.java)
    val ret = JSObject()
    if (!ALLOWED_EXTERNAL_URLS.contains(args.url)) {
      ret.put("value", false)
      invoke.resolve(ret)
      return
    }
    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(args.url))
    if (intent.resolveActivity(activity.packageManager) == null) {
      ret.put("value", false)
      invoke.resolve(ret)
      return
    }
    activity.runOnUiThread { activity.startActivity(intent) }
    ret.put("value", true)
    invoke.resolve(ret)
  }

  // ---- Internal ----

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
      .apply {
        val llm = FloatingBubbleService.llmPostProcessingSupport(activity)
        put("llmPostProcessingSupported", llm.supported)
        put("llmPostProcessingReason", llm.reason)
        put("llmTotalRamMb", llm.totalRamMb)
        put("llmAvailableRamMb", llm.availableRamMb)
        put("llmMinRamMb", llm.minRamMb)
        put("llmHardware", llm.hardware)
        put("llmSocModel", llm.socModel)
      }

  private fun llmSupportSnapshot(): JSObject {
    val llm = FloatingBubbleService.llmPostProcessingSupport(activity)
    return JSObject()
      .put("supported", llm.supported)
      .put("reason", llm.reason)
      .put("totalRamMb", llm.totalRamMb)
      .put("availableRamMb", llm.availableRamMb)
      .put("minRamMb", llm.minRamMb)
      .put("hardware", llm.hardware)
      .put("socModel", llm.socModel)
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

  companion object {
    private const val MICROPHONE_REQUEST_CODE = 4808

    private val ALLOWED_EXTERNAL_URLS = setOf(
      "https://github.com/GalaxyRuler/Verbatim",
      "https://github.com/cjpais/Handy",
      "https://verbatim.alkulaib.io/privacy",
    )

    /** Set in load(); lets MainActivity (e.g. onRequestPermissionsResult) push a fresh snapshot. */
    @Volatile
    var instance: VerbatimAndroidPlugin? = null
      private set
  }
}
