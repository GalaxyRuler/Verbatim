package com.galaxyruler.verbatim

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.provider.Settings
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale
import kotlin.math.abs
import kotlin.math.min

class FloatingBubbleService : Service() {
  private var windowManager: WindowManager? = null
  private var bubbleView: LinearLayout? = null
  private var layoutParams: WindowManager.LayoutParams? = null
  private var speechRecognizer: SpeechRecognizer? = null
  private var bubbleState = BubbleState.IDLE
  private var foregroundActive = false
  private var recoveryText: String? = null
  private var failureMessageResId = R.string.bubble_failed

  override fun onCreate() {
    super.onCreate()
    isRunning = true
    windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
    updateBubbleVisibility()
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (BuildConfig.DEBUG && intent?.action == ACTION_DEBUG_INSERT_PROBE) {
      insertDebugProbe()
      return START_STICKY
    }

    when (intent?.action) {
      ACTION_INPUT_TARGET_ACTIVE -> inputTargetActive = true
      ACTION_INPUT_TARGET_INACTIVE -> inputTargetActive = false
    }
    updateBubbleVisibility()
    return START_STICKY
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onDestroy() {
    speechRecognizer?.destroy()
    speechRecognizer = null
    stopMicrophoneForeground()
    bubbleView?.let { view ->
      windowManager?.removeView(view)
    }
    bubbleView = null
    windowManager = null
    isVisible = false
    isRunning = false
    super.onDestroy()
  }

  private fun updateBubbleVisibility() {
    if (inputTargetActive) {
      showBubble()
    } else {
      hideBubble()
    }
  }

  private fun showBubble() {
    if (!inputTargetActive || !Settings.canDrawOverlays(this) || bubbleView != null) {
      return
    }

    val view = createBubbleView()
    val params = WindowManager.LayoutParams(
      WindowManager.LayoutParams.WRAP_CONTENT,
      WindowManager.LayoutParams.WRAP_CONTENT,
      WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
      WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE,
      PixelFormat.TRANSLUCENT,
    ).apply {
      gravity = Gravity.TOP or Gravity.START
      x = loadCoordinate("bubble_x", dp(20))
      y = loadCoordinate("bubble_y", dp(140))
    }

    layoutParams = params
    bubbleView = view
    windowManager?.addView(view, params)
    isVisible = true
  }

  private fun hideBubble() {
    if (bubbleView == null) {
      isVisible = false
      return
    }

    speechRecognizer?.cancel()
    speechRecognizer?.destroy()
    speechRecognizer = null
    stopMicrophoneForeground()
    recoveryText = null
    bubbleState = BubbleState.IDLE
    bubbleView?.let { view ->
      windowManager?.removeView(view)
    }
    bubbleView = null
    isVisible = false
  }

  private fun createBubbleView(): LinearLayout {
    val view = LinearLayout(this).apply {
      orientation = LinearLayout.HORIZONTAL
      gravity = Gravity.CENTER
      setPadding(dp(16), dp(10), dp(10), dp(10))
      background = pillBackground("#24202A")
      contentDescription = getString(R.string.bubble_idle)
      elevation = dp(8).toFloat()
    }

    renderBubble(view)
    installDragHandler(view)
    return view
  }

  private fun installDragHandler(view: View) {
    var downRawX = 0f
    var downRawY = 0f
    var startX = 0
    var startY = 0
    var dragging = false

    view.setOnTouchListener { _, event ->
      val params = layoutParams ?: return@setOnTouchListener false
      when (event.actionMasked) {
        MotionEvent.ACTION_DOWN -> {
          downRawX = event.rawX
          downRawY = event.rawY
          startX = params.x
          startY = params.y
          dragging = false
          true
        }
        MotionEvent.ACTION_MOVE -> {
          val deltaX = event.rawX - downRawX
          val deltaY = event.rawY - downRawY
          dragging = dragging || abs(deltaX) > dp(6) || abs(deltaY) > dp(6)
          params.x = startX + deltaX.toInt()
          params.y = startY + deltaY.toInt()
          windowManager?.updateViewLayout(view, params)
          true
        }
        MotionEvent.ACTION_UP -> {
          saveCoordinate("bubble_x", params.x)
          saveCoordinate("bubble_y", params.y)
          if (!dragging) {
            handleBubbleTap()
          }
          true
        }
        else -> false
      }
    }
  }

  private fun renderBubble(view: LinearLayout) {
    view.removeAllViews()
    when (bubbleState) {
      BubbleState.IDLE -> renderIdle(view)
      BubbleState.RECORDING -> renderRecording(view)
      BubbleState.TRANSCRIBING -> renderTranscribing(view)
      BubbleState.FAILED -> renderFailed(view)
    }
  }

  private fun renderIdle(view: LinearLayout) {
    view.contentDescription = getString(R.string.bubble_idle)
    view.background = pillBackground("#24202A")
    view.addView(label(getString(R.string.bubble_idle), Color.WHITE, 16, true))
  }

  private fun renderRecording(view: LinearLayout) {
    view.contentDescription = getString(R.string.bubble_recording)
    view.background = pillBackground("#3F1010")
    view.addView(label(getString(R.string.bubble_recording), Color.WHITE, 14, true))
    repeat(5) { index ->
      val bar = View(this).apply {
        background = pillBackground("#FFB4AB")
      }
      val height = listOf(12, 22, 30, 18, 26)[index]
      val params = LinearLayout.LayoutParams(dp(4), dp(height)).apply {
        marginStart = dp(5)
      }
      view.addView(bar, params)
    }
    view.addView(
      label(getString(R.string.bubble_stop), Color.rgb(65, 0, 2), 12, true).apply {
        background = pillBackground("#FFDAD6")
        setPadding(dp(10), dp(8), dp(10), dp(8))
      },
      LinearLayout.LayoutParams(
        LinearLayout.LayoutParams.WRAP_CONTENT,
        LinearLayout.LayoutParams.WRAP_CONTENT,
      ).apply { marginStart = dp(10) },
    )
  }

  private fun renderTranscribing(view: LinearLayout) {
    view.contentDescription = getString(R.string.bubble_transcribing)
    view.background = pillBackground("#17345C")
    view.addView(label(getString(R.string.bubble_transcribing), Color.WHITE, 14, true))
    repeat(3) {
      val dot = View(this).apply {
        background = pillBackground("#D3E3FF")
      }
      val params = LinearLayout.LayoutParams(dp(7), dp(7)).apply {
        marginStart = dp(5)
      }
      view.addView(dot, params)
    }
  }

  private fun renderFailed(view: LinearLayout) {
    view.contentDescription = getString(failureMessageResId)
    view.background = pillBackground("#4B1717")
    view.addView(label(getString(failureMessageResId), Color.WHITE, 14, true))
    val action = if (recoveryText.isNullOrBlank()) {
      R.string.bubble_dismiss
    } else {
      R.string.bubble_retry_insert
    }
    view.addView(
      label(getString(action), Color.rgb(65, 0, 2), 12, true).apply {
        background = pillBackground("#FFDAD6")
        setPadding(dp(10), dp(8), dp(10), dp(8))
      },
      LinearLayout.LayoutParams(
        LinearLayout.LayoutParams.WRAP_CONTENT,
        LinearLayout.LayoutParams.WRAP_CONTENT,
      ).apply { marginStart = dp(10) },
    )
  }

  private fun handleBubbleTap() {
    when (bubbleState) {
      BubbleState.IDLE -> startListening()
      BubbleState.RECORDING -> {
        bubbleState = BubbleState.TRANSCRIBING
        bubbleView?.let { renderBubble(it) }
        speechRecognizer?.stopListening()
      }
      BubbleState.TRANSCRIBING -> Unit
      BubbleState.FAILED -> retryRecovery()
    }
  }

  private fun startListening() {
    if (!hasRequiredPermissions()) {
      showFailure(R.string.bubble_permissions_needed, null)
      Toast.makeText(this, R.string.bubble_permission_missing, Toast.LENGTH_LONG).show()
      return
    }

    if (!isOnDeviceSpeechRecognitionAvailable()) {
      showFailure(R.string.bubble_speech_missing, null)
      Toast.makeText(this, R.string.bubble_speech_unavailable, Toast.LENGTH_LONG).show()
      return
    }

    if (!AndroidSpeechSupport.isLanguageAvailable(this)) {
      AndroidSpeechSupport.refreshOnDeviceSpeechSupport(this)
      showFailure(R.string.bubble_speech_pack_missing, null)
      Toast.makeText(this, R.string.bubble_speech_pack_download, Toast.LENGTH_LONG).show()
      return
    }

    if (!startMicrophoneForeground()) {
      return
    }

    speechRecognizer?.destroy()
    speechRecognizer = createSpeechRecognizer().apply {
      setRecognitionListener(object : RecognitionListener {
        override fun onReadyForSpeech(params: Bundle?) {
          AndroidSpeechSupport.markLanguageReady(this@FloatingBubbleService)
        }
        override fun onBeginningOfSpeech() = Unit
        override fun onRmsChanged(rmsdB: Float) = Unit
        override fun onBufferReceived(buffer: ByteArray?) = Unit
        override fun onEndOfSpeech() {
          bubbleState = BubbleState.TRANSCRIBING
          bubbleView?.let { renderBubble(it) }
        }
        override fun onError(error: Int) {
          stopMicrophoneForeground()
          if (error == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE) {
            AndroidSpeechSupport.markLanguageUnavailable(this@FloatingBubbleService)
          }
          val message = speechErrorMessage(error)
          showFailure(message, null)
          Toast.makeText(this@FloatingBubbleService, message, Toast.LENGTH_SHORT).show()
        }
        override fun onResults(results: Bundle?) {
          stopMicrophoneForeground()
          AndroidSpeechSupport.markLanguageReady(this@FloatingBubbleService)
          val text = results
            ?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
            ?.firstOrNull()
            ?.trim()
          handleRecognizedText(text)
        }
        override fun onPartialResults(partialResults: Bundle?) = Unit
        override fun onEvent(eventType: Int, params: Bundle?) = Unit
      })
    }

    bubbleState = BubbleState.RECORDING
    bubbleView?.let { renderBubble(it) }
    speechRecognizer?.startListening(
      Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
        putExtra(
          RecognizerIntent.EXTRA_LANGUAGE_MODEL,
          RecognizerIntent.LANGUAGE_MODEL_FREE_FORM,
        )
        putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, false)
        putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
        putExtra(RecognizerIntent.EXTRA_LANGUAGE, Locale.getDefault().toLanguageTag())
      },
    )
  }

  private fun isOnDeviceSpeechRecognitionAvailable(): Boolean =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
      SpeechRecognizer.isOnDeviceRecognitionAvailable(this)

  private fun createSpeechRecognizer(): SpeechRecognizer =
    SpeechRecognizer.createOnDeviceSpeechRecognizer(this)

  private fun speechErrorMessage(error: Int): Int =
    when (error) {
      SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE -> R.string.bubble_speech_pack_missing
      SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED -> R.string.bubble_language_unsupported
      SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> R.string.bubble_permissions_needed
      SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> R.string.bubble_recognizer_busy
      SpeechRecognizer.ERROR_SERVER,
      SpeechRecognizer.ERROR_SERVER_DISCONNECTED -> R.string.bubble_speech_missing
      else -> R.string.bubble_listen_failed
    }

  private fun startMicrophoneForeground(): Boolean {
    if (foregroundActive) {
      return true
    }

    return try {
      createNotificationChannel()
      val intent = Intent(this, MainActivity::class.java)
      val pendingIntent = PendingIntent.getActivity(
        this,
        0,
        intent,
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
      )
      val notification = NotificationCompat.Builder(this, NOTIFICATION_CHANNEL_ID)
        .setSmallIcon(R.mipmap.ic_launcher)
        .setContentTitle(getString(R.string.bubble_recording))
        .setContentText(getString(R.string.bubble_notification_text))
        .setContentIntent(pendingIntent)
        .setOngoing(true)
        .setCategory(NotificationCompat.CATEGORY_SERVICE)
        .setPriority(NotificationCompat.PRIORITY_LOW)
        .build()

      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        startForeground(
          FOREGROUND_NOTIFICATION_ID,
          notification,
          ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE,
        )
      } else {
        startForeground(FOREGROUND_NOTIFICATION_ID, notification)
      }
      foregroundActive = true
      true
    } catch (_: RuntimeException) {
      showFailure(R.string.bubble_microphone_blocked, null)
      Toast.makeText(this, R.string.bubble_foreground_failed, Toast.LENGTH_LONG).show()
      false
    }
  }

  private fun stopMicrophoneForeground() {
    if (!foregroundActive) {
      return
    }

    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
      stopForeground(STOP_FOREGROUND_REMOVE)
    } else {
      @Suppress("DEPRECATION")
      stopForeground(true)
    }
    foregroundActive = false
  }

  private fun createNotificationChannel() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
      return
    }

    val channel = NotificationChannel(
      NOTIFICATION_CHANNEL_ID,
      getString(R.string.app_name),
      NotificationManager.IMPORTANCE_LOW,
    ).apply {
      description = getString(R.string.bubble_notification_text)
    }

    val manager = getSystemService(NotificationManager::class.java)
    manager.createNotificationChannel(channel)
  }

  private fun handleRecognizedText(text: String?) {
    if (text.isNullOrBlank()) {
      showFailure(R.string.bubble_listen_failed, null)
      Toast.makeText(this, R.string.bubble_listen_failed, Toast.LENGTH_SHORT).show()
      return
    }

    insertOrRecover(text)
  }

  private fun insertOrRecover(text: String, shouldRecord: Boolean = true) {
    when (VerbatimAccessibilityService.insert(text)) {
      VerbatimAccessibilityService.InsertResult.INSERTED -> {
        if (shouldRecord) {
          recordTranscript(text, HISTORY_STATUS_INSERTED)
        }
        resetToIdle()
        Toast.makeText(this, R.string.bubble_inserted, Toast.LENGTH_SHORT).show()
      }
      VerbatimAccessibilityService.InsertResult.SENSITIVE -> {
        showFailure(R.string.bubble_sensitive_blocked, null)
        Toast.makeText(this, R.string.bubble_sensitive_blocked, Toast.LENGTH_LONG).show()
      }
      VerbatimAccessibilityService.InsertResult.FAILED,
      VerbatimAccessibilityService.InsertResult.NO_TARGET -> {
        copyForRecovery(text)
        if (shouldRecord) {
          recordTranscript(text, HISTORY_STATUS_COPIED)
        }
        showFailure(R.string.bubble_recovery_copied, text)
        Toast.makeText(this, R.string.bubble_copied, Toast.LENGTH_LONG).show()
      }
    }
  }

  private fun insertDebugProbe() {
    if (!BuildConfig.DEBUG) {
      return
    }

    insertOrRecover(DEBUG_INSERTION_TEXT)
  }

  private fun retryRecovery() {
    val text = recoveryText
    if (text.isNullOrBlank()) {
      resetToIdle()
      return
    }
    bubbleState = BubbleState.TRANSCRIBING
    bubbleView?.let { renderBubble(it) }
    insertOrRecover(text, shouldRecord = false)
  }

  private fun showFailure(messageResId: Int, recoverableText: String?) {
    failureMessageResId = messageResId
    recoveryText = recoverableText
    bubbleState = BubbleState.FAILED
    bubbleView?.let { renderBubble(it) }
  }

  private fun resetToIdle() {
    recoveryText = null
    bubbleState = BubbleState.IDLE
    bubbleView?.let { renderBubble(it) }
  }

  private fun copyForRecovery(text: String) {
    val clipboard = getSystemService(CLIPBOARD_SERVICE) as ClipboardManager
    clipboard.setPrimaryClip(
      ClipData.newPlainText(getString(R.string.bubble_clip_label), text),
    )
  }

  private fun recordTranscript(text: String, status: String) {
    try {
      val now = System.currentTimeMillis()
      val entry = JSONObject()
        .put("id", now)
        .put("timestamp", now)
        .put("title", getString(R.string.android_history_title))
        .put("transcription_text", text)
        .put("insertion_status", status)

      val stored = getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
        .getString(ANDROID_HISTORY_KEY, "[]") ?: "[]"
      val existing = JSONArray(stored)
      val next = JSONArray().put(entry)
      val keep = min(existing.length(), ANDROID_HISTORY_LIMIT - 1)
      for (index in 0 until keep) {
        existing.optJSONObject(index)?.let { next.put(it) }
      }

      getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
        .edit()
        .putString(ANDROID_HISTORY_KEY, next.toString())
        .apply()
    } catch (_: Exception) {
      // History is a local convenience path; never surface transcript text in errors.
    }
  }

  private fun hasRequiredPermissions(): Boolean =
      ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) ==
      PackageManager.PERMISSION_GRANTED &&
      Settings.canDrawOverlays(this) &&
      VerbatimAccessibilityService.isEnabled()

  private fun label(
    text: String,
    color: Int,
    sizeSp: Int,
    bold: Boolean,
  ): TextView =
    TextView(this).apply {
      this.text = text
      setTextColor(color)
      textSize = sizeSp.toFloat()
      if (bold) {
        typeface = android.graphics.Typeface.DEFAULT_BOLD
      }
      includeFontPadding = false
    }

  private fun pillBackground(color: String): GradientDrawable =
    GradientDrawable().apply {
      setColor(Color.parseColor(color))
      cornerRadius = dp(999).toFloat()
    }

  private fun dp(value: Int): Int =
    (value * resources.displayMetrics.density).toInt()

  private fun loadCoordinate(key: String, fallback: Int): Int =
    getSharedPreferences(PREFS_NAME, MODE_PRIVATE).getInt(key, fallback)

  private fun saveCoordinate(key: String, value: Int) {
    getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
      .edit()
      .putInt(key, value)
      .apply()
  }

  private enum class BubbleState {
    IDLE,
    RECORDING,
    TRANSCRIBING,
    FAILED,
  }

  companion object {
    private const val PREFS_NAME = "verbatim_android"
    private const val ANDROID_HISTORY_KEY = "native_transcript_history"
    private const val ANDROID_HISTORY_LIMIT = 30
    private const val HISTORY_STATUS_INSERTED = "inserted"
    private const val HISTORY_STATUS_COPIED = "copied"
    private const val NOTIFICATION_CHANNEL_ID = "verbatim_dictation"
    private const val FOREGROUND_NOTIFICATION_ID = 4808
    private const val ACTION_DEBUG_INSERT_PROBE =
      "com.galaxyruler.verbatim.action.DEBUG_INSERT_PROBE"
    private const val ACTION_INPUT_TARGET_ACTIVE =
      "com.galaxyruler.verbatim.action.INPUT_TARGET_ACTIVE"
    private const val ACTION_INPUT_TARGET_INACTIVE =
      "com.galaxyruler.verbatim.action.INPUT_TARGET_INACTIVE"
    private const val DEBUG_INSERTION_TEXT = "Verbatim Android insertion probe"

    @Volatile
    private var inputTargetActive: Boolean = false

    @Volatile
    var isRunning: Boolean = false
      private set

    @Volatile
    var isVisible: Boolean = false
      private set

    fun setInputTargetActive(context: Context, active: Boolean) {
      inputTargetActive = active
      if (!active && !isRunning) {
        return
      }

      val action = if (active) {
        ACTION_INPUT_TARGET_ACTIVE
      } else {
        ACTION_INPUT_TARGET_INACTIVE
      }
      try {
        context.startService(Intent(context, FloatingBubbleService::class.java).apply {
          this.action = action
        })
      } catch (_: IllegalStateException) {
        // Android may reject background service starts; the hidden coordinator
        // will recover on the next app resume or explicit bubble action.
      }
    }

    fun nativeTranscriptHistory(context: Context): String =
      context
        .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        .getString(ANDROID_HISTORY_KEY, "[]") ?: "[]"
  }
}
