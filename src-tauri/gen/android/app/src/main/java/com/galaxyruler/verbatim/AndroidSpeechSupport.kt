package com.galaxyruler.verbatim

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.speech.ModelDownloadListener
import android.speech.RecognitionSupport
import android.speech.RecognitionSupportCallback
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.core.content.ContextCompat
import java.util.Locale

object AndroidSpeechSupport {
  const val STATUS_UNKNOWN = "unknown"
  const val STATUS_READY = "ready"
  const val STATUS_MISSING = "missing"
  const val STATUS_PENDING = "pending"
  const val STATUS_DOWNLOADING = "downloading"
  const val STATUS_ERROR = "error"
  const val STATUS_UNSUPPORTED = "unsupported"

  private const val PREFS_NAME = "verbatim_android"
  private const val SPEECH_MODEL_STATUS_KEY = "on_device_speech_model_status"

  private val mainHandler = Handler(Looper.getMainLooper())

  fun currentStatus(context: Context): String =
    context
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .getString(SPEECH_MODEL_STATUS_KEY, STATUS_UNKNOWN) ?: STATUS_UNKNOWN

  fun isLanguageAvailable(context: Context): Boolean =
    currentStatus(context) == STATUS_READY

  fun markLanguageReady(context: Context) {
    storeStatus(context, STATUS_READY)
  }

  fun markLanguageUnavailable(context: Context) {
    storeStatus(context, STATUS_MISSING)
  }

  fun refreshOnDeviceSpeechSupport(context: Context) {
    if (!canCheckSupport(context)) {
      storeStatus(context, STATUS_UNSUPPORTED)
      return
    }

    runOnMain {
      val appContext = context.applicationContext
      val recognizer = createRecognizerOrNull(appContext) ?: run {
        storeStatus(appContext, STATUS_UNSUPPORTED)
        return@runOnMain
      }
      recognizer.checkRecognitionSupport(
        recognizerIntent(),
        ContextCompat.getMainExecutor(appContext),
        object : RecognitionSupportCallback {
          override fun onSupportResult(recognitionSupport: RecognitionSupport) {
            storeStatus(appContext, statusFromSupport(recognitionSupport))
            recognizer.destroy()
          }

          override fun onError(error: Int) {
            storeStatus(appContext, STATUS_ERROR)
            recognizer.destroy()
          }
        },
      )
    }
  }

  fun requestModelDownload(context: Context) {
    if (!canCheckSupport(context)) {
      storeStatus(context, STATUS_UNSUPPORTED)
      return
    }

    runOnMain {
      val appContext = context.applicationContext
      val recognizer = createRecognizerOrNull(appContext) ?: run {
        storeStatus(appContext, STATUS_UNSUPPORTED)
        return@runOnMain
      }

      storeStatus(appContext, STATUS_DOWNLOADING)
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        recognizer.triggerModelDownload(
          recognizerIntent(),
          ContextCompat.getMainExecutor(appContext),
          object : ModelDownloadListener {
            override fun onSuccess() {
              storeStatus(appContext, STATUS_READY)
              recognizer.destroy()
            }

            override fun onProgress(completedPercent: Int) {
              storeStatus(appContext, STATUS_DOWNLOADING)
            }

            override fun onScheduled() {
              storeStatus(appContext, STATUS_PENDING)
              recognizer.destroy()
            }

            override fun onError(error: Int) {
              storeStatus(appContext, STATUS_ERROR)
              recognizer.destroy()
            }
          },
        )
      } else {
        recognizer.triggerModelDownload(recognizerIntent())
        storeStatus(appContext, STATUS_PENDING)
        recognizer.destroy()
      }
    }
  }

  private fun canCheckSupport(context: Context): Boolean =
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
      SpeechRecognizer.isOnDeviceRecognitionAvailable(context)

  private fun createRecognizerOrNull(context: Context): SpeechRecognizer? =
    try {
      SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
    } catch (_: RuntimeException) {
      null
    }

  private fun statusFromSupport(recognitionSupport: RecognitionSupport): String {
    val languageTag = Locale.getDefault().toLanguageTag()
    return when {
      languageMatches(recognitionSupport.installedOnDeviceLanguages, languageTag) ->
        STATUS_READY
      languageMatches(recognitionSupport.pendingOnDeviceLanguages, languageTag) ->
        STATUS_PENDING
      languageMatches(recognitionSupport.supportedOnDeviceLanguages, languageTag) ->
        STATUS_MISSING
      else -> STATUS_UNSUPPORTED
    }
  }

  private fun languageMatches(languages: List<String>, languageTag: String): Boolean {
    val target = normalizeLanguage(languageTag)
    val targetLanguage = target.substringBefore("-")
    return languages.any { language ->
      val normalized = normalizeLanguage(language)
      normalized == target ||
        normalized == targetLanguage ||
        normalized.substringBefore("-") == targetLanguage
    }
  }

  private fun normalizeLanguage(language: String): String =
    language.replace('_', '-').lowercase(Locale.ROOT)

  private fun recognizerIntent(): Intent =
    Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
      putExtra(
        RecognizerIntent.EXTRA_LANGUAGE_MODEL,
        RecognizerIntent.LANGUAGE_MODEL_FREE_FORM,
      )
      putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, false)
      putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
      putExtra(RecognizerIntent.EXTRA_LANGUAGE, Locale.getDefault().toLanguageTag())
    }

  private fun runOnMain(action: () -> Unit) {
    if (Looper.myLooper() == Looper.getMainLooper()) {
      action()
    } else {
      mainHandler.post(action)
    }
  }

  private fun storeStatus(context: Context, status: String) {
    context
      .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
      .edit()
      .putString(SPEECH_MODEL_STATUS_KEY, status)
      .apply()
  }
}
