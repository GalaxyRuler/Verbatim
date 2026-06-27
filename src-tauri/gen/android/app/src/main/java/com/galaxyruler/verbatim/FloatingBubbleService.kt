package com.galaxyruler.verbatim

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PixelFormat
import android.graphics.RectF
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.provider.Settings
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.util.Log
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowManager
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import androidx.core.view.AccessibilityDelegateCompat
import androidx.core.view.ViewCompat
import androidx.core.view.accessibility.AccessibilityNodeInfoCompat
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileInputStream
import java.util.Locale
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

class FloatingBubbleService : Service() {
  private var windowManager: WindowManager? = null
  private var bubbleView: LinearLayout? = null
  private var layoutParams: WindowManager.LayoutParams? = null
  private var speechRecognizer: SpeechRecognizer? = null
  private var bubbleState = BubbleState.IDLE
  private var foregroundActive = false
  private var recoveryText: String? = null
  private var livePartialText: String? = null
  private var engineCaptureThread: Thread? = null
  private val engineRecording = AtomicBoolean(false)
  private var failureMessageResId = R.string.bubble_failed
  private val mainHandler = Handler(Looper.getMainLooper())

  override fun onCreate() {
    super.onCreate()
    instance = this
    isRunning = true
    windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
    updateBubbleVisibility()
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (BuildConfig.DEBUG && intent?.action == ACTION_DEBUG_INSERT_PROBE) {
      insertDebugProbe()
      return START_STICKY
    }
    if (BuildConfig.DEBUG && intent?.action == ACTION_DEBUG_ENGINE_WAV_SMOKE) {
      inputTargetActive = true
      updateBubbleVisibility()
      startDebugEngineWavSmoke(intent.getStringExtra(EXTRA_DEBUG_WAV_PATH))
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
    stopEngineCapture()
    stopMicrophoneForeground()
    bubbleView?.let { view ->
      windowManager?.removeView(view)
    }
    bubbleView = null
    windowManager = null
    isVisible = false
    isRunning = false
    if (instance === this) {
      instance = null
    }
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
      bubbleWindowType(),
      WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE,
      PixelFormat.TRANSLUCENT,
    ).apply {
      gravity = Gravity.TOP or Gravity.START
      x = loadCoordinate(BUBBLE_X_KEY, dp(20))
      y = loadCoordinate(BUBBLE_Y_KEY, dp(140))
    }

    if (addBubbleView(view, params)) {
      layoutParams = params
      bubbleView = view
      isVisible = true
    } else {
      layoutParams = null
      bubbleView = null
      isVisible = false
    }
  }

  private fun bubbleWindowType(): Int =
    if (VerbatimAccessibilityService.isEnabled()) {
      WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY
    } else {
      WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
    }

  private fun addBubbleView(view: View, params: WindowManager.LayoutParams): Boolean {
    val manager = windowManager ?: return false
    return try {
      manager.addView(view, params)
      true
    } catch (_: RuntimeException) {
      if (params.type != WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY) {
        return false
      }

      params.type = WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY
      try {
        manager.addView(view, params)
        true
      } catch (_: RuntimeException) {
        false
      }
    }
  }

  private fun hideBubble() {
    if (bubbleView == null) {
      isVisible = false
      return
    }

    mainHandler.removeCallbacksAndMessages(null)
    speechRecognizer?.cancel()
    speechRecognizer?.destroy()
    speechRecognizer = null
    stopEngineCapture()
    stopMicrophoneForeground()
    recoveryText = null
    livePartialText = null
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
      background = pillBackground("#24202A")
      contentDescription = getString(R.string.bubble_idle)
      elevation = dp(8).toFloat()
      // TalkBack: focus the bubble as one node and expose a click action so screen-reader
      // users can drive it without the press-and-hold gesture.
      isFocusable = true
      isClickable = true
      importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
    }

    ViewCompat.setAccessibilityDelegate(
      view,
      object : AccessibilityDelegateCompat() {
        override fun onInitializeAccessibilityNodeInfo(
          host: View,
          info: AccessibilityNodeInfoCompat,
        ) {
          super.onInitializeAccessibilityNodeInfo(host, info)
          info.addAction(
            AccessibilityNodeInfoCompat.AccessibilityActionCompat(
              AccessibilityNodeInfoCompat.ACTION_CLICK,
              bubbleActionLabel(),
            ),
          )
        }

        override fun performAccessibilityAction(host: View, action: Int, args: Bundle?): Boolean {
          if (action == AccessibilityNodeInfoCompat.ACTION_CLICK) {
            handleBubbleTap()
            return true
          }
          return super.performAccessibilityAction(host, action, args)
        }
      },
    )

