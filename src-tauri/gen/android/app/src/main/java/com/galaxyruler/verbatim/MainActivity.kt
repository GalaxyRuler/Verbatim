package com.galaxyruler.verbatim

import android.Manifest
import android.content.ComponentName
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.speech.SpeechRecognizer
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject

class MainActivity : TauriActivity() {
  private val microphoneRequestCode = 4808

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    webView.addJavascriptInterface(AndroidBridge(this), "VerbatimAndroid")
  }

  override fun onResume() {
    super.onResume()
    if (hasOverlayPermission()) {
      startBubbleService()
    }
  }

  private fun hasMicrophonePermission(): Boolean =
    ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) ==
      PackageManager.PERMISSION_GRANTED

  private fun hasOverlayPermission(): Boolean = Settings.canDrawOverlays(this)

  private fun isAccessibilityEnabled(): Boolean {
    val expected = ComponentName(this, VerbatimAccessibilityService::class.java)
      .flattenToString()
    val enabled = Settings.Secure.getString(
      contentResolver,
      Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
    ) ?: return false

    return enabled.split(':').any { it.equals(expected, ignoreCase = true) }
  }

  private fun startBubbleService() {
    startService(Intent(this, FloatingBubbleService::class.java))
  }

  private fun stopBubbleService() {
    stopService(Intent(this, FloatingBubbleService::class.java))
  }

  class AndroidBridge(private val activity: MainActivity) {
    @JavascriptInterface
    fun permissionSnapshot(): String =
      JSONObject()
        .put("microphone", activity.hasMicrophonePermission())
        .put("overlay", activity.hasOverlayPermission())
        .put("accessibility", activity.isAccessibilityEnabled())
        .put("bubbleRunning", FloatingBubbleService.isRunning)
        .put("speechRecognizerAvailable", SpeechRecognizer.isRecognitionAvailable(activity))
        .put(
          "onDeviceSpeechRecognizerAvailable",
          android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S &&
            SpeechRecognizer.isOnDeviceRecognitionAvailable(activity),
        )
        .toString()

    @JavascriptInterface
    fun nativeTranscriptHistory(): String =
      FloatingBubbleService.nativeTranscriptHistory(activity)

    @JavascriptInterface
    fun requestMicrophone() {
      activity.runOnUiThread {
        ActivityCompat.requestPermissions(
          activity,
          arrayOf(Manifest.permission.RECORD_AUDIO),
          activity.microphoneRequestCode,
        )
      }
    }

    @JavascriptInterface
    fun openOverlaySettings() {
      activity.runOnUiThread {
        val intent = Intent(
          Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
          Uri.parse("package:${activity.packageName}"),
        )
        activity.startActivity(intent)
      }
    }

    @JavascriptInterface
    fun openAccessibilitySettings() {
      val component = ComponentName(
        activity,
        VerbatimAccessibilityService::class.java,
      ).flattenToString()
      val args = Bundle().apply {
        putString(":settings:fragment_args_key", component)
      }
      val intent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).apply {
        putExtra(":settings:fragment_args_key", component)
        putExtra(":settings:show_fragment_args", args)
      }
      activity.runOnUiThread {
        activity.startActivity(intent)
      }
    }

    @JavascriptInterface
    fun startBubble() {
      activity.runOnUiThread {
        if (activity.hasOverlayPermission()) {
          activity.startBubbleService()
        }
      }
    }

    @JavascriptInterface
    fun stopBubble() {
      activity.runOnUiThread {
        activity.stopBubbleService()
      }
    }
  }
}
