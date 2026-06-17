package com.galaxyruler.verbatim

import android.Manifest
import android.app.Service
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
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
import androidx.core.content.ContextCompat
import java.util.Locale
import kotlin.math.abs

class FloatingBubbleService : Service() {
  private var windowManager: WindowManager? = null
  private var bubbleView: LinearLayout? = null
  private var layoutParams: WindowManager.LayoutParams? = null
  private var speechRecognizer: SpeechRecognizer? = null
  private var bubbleState = BubbleState.IDLE

  override fun onCreate() {
    super.onCreate()
    isRunning = true
    windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
    showBubble()
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (bubbleView == null) {
      showBubble()
    }
    return START_STICKY
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onDestroy() {
    speechRecognizer?.destroy()
    speechRecognizer = null
    bubbleView?.let { view ->
      windowManager?.removeView(view)
    }
    bubbleView = null
    windowManager = null
    isRunning = false
    super.onDestroy()
  }

  private fun showBubble() {
    if (!Settings.canDrawOverlays(this) || bubbleView != null) {
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
    }
  }

  private fun renderIdle(view: LinearLayout) {
    view.background = pillBackground("#24202A")
    view.addView(label(getString(R.string.bubble_idle), Color.WHITE, 16, true))
  }

  private fun renderRecording(view: LinearLayout) {
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

  private fun handleBubbleTap() {
    when (bubbleState) {
      BubbleState.IDLE -> startListening()
      BubbleState.RECORDING -> {
        bubbleState = BubbleState.TRANSCRIBING
        bubbleView?.let { renderBubble(it) }
        speechRecognizer?.stopListening()
      }
      BubbleState.TRANSCRIBING -> Unit
    }
  }

  private fun startListening() {
    if (!hasRequiredPermissions()) {
      Toast.makeText(this, R.string.bubble_permission_missing, Toast.LENGTH_LONG).show()
      return
    }

    if (!SpeechRecognizer.isRecognitionAvailable(this)) {
      Toast.makeText(this, R.string.bubble_speech_unavailable, Toast.LENGTH_LONG).show()
      return
    }

    speechRecognizer?.destroy()
    speechRecognizer = SpeechRecognizer.createSpeechRecognizer(this).apply {
      setRecognitionListener(object : RecognitionListener {
        override fun onReadyForSpeech(params: Bundle?) = Unit
        override fun onBeginningOfSpeech() = Unit
        override fun onRmsChanged(rmsdB: Float) = Unit
        override fun onBufferReceived(buffer: ByteArray?) = Unit
        override fun onEndOfSpeech() {
          bubbleState = BubbleState.TRANSCRIBING
          bubbleView?.let { renderBubble(it) }
        }
        override fun onError(error: Int) {
          bubbleState = BubbleState.IDLE
          bubbleView?.let { renderBubble(it) }
          Toast.makeText(this@FloatingBubbleService, R.string.bubble_listen_failed, Toast.LENGTH_SHORT).show()
        }
        override fun onResults(results: Bundle?) {
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
        putExtra(RecognizerIntent.EXTRA_LANGUAGE, Locale.getDefault().toLanguageTag())
      },
    )
  }

  private fun handleRecognizedText(text: String?) {
    bubbleState = BubbleState.IDLE
    bubbleView?.let { renderBubble(it) }
    if (text.isNullOrBlank()) {
      Toast.makeText(this, R.string.bubble_listen_failed, Toast.LENGTH_SHORT).show()
      return
    }

    when (VerbatimAccessibilityService.insert(text)) {
      VerbatimAccessibilityService.InsertResult.INSERTED -> {
        Toast.makeText(this, R.string.bubble_inserted, Toast.LENGTH_SHORT).show()
      }
      VerbatimAccessibilityService.InsertResult.SENSITIVE -> {
        Toast.makeText(this, R.string.bubble_sensitive_blocked, Toast.LENGTH_LONG).show()
      }
      VerbatimAccessibilityService.InsertResult.FAILED,
      VerbatimAccessibilityService.InsertResult.NO_TARGET -> {
        copyForRecovery(text)
        Toast.makeText(this, R.string.bubble_copied, Toast.LENGTH_LONG).show()
      }
    }
  }

  private fun copyForRecovery(text: String) {
    val clipboard = getSystemService(CLIPBOARD_SERVICE) as ClipboardManager
    clipboard.setPrimaryClip(
      ClipData.newPlainText(getString(R.string.bubble_clip_label), text),
    )
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
    getSharedPreferences("verbatim_android", MODE_PRIVATE).getInt(key, fallback)

  private fun saveCoordinate(key: String, value: Int) {
    getSharedPreferences("verbatim_android", MODE_PRIVATE)
      .edit()
      .putInt(key, value)
      .apply()
  }

  private enum class BubbleState {
    IDLE,
    RECORDING,
    TRANSCRIBING,
  }

  companion object {
    @Volatile
    var isRunning: Boolean = false
      private set
  }
}