    renderBubble(view)
    installDragHandler(view)
    return view
  }

  /** Label for the TalkBack click action, announced as "double tap to <label>" for each state. */
  private fun bubbleActionLabel(): CharSequence = when (bubbleState) {
    BubbleState.IDLE -> getString(R.string.bubble_idle)
    BubbleState.RECORDING -> getString(R.string.bubble_stop)
    BubbleState.INSERTED -> getString(R.string.bubble_dismiss)
    BubbleState.FAILED ->
      getString(
        if (recoveryText.isNullOrBlank()) R.string.bubble_dismiss else R.string.bubble_retry_insert,
      )
    BubbleState.TRANSCRIBING -> getString(R.string.bubble_transcribing)
  }

  private fun installDragHandler(view: View) {
    var downRawX = 0f
    var downRawY = 0f
    var startX = 0
    var startY = 0
    var dragging = false
    var longPressActive = false
    var longPressRunnable: Runnable? = null

    view.setOnTouchListener { _, event ->
      val params = layoutParams ?: return@setOnTouchListener false
      when (event.actionMasked) {
        MotionEvent.ACTION_DOWN -> {
          downRawX = event.rawX
          downRawY = event.rawY
          startX = params.x
          startY = params.y
          dragging = false
          longPressActive = false
          longPressRunnable = Runnable {
            if (!dragging && bubbleState == BubbleState.IDLE) {
              longPressActive = true
              startListening()
            }
          }.also { runnable ->
            mainHandler.postDelayed(
              runnable,
              ViewConfiguration.getLongPressTimeout().toLong(),
            )
          }
          true
        }
        MotionEvent.ACTION_MOVE -> {
          val deltaX = event.rawX - downRawX
          val deltaY = event.rawY - downRawY
          dragging = dragging || abs(deltaX) > dp(6) || abs(deltaY) > dp(6)
          if (dragging) {
            longPressRunnable?.let(mainHandler::removeCallbacks)
            longPressRunnable = null
          }
          params.x = startX + deltaX.toInt()
          params.y = startY + deltaY.toInt()
          windowManager?.updateViewLayout(view, params)
          true
        }
        MotionEvent.ACTION_UP -> {
          longPressRunnable?.let(mainHandler::removeCallbacks)
          longPressRunnable = null
          saveCoordinate(BUBBLE_X_KEY, params.x)
          saveCoordinate(BUBBLE_Y_KEY, params.y)
          saveBubbleCorner(nearestCorner(params.x, params.y))
          if (longPressActive) {
            if (bubbleState == BubbleState.RECORDING) {
              bubbleState = BubbleState.TRANSCRIBING
              bubbleView?.let { renderBubble(it) }
              speechRecognizer?.stopListening()
            }
          } else if (!dragging) {
            handleBubbleTap()
          }
          true
        }
        MotionEvent.ACTION_CANCEL -> {
          longPressRunnable?.let(mainHandler::removeCallbacks)
          longPressRunnable = null
          if (longPressActive && bubbleState == BubbleState.RECORDING) {
            speechRecognizer?.cancel()
            stopMicrophoneForeground()
            resetToIdle()
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
      BubbleState.INSERTED -> renderInserted(view)
      BubbleState.FAILED -> renderFailed(view)
    }
    // The inner labels/bars/dots are decorative; TalkBack should announce only the bubble's
    // contentDescription + click action, not each child.
    for (index in 0 until view.childCount) {
      view.getChildAt(index).importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
    }
  }

  private fun renderIdle(view: LinearLayout) {
    view.contentDescription = getString(R.string.bubble_idle)
    view.background = roundedBackground("#2563EB", 15)
    view.minimumWidth = dp(52)
    view.minimumHeight = dp(52)
    view.setPadding(dp(11), dp(11), dp(11), dp(11))
    view.addView(
      VerbatimBubbleIconView(this),
      LinearLayout.LayoutParams(dp(30), dp(30)),
    )
  }

  private fun renderRecording(view: LinearLayout) {
    view.contentDescription = getString(R.string.bubble_recording)
    view.minimumWidth = 0
    view.minimumHeight = 0
    view.setPadding(dp(16), dp(10), dp(10), dp(10))
    view.background = pillBackground("#3F1010")
    view.addView(
      label(
        livePartialText?.takeIf { it.isNotBlank() } ?: getString(R.string.bubble_recording),
        Color.WHITE,
        14,
        true,
      ),
    )
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
    view.minimumWidth = 0
    view.minimumHeight = 0
    view.setPadding(dp(16), dp(10), dp(10), dp(10))
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

  private fun renderInserted(view: LinearLayout) {
    view.contentDescription = getString(R.string.bubble_inserted_short)
    view.minimumWidth = 0
    view.minimumHeight = 0
    view.setPadding(dp(16), dp(10), dp(10), dp(10))
    view.background = pillBackground("#133B1E")
    view.addView(label(getString(R.string.bubble_inserted_short), Color.WHITE, 14, true))
  }

  private fun renderFailed(view: LinearLayout) {
    view.contentDescription = getString(failureMessageResId)
    view.minimumWidth = 0
    view.minimumHeight = 0
    view.setPadding(dp(16), dp(10), dp(10), dp(10))
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
        if (engineRecording.get()) {
          stopEngineDictation()
        } else {
          speechRecognizer?.stopListening()
        }
      }
      BubbleState.TRANSCRIBING -> Unit
      BubbleState.INSERTED -> resetToIdle()
      BubbleState.FAILED -> retryRecovery()
    }
  }

  private fun startListening() {
    if (!hasRequiredPermissions()) {
      showFailure(R.string.bubble_permissions_needed, null)
      Toast.makeText(this, R.string.bubble_permission_missing, Toast.LENGTH_LONG).show()
      return
    }

    if (isEngineDictationEnabled() && isEngineModelInstalled()) {
      startEngineDictation()
      return
    }
    if (isEngineDictationEnabled()) {
      Toast.makeText(this, R.string.bubble_asr_model_missing, Toast.LENGTH_SHORT).show()
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

    livePartialText = null
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

  private fun startEngineDictation() {
    if (!startMicrophoneForeground()) {
      return
    }

    livePartialText = null
    val listener = createEngineListener()

    val modelDir = engineModelDir()
    val lang = Locale.getDefault().language.ifBlank { "en" }
    logAsr("nativeAsrStart called lang=$lang modelDir=$modelDir")
    if (!nativeAsrStart(modelDir, lang, listener)) {
      logAsr("nativeAsrStart returned false")
      stopMicrophoneForeground()
      showFailure(R.string.bubble_listen_failed, null)
      return
    }
    logAsr("nativeAsrStart returned true")

    engineRecording.set(true)
    bubbleState = BubbleState.RECORDING
    bubbleView?.let { renderBubble(it) }
    engineCaptureThread = Thread({ runEngineCaptureLoop() }, "verbatim-asr-capture").apply {
      isDaemon = true
      start()
    }
  }

  private fun createEngineListener(): AsrEngineListener =
    object : AsrEngineListener {
      override fun onAsrPartial(text: String) {
        logAsr("onPartial callback len=${text.length}")
        mainHandler.post {
          livePartialText = text.trim().takeIf { it.isNotBlank() }
          if (bubbleState == BubbleState.RECORDING) {
            bubbleView?.let { renderBubble(it) }
            logBubble("partial text rendered in bubble")
          }
        }
      }

      override fun onAsrFinal(text: String) {
        logAsr("onFinal callback len=${text.length}")
        mainHandler.post {
          stopMicrophoneForeground()
          engineRecording.set(false)
          handleRecognizedText(text)
        }
      }

      override fun onAsrError(message: String) {
        logAsr("onError callback len=${message.length}")
        mainHandler.post {
          stopEngineCapture()
          stopMicrophoneForeground()
          showFailure(R.string.bubble_listen_failed, null)
          Toast
            .makeText(this@FloatingBubbleService, R.string.bubble_listen_failed, Toast.LENGTH_SHORT)
            .show()
        }
      }
    }

  private fun startDebugEngineWavSmoke(wavPath: String?) {
    if (wavPath.isNullOrBlank()) {
      logAsr("debug WAV smoke missing wav path")
      return
    }

    if (!startMicrophoneForeground()) {
      return
    }

    livePartialText = null
    val listener = createEngineListener()
    val modelDir = engineModelDir()
    val lang = Locale.getDefault().language.ifBlank { "en" }
    logAsr("nativeAsrStart called lang=$lang modelDir=$modelDir debugWav=$wavPath")
    if (!nativeAsrStart(modelDir, lang, listener)) {
      logAsr("nativeAsrStart returned false")
      stopMicrophoneForeground()
      showFailure(R.string.bubble_listen_failed, null)
      return
    }
    logAsr("nativeAsrStart returned true")

    engineRecording.set(true)
    bubbleState = BubbleState.RECORDING
    bubbleView?.let { renderBubble(it) }
    engineCaptureThread = Thread({ runDebugWavFeedLoop(wavPath) }, "verbatim-asr-debug-wav").apply {
      isDaemon = true
      start()
    }
  }

  private fun stopEngineDictation() {
    stopEngineCapture()
    livePartialText = null
    logAsr("nativeAsrStop called")
    if (!nativeAsrStop()) {
      logAsr("nativeAsrStop returned false")
      stopMicrophoneForeground()
      showFailure(R.string.bubble_listen_failed, null)
    } else {
      logAsr("nativeAsrStop returned true")
    }
  }

  private fun stopEngineCapture() {
    engineRecording.set(false)
    engineCaptureThread?.interrupt()
    engineCaptureThread = null
  }

  private fun runEngineCaptureLoop() {
    var recorder: AudioRecord? = null
    try {
      val minBufferSize = AudioRecord.getMinBufferSize(
        PcmFrameNormalizer.SAMPLE_RATE,
        AudioFormat.CHANNEL_IN_MONO,
        AudioFormat.ENCODING_PCM_16BIT,
      )
      val bufferSize = max(minBufferSize, PcmFrameNormalizer.FRAME_SIZE * 2 * 4)
      recorder = AudioRecord(
        MediaRecorder.AudioSource.VOICE_RECOGNITION,
        PcmFrameNormalizer.SAMPLE_RATE,
        AudioFormat.CHANNEL_IN_MONO,
        AudioFormat.ENCODING_PCM_16BIT,
        bufferSize,
      )

      recorder.startRecording()
      val buffer = ByteArray(PcmFrameNormalizer.FRAME_SIZE * 2)
      while (engineRecording.get() && !Thread.currentThread().isInterrupted) {
        val read = recorder.read(buffer, 0, buffer.size)
        if (read > 0) {
          PcmFrameNormalizer.pcm16LeToFloatFrames(buffer.copyOf(read)).forEach { frame ->
            if (!nativeAsrFeedPcm(frame)) {
              engineRecording.set(false)
            }
          }
        }
      }
    } catch (_: SecurityException) {
      mainHandler.post {
        stopMicrophoneForeground()
        showFailure(R.string.bubble_permissions_needed, null)
      }
    } catch (_: RuntimeException) {
      mainHandler.post {
        stopMicrophoneForeground()
        showFailure(R.string.bubble_listen_failed, null)
      }
    } finally {
      try {
        recorder?.stop()
      } catch (_: RuntimeException) {
      }
      recorder?.release()
    }
  }

  private fun runDebugWavFeedLoop(wavPath: String) {
    try {
      FileInputStream(File(wavPath)).use { input ->
        val skipped = input.skip(WAV_HEADER_BYTES)
        if (skipped < WAV_HEADER_BYTES) {
          throw IllegalArgumentException("invalid WAV header")
        }

        val buffer = ByteArray(PcmFrameNormalizer.FRAME_SIZE * 2 * 4)
        while (engineRecording.get() && !Thread.currentThread().isInterrupted) {
          val read = input.read(buffer)
          if (read <= 0) {
            break
          }

          PcmFrameNormalizer.pcm16LeToFloatFrames(buffer.copyOf(read)).forEach { frame ->
            if (!nativeAsrFeedPcm(frame)) {
              engineRecording.set(false)
              return@forEach
            }
            Thread.sleep(DEBUG_WAV_FRAME_SLEEP_MS)
          }
        }
      }
      mainHandler.post {
        bubbleState = BubbleState.TRANSCRIBING
        bubbleView?.let { renderBubble(it) }
      }
      logAsr("nativeAsrStop called")
      if (!nativeAsrStop()) {
        logAsr("nativeAsrStop returned false")
      } else {
        logAsr("nativeAsrStop returned true")
      }
    } catch (error: Exception) {
      logAsr("debug WAV feed failed type=${error.javaClass.simpleName}")
      mainHandler.post {
        stopMicrophoneForeground()
        showFailure(R.string.bubble_listen_failed, null)
      }
    } finally {
      engineRecording.set(false)
    }
  }

  private fun isEngineDictationEnabled(): Boolean =
    getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
      .getBoolean(ENGINE_DICTATION_ENABLED_KEY, false)

  private fun engineModelDir(): String {
    val modelId = engineModelId(this)
    val modelPath = File(modelId)
    if (modelPath.isAbsolute) {
      return modelPath.absolutePath
    }

    return File(filesDir, "models/android-asr/$modelId").absolutePath
  }

  private fun isEngineModelInstalled(): Boolean {
    val modelDir = File(engineModelDir())
    return REQUIRED_ENGINE_MODEL_FILES.all { relativePath ->
      File(modelDir, relativePath).isFile
    }
  }

  private fun logAsr(message: String) {
    if (BuildConfig.DEBUG) {
      Log.i(ASR_LOG_TAG, message)
    }
  }

  private fun logBubble(message: String) {
    if (BuildConfig.DEBUG) {
      Log.i(BUBBLE_LOG_TAG, message)
    }
  }

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
    val rawText = text?.trim()
    if (rawText.isNullOrBlank()) {
      showFailure(R.string.bubble_listen_failed, null)
      Toast.makeText(this, R.string.bubble_listen_failed, Toast.LENGTH_SHORT).show()
      return
    }

    insertOrRecover(applyNativeTextFormatter(rawText), rawText)
  }

  private fun insertOrRecover(
    text: String,
    rawText: String = text,
    shouldRecord: Boolean = true,
  ) {
    when (VerbatimAccessibilityService.insert(text)) {
      VerbatimAccessibilityService.InsertResult.INSERTED -> {
        if (shouldRecord) {
          recordTranscript(rawText, text, HISTORY_STATUS_INSERTED)
        }
        showInserted()
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
          recordTranscript(rawText, text, HISTORY_STATUS_COPIED)
        }
        showFailure(R.string.bubble_recovery_copied, text)
        Toast.makeText(this, R.string.bubble_copied, Toast.LENGTH_LONG).show()
      }
    }
  }

  private fun showInserted() {
    recoveryText = null
    bubbleState = BubbleState.INSERTED
    bubbleView?.let { renderBubble(it) }
    mainHandler.postDelayed(
      {
        if (bubbleState == BubbleState.INSERTED) {
          resetToIdle()
        }
      },
      INSERTED_STATE_MS,
    )
  }

  private fun insertDebugProbe() {
    if (!BuildConfig.DEBUG) {
      return
    }

    insertOrRecover(applyNativeTextFormatter(DEBUG_INSERTION_TEXT), DEBUG_INSERTION_TEXT)
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

  private fun applyNativeTextFormatter(text: String): String {
    val snapshot = getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
      .getString(TEXT_FORMATTER_KEY, null)
      ?: return text

    return try {
      val config = JSONObject(snapshot)
      val dictionaryRules = mutableListOf<ReplacementRule>()
      val dictionaryEntries = config.optJSONArray("dictionary_entries")
      if (dictionaryEntries != null) {
        for (index in 0 until dictionaryEntries.length()) {
          val entry = dictionaryEntries.optJSONObject(index) ?: continue
          val from = entry.optString("replacement_of").trim()
          val to = entry.optString("phrase").trim()
          if (from.isNotBlank() && to.isNotBlank()) {
            dictionaryRules.add(ReplacementRule(from, to))
          }
        }
      }

      val snippetRules = mutableListOf<ReplacementRule>()
      val snippets = config.optJSONArray("snippets")
      if (snippets != null) {
        for (index in 0 until snippets.length()) {
          val entry = snippets.optJSONObject(index) ?: continue
          val from = entry.optString("trigger").trim()
          val to = entry.optString("content")
          if (from.isNotBlank() && to.isNotBlank()) {
            snippetRules.add(ReplacementRule(from, to))
          }
        }
      }

      applyReplacementRules(
        applyReplacementRules(text, dictionaryRules),
        snippetRules,
      )
    } catch (_: Exception) {
      text
    }
  }

  private fun applyReplacementRules(
    text: String,
    rules: List<ReplacementRule>,
  ): String =
    rules
      .sortedByDescending { it.from.length }
      .fold(text) { current, rule ->
        applyBoundedReplacement(current, rule.from, rule.to)
      }

  private fun applyBoundedReplacement(text: String, from: String, to: String): String {
    if (from.isEmpty()) {
      return text
    }

    val output = StringBuilder()
    var cursor = 0
    while (cursor < text.length) {
      val matchStart = text.indexOf(from, cursor, ignoreCase = true)
      if (matchStart < 0) {
        output.append(text.substring(cursor))
        break
      }

      val matchEnd = matchStart + from.length
      if (isRuleBoundary(text, matchStart - 1) && isRuleBoundary(text, matchEnd)) {
        output.append(text.substring(cursor, matchStart))
        output.append(to)
      } else {
        output.append(text.substring(cursor, matchEnd))
      }
      cursor = matchEnd
    }

    return output.toString()
  }

  private fun isRuleBoundary(text: String, index: Int): Boolean =
    index < 0 ||
      index >= text.length ||
      (!text[index].isLetterOrDigit() && text[index] != '_')

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

  private fun recordTranscript(rawText: String, insertedText: String, status: String) {
    try {
      val now = System.currentTimeMillis()
      val entry = JSONObject()
        .put("id", now)
        .put("timestamp", now)
        .put("title", getString(R.string.android_history_title))
        .put("transcription_text", rawText)
        .put(
          "post_processed_text",
          if (insertedText == rawText) JSONObject.NULL else insertedText,
        )
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

  private fun roundedBackground(color: String, radiusDp: Int): GradientDrawable =
    GradientDrawable().apply {
      setColor(Color.parseColor(color))
      cornerRadius = dp(radiusDp).toFloat()
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

  private fun moveBubbleToCorner(corner: String) {
    val view = bubbleView ?: return
    val params = layoutParams ?: return
    val (nextX, nextY) = coordinatesForCorner(this, corner)
    params.x = nextX
    params.y = nextY
    windowManager?.updateViewLayout(view, params)
  }

  private fun saveBubbleCorner(corner: String) {
    getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
      .edit()
      .putString(BUBBLE_CORNER_KEY, corner)
      .apply()
  }

  private fun nearestCorner(x: Int, y: Int): String {
    val metrics = resources.displayMetrics
    val horizontal = if (x < metrics.widthPixels / 2) "left" else "right"
    val vertical = if (y < metrics.heightPixels / 2) "top" else "bottom"
    return "$vertical-$horizontal"
  }

  private enum class BubbleState {
    IDLE,
    RECORDING,
    TRANSCRIBING,
    INSERTED,
    FAILED,
  }

  private data class ReplacementRule(
    val from: String,
    val to: String,
  )

  private external fun nativeAsrStart(
    modelDir: String,
    lang: String,
    listener: AsrEngineListener,
  ): Boolean

  private external fun nativeAsrFeedPcm(frames: FloatArray): Boolean

  private external fun nativeAsrStop(): Boolean

  private interface AsrEngineListener {
    fun onAsrPartial(text: String)
    fun onAsrFinal(text: String)
    fun onAsrError(message: String)
  }

  private class VerbatimBubbleIconView(context: Context) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      color = Color.WHITE
      style = Paint.Style.FILL
    }
    private val barPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
      color = Color.parseColor("#2563EB")
      style = Paint.Style.FILL
    }
    private val bubblePath = Path().apply {
      moveTo(30f, 32f)
      lineTo(76f, 32f)
      cubicTo(85f, 32f, 91f, 39f, 91f, 47f)
      lineTo(91f, 60f)
      cubicTo(91f, 69f, 85f, 76f, 76f, 76f)
      lineTo(57f, 76f)
      lineTo(42f, 86f)
      cubicTo(39f, 88f, 35f, 86f, 35f, 82f)
      lineTo(35f, 76f)
      lineTo(30f, 76f)
      cubicTo(21f, 76f, 15f, 69f, 15f, 60f)
      lineTo(15f, 47f)
      cubicTo(15f, 39f, 21f, 32f, 30f, 32f)
      close()
    }

    override fun onDraw(canvas: Canvas) {
      super.onDraw(canvas)
      val scale = min(width / MARK_WIDTH, height / MARK_HEIGHT)
      if (scale <= 0f) {
        return
      }

      canvas.save()
      canvas.translate(
        (width - MARK_WIDTH * scale) / 2f,
        (height - MARK_HEIGHT * scale) / 2f,
      )
      canvas.scale(scale, scale)

      canvas.drawPath(bubblePath, paint)
      drawBar(canvas, 37f, 50f, 5f, 10f, 2.5f)
      drawBar(canvas, 46f, 44f, 5f, 22f, 2.5f)
      drawBar(canvas, 56f, 39f, 5f, 32f, 2.5f)
      drawBar(canvas, 65f, 44f, 5f, 22f, 2.5f)
      drawBar(canvas, 75f, 49f, 5f, 14f, 2.5f)

      canvas.restore()
    }

    private fun drawBar(
      canvas: Canvas,
      x: Float,
      y: Float,
      width: Float,
      height: Float,
      radius: Float,
    ) {
      canvas.drawRoundRect(
        RectF(x, y, x + width, y + height),
        radius,
        radius,
        barPaint,
      )
    }

    companion object {
      private const val MARK_WIDTH = 108f
      private const val MARK_HEIGHT = 108f
    }
  }

  companion object {
    private const val PREFS_NAME = "verbatim_android"
    private const val ANDROID_HISTORY_KEY = "native_transcript_history"
    private const val TEXT_FORMATTER_KEY = "native_text_formatter_snapshot"
    private const val ENGINE_DICTATION_ENABLED_KEY = "native_engine_dictation_enabled"
    private val REQUIRED_ENGINE_MODEL_FILES = arrayOf(
      "streaming/encoder.onnx",
      "streaming/decoder.onnx",
      "streaming/joiner.onnx",
      "streaming/tokens.txt",
      "whisper/encoder.onnx",
      "whisper/decoder.onnx",
      "whisper/tokens.txt",
      "silero_vad_v4.onnx",
    )
    private const val ASR_LOG_TAG = "VerbatimASR"
    private const val BUBBLE_LOG_TAG = "FloatingBubble"
    private const val BUBBLE_X_KEY = "bubble_x"
    private const val BUBBLE_Y_KEY = "bubble_y"
    private const val BUBBLE_CORNER_KEY = "bubble_corner"
    private const val CORNER_TOP_LEFT = "top-left"
    private const val CORNER_TOP_RIGHT = "top-right"
    private const val CORNER_BOTTOM_LEFT = "bottom-left"
    private const val CORNER_BOTTOM_RIGHT = "bottom-right"
    private const val ANDROID_HISTORY_LIMIT = 30
    private const val MAX_TEXT_FORMATTER_SNAPSHOT_CHARS = 256 * 1024
    private const val INSERTED_STATE_MS = 1800L
    private const val HISTORY_STATUS_INSERTED = "inserted"
    private const val HISTORY_STATUS_COPIED = "copied"
    private const val NOTIFICATION_CHANNEL_ID = "verbatim_dictation"
    private const val FOREGROUND_NOTIFICATION_ID = 4808
    private const val ACTION_DEBUG_INSERT_PROBE =
      "com.galaxyruler.verbatim.action.DEBUG_INSERT_PROBE"
    private const val ACTION_DEBUG_ENGINE_WAV_SMOKE =
      "com.galaxyruler.verbatim.action.DEBUG_ENGINE_WAV_SMOKE"
    private const val EXTRA_DEBUG_WAV_PATH = "wav_path"
    private const val ACTION_INPUT_TARGET_ACTIVE =
      "com.galaxyruler.verbatim.action.INPUT_TARGET_ACTIVE"
    private const val ACTION_INPUT_TARGET_INACTIVE =
      "com.galaxyruler.verbatim.action.INPUT_TARGET_INACTIVE"
    private const val DEBUG_INSERTION_TEXT = "Verbatim Android insertion probe"
    private const val WAV_HEADER_BYTES = 44L
    private const val DEBUG_WAV_FRAME_SLEEP_MS = 10L

    init {
      System.loadLibrary("verbatim_app_lib")
    }

    @Volatile
    private var instance: FloatingBubbleService? = null

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

    /** Remove the native history entry with the given id; returns the updated history JSON. */
    fun deleteNativeHistoryEntry(context: Context, id: Long): String {
      val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      val stored = prefs.getString(ANDROID_HISTORY_KEY, "[]") ?: "[]"
      return try {
        val existing = JSONArray(stored)
        val next = JSONArray()
        for (index in 0 until existing.length()) {
          val item = existing.optJSONObject(index) ?: continue
          if (item.optLong("id", Long.MIN_VALUE) != id) {
            next.put(item)
          }
        }
        val result = next.toString()
        prefs.edit().putString(ANDROID_HISTORY_KEY, result).apply()
        result
      } catch (_: Exception) {
        stored
      }
    }

    fun syncTextFormatter(context: Context, snapshot: String) {
      if (snapshot.length > MAX_TEXT_FORMATTER_SNAPSHOT_CHARS) {
        return
      }

      try {
        JSONObject(snapshot)
        context
          .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
          .edit()
          .putString(TEXT_FORMATTER_KEY, snapshot)
          .apply()
      } catch (_: Exception) {
        // The formatter snapshot is optional and must never expose transcript text in logs.
      }
    }

    fun isEngineDictationEnabled(context: Context): Boolean =
      context
        .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        .getBoolean(ENGINE_DICTATION_ENABLED_KEY, false)

    fun setEngineDictationEnabled(context: Context, enabled: Boolean): Boolean {
      context
        .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        .edit()
        .putBoolean(ENGINE_DICTATION_ENABLED_KEY, enabled)
        .apply()
      return enabled
    }

    fun engineModelId(context: Context): String = EngineModelSelectionStore.engineModelId(context)

    fun setEngineModelId(context: Context, modelId: String): String =
      EngineModelSelectionStore.setEngineModelId(context, modelId)

    fun bubbleCornerSnapshot(context: Context): String {
      val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      val savedCorner = prefs.getString(BUBBLE_CORNER_KEY, null)
      if (savedCorner != null) {
        return normalizeCorner(savedCorner)
      }

      val defaultX = dp(context, 20)
      val defaultY = dp(context, 140)
      val x = prefs.getInt(BUBBLE_X_KEY, defaultX)
      val y = prefs.getInt(BUBBLE_Y_KEY, defaultY)
      return nearestCorner(context, x, y)
    }

    fun setBubbleCorner(context: Context, corner: String): String {
      val normalized = normalizeCorner(corner)
      val (x, y) = coordinatesForCorner(context, normalized)
      context
        .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        .edit()
        .putString(BUBBLE_CORNER_KEY, normalized)
        .putInt(BUBBLE_X_KEY, x)
        .putInt(BUBBLE_Y_KEY, y)
        .apply()

      instance?.mainHandler?.post {
        instance?.moveBubbleToCorner(normalized)
      }
      return normalized
    }

    fun startDebugInsertionProbe(context: Context) {
      if (!BuildConfig.DEBUG) {
        return
      }

      instance?.insertDebugProbe() ?: try {
        context.startService(Intent(context, FloatingBubbleService::class.java).apply {
          action = ACTION_DEBUG_INSERT_PROBE
        })
      } catch (_: IllegalStateException) {
        // Debug-only adb hook; production builds do not register its receiver.
      }
    }

    fun startDebugEngineWavSmoke(context: Context, wavPath: String?) {
      if (!BuildConfig.DEBUG) {
        return
      }

      try {
        context.startService(Intent(context, FloatingBubbleService::class.java).apply {
          action = ACTION_DEBUG_ENGINE_WAV_SMOKE
          putExtra(EXTRA_DEBUG_WAV_PATH, wavPath)
        })
      } catch (_: IllegalStateException) {
        // Debug-only adb hook; production builds do not register its receiver.
      }
    }

    private fun normalizeCorner(corner: String): String =
      when (corner) {
        CORNER_TOP_LEFT,
        CORNER_TOP_RIGHT,
        CORNER_BOTTOM_LEFT,
        CORNER_BOTTOM_RIGHT -> corner
        else -> CORNER_TOP_RIGHT
      }

    private fun coordinatesForCorner(context: Context, corner: String): Pair<Int, Int> {
      val metrics = context.resources.displayMetrics
      val margin = dp(context, 20)
      val topOffset = dp(context, 140)
      val bottomOffset = dp(context, 220)
      val estimatedBubbleWidth = dp(context, 180)
      val estimatedBubbleHeight = dp(context, 64)
      val x = when (corner) {
        CORNER_TOP_RIGHT,
        CORNER_BOTTOM_RIGHT -> max(margin, metrics.widthPixels - estimatedBubbleWidth - margin)
        else -> margin
      }
      val y = when (corner) {
        CORNER_BOTTOM_LEFT,
        CORNER_BOTTOM_RIGHT -> max(topOffset, metrics.heightPixels - estimatedBubbleHeight - bottomOffset)
        else -> topOffset
      }
      return Pair(x, y)
    }

    private fun nearestCorner(context: Context, x: Int, y: Int): String {
      val metrics = context.resources.displayMetrics
      val horizontal = if (x < metrics.widthPixels / 2) "left" else "right"
      val vertical = if (y < metrics.heightPixels / 2) "top" else "bottom"
      return "$vertical-$horizontal"
    }

    private fun dp(context: Context, value: Int): Int =
      (value * context.resources.displayMetrics.density).toInt()
  }
}
