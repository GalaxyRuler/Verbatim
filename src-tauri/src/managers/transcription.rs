use crate::audio_toolkit::{apply_dictionary_entries, filter_transcription_output};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::{EngineType, ModelManager};
use crate::providers::{EngineProvider, TranscribeRsProvider};
use crate::settings::{
    get_settings, AppSettings, ModelUnloadTimeout, OrtAcceleratorSetting, WhisperAcceleratorSetting,
};
use anyhow::Result;
use log::{debug, error, info, warn};
use serde::Serialize;
use specta::Type;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
}

fn effective_english_translation(user_requested: bool, model_supports_translation: bool) -> bool {
    user_requested && model_supports_translation
}

fn validate_selected_language(selected_language: &str, supported_languages: &[String]) -> String {
    if selected_language == "auto"
        || supported_languages.is_empty()
        || supported_languages.contains(&selected_language.to_string())
    {
        selected_language.to_string()
    } else {
        "auto".to_string()
    }
}

fn build_transcription_request(
    audio: Vec<f32>,
    selected_language: &str,
    translate_to_english: bool,
    language_shortlist: &[String],
    custom_words: &[String],
) -> crate::providers::SpeechRequest {
    let source_language = if selected_language == "auto" {
        crate::providers::LanguageSelection::Auto
    } else {
        crate::providers::LanguageSelection::Language(selected_language.to_string())
    };

    crate::providers::SpeechRequest {
        task: crate::providers::SpeechTaskKind::Transcribe,
        input: crate::providers::SpeechInput::Audio(std::sync::Arc::from(audio)),
        source_language: source_language.clone(),
        translation: translate_to_english.then_some(crate::providers::TranslationTarget {
            source_language,
            target_language: "en".to_string(),
        }),
        language_shortlist: language_shortlist.to_vec(),
        custom_words: custom_words.to_vec(),
        cancellation: crate::providers::CancellationToken::default(),
    }
}

/// RAII guard that clears the `is_loading` flag and notifies waiters on drop.
/// Ensures the loading flag is always reset, even on early returns or panics.
pub struct LoadingGuard {
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
}

impl Drop for LoadingGuard {
    fn drop(&mut self) {
        let mut is_loading = self.is_loading.lock().unwrap();
        *is_loading = false;
        self.loading_condvar.notify_all();
    }
}

#[derive(Clone)]
pub struct TranscriptionManager {
    engine: Arc<Mutex<Option<TranscribeRsProvider>>>,
    model_manager: Arc<ModelManager>,
    app_handle: AppHandle,
    current_model_id: Arc<Mutex<Option<String>>>,
    last_activity: Arc<AtomicU64>,
    shutdown_signal: Arc<AtomicBool>,
    watcher_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    is_loading: Arc<Mutex<bool>>,
    loading_condvar: Arc<Condvar>,
}

impl TranscriptionManager {
    pub fn new(app_handle: &AppHandle, model_manager: Arc<ModelManager>) -> Result<Self> {
        let manager = Self {
            engine: Arc::new(Mutex::new(None)),
            model_manager,
            app_handle: app_handle.clone(),
            current_model_id: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(AtomicU64::new(Self::now_ms())),
            shutdown_signal: Arc::new(AtomicBool::new(false)),
            watcher_handle: Arc::new(Mutex::new(None)),
            is_loading: Arc::new(Mutex::new(false)),
            loading_condvar: Arc::new(Condvar::new()),
        };

        // Start the idle watcher
        {
            let app_handle_cloned = app_handle.clone();
            let manager_cloned = manager.clone();
            let shutdown_signal = manager.shutdown_signal.clone();
            let handle = thread::spawn(move || {
                debug!("Idle watcher thread started");
                while !shutdown_signal.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_secs(10)); // Check every 10 seconds

                    // Check shutdown signal again after sleep
                    if shutdown_signal.load(Ordering::Relaxed) {
                        break;
                    }

                    let settings = get_settings(&app_handle_cloned);
                    let timeout = settings.model_unload_timeout;

                    // Skip Immediately — that variant is handled by
                    // maybe_unload_immediately() after each transcription.
                    // Treating it as 0s here would unload the model mid-recording.
                    if timeout == ModelUnloadTimeout::Immediately {
                        continue;
                    }

                    // While recording, keep the idle timer fresh so the
                    // model is never unloaded mid-session.
                    let is_recording = app_handle_cloned
                        .try_state::<Arc<AudioRecordingManager>>()
                        .map_or(false, |a| a.is_recording());
                    if is_recording {
                        manager_cloned.touch_activity();
                        continue;
                    }

                    if let Some(limit_seconds) = timeout.to_seconds() {
                        let last = manager_cloned.last_activity.load(Ordering::Relaxed);
                        let now_ms = TranscriptionManager::now_ms();
                        let idle_ms = now_ms.saturating_sub(last);
                        let limit_ms = limit_seconds * 1000;

                        if idle_ms > limit_ms {
                            // idle -> unload
                            if manager_cloned.is_model_loaded() {
                                let unload_start = std::time::Instant::now();
                                info!(
                                    "Model idle for {}s (limit: {}s), unloading",
                                    idle_ms / 1000,
                                    limit_seconds
                                );
                                match manager_cloned.unload_model() {
                                    Ok(()) => {
                                        let unload_duration = unload_start.elapsed();
                                        info!(
                                            "Model unloaded due to inactivity (took {}ms)",
                                            unload_duration.as_millis()
                                        );
                                    }
                                    Err(e) => {
                                        error!("Failed to unload idle model: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                debug!("Idle watcher thread shutting down gracefully");
            });
            *manager.watcher_handle.lock().unwrap() = Some(handle);
        }

        Ok(manager)
    }

    /// Lock the engine mutex, recovering from poison if a previous transcription panicked.
    fn lock_engine(&self) -> MutexGuard<'_, Option<TranscribeRsProvider>> {
        self.engine.lock().unwrap_or_else(|poisoned| {
            warn!("Engine mutex was poisoned by a previous panic, recovering");
            poisoned.into_inner()
        })
    }

    pub fn is_model_loaded(&self) -> bool {
        let engine = self.lock_engine();
        engine.is_some()
    }

    /// Atomically check whether a model load is in progress and, if not, mark
    /// one as starting. Returns a [`LoadingGuard`] whose [`Drop`] impl will
    /// clear the flag and wake waiters. Returns `None` if a load is already in
    /// progress.
    pub fn try_start_loading(&self) -> Option<LoadingGuard> {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading {
            return None;
        }
        *is_loading = true;
        Some(LoadingGuard {
            is_loading: self.is_loading.clone(),
            loading_condvar: self.loading_condvar.clone(),
        })
    }

    pub fn unload_model(&self) -> Result<()> {
        let unload_start = std::time::Instant::now();
        debug!("Starting to unload model");

        {
            let mut engine = self.lock_engine();
            if let Some(provider) = engine.as_mut() {
                provider.unload();
            }
            // Dropping the provider frees all resources.
            *engine = None;
        }
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = None;
        }

        // Emit unloaded event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "unloaded".to_string(),
                model_id: None,
                model_name: None,
                error: None,
            },
        );

        let unload_duration = unload_start.elapsed();
        debug!(
            "Model unloaded manually (took {}ms)",
            unload_duration.as_millis()
        );
        Ok(())
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Reset the idle timer to now.
    fn touch_activity(&self) {
        self.last_activity.store(Self::now_ms(), Ordering::Relaxed);
    }

    /// Unloads the model immediately if the setting is enabled and the model is loaded
    pub fn maybe_unload_immediately(&self, context: &str) {
        let settings = get_settings(&self.app_handle);
        if settings.model_unload_timeout == ModelUnloadTimeout::Immediately
            && self.is_model_loaded()
        {
            info!("Immediately unloading model after {}", context);
            if let Err(e) = self.unload_model() {
                warn!("Failed to immediately unload model: {}", e);
            }
        }
    }

    pub fn load_model(&self, model_id: &str) -> Result<()> {
        let load_start = std::time::Instant::now();
        debug!("Starting to load model: {}", model_id);

        // Emit loading started event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_started".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: None,
                error: None,
            },
        );

        let model_info = self
            .model_manager
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !model_info.is_downloaded {
            let error_msg = "Model not downloaded";
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: Some(model_info.name.clone()),
                    error: Some(error_msg.to_string()),
                },
            );
            return Err(anyhow::anyhow!(error_msg));
        }

        let emit_loading_failed = |error_msg: &str| {
            let _ = self.app_handle.emit(
                "model-state-changed",
                ModelStateEvent {
                    event_type: "loading_failed".to_string(),
                    model_id: Some(model_id.to_string()),
                    model_name: Some(model_info.name.clone()),
                    error: Some(error_msg.to_string()),
                },
            );
        };

        let model_asset = self.model_manager.get_model_asset(model_id)?;
        let mut provider = TranscribeRsProvider::new();
        provider.load(&model_asset).map_err(|e| {
            let engine_label = match model_info.engine_type {
                EngineType::Whisper => "whisper",
                EngineType::Parakeet => "parakeet",
                EngineType::Moonshine => "moonshine",
                EngineType::MoonshineStreaming => "moonshine streaming",
                EngineType::SenseVoice => "SenseVoice",
                EngineType::GigaAM => "gigaam",
                EngineType::Canary => "canary",
                EngineType::Cohere => "cohere",
            };
            let error_msg = format!("Failed to load {} model {}: {}", engine_label, model_id, e);
            emit_loading_failed(&error_msg);
            anyhow::anyhow!(error_msg)
        })?;

        // Update the current provider and model ID
        {
            let mut engine = self.lock_engine();
            *engine = Some(provider);
        }
        {
            let mut current_model = self.current_model_id.lock().unwrap();
            *current_model = Some(model_id.to_string());
        }

        // Reset idle timer so the watcher doesn't immediately unload a just-loaded model
        self.touch_activity();

        // Emit loading completed event
        let _ = self.app_handle.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "loading_completed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
            },
        );

        let load_duration = load_start.elapsed();
        debug!(
            "Successfully loaded transcription model: {} (took {}ms)",
            model_id,
            load_duration.as_millis()
        );
        Ok(())
    }

    /// Kicks off the model loading in a background thread if it's not already loaded
    pub fn initiate_model_load(&self) {
        let mut is_loading = self.is_loading.lock().unwrap();
        if *is_loading || self.is_model_loaded() {
            return;
        }

        *is_loading = true;
        let self_clone = self.clone();
        thread::spawn(move || {
            let settings = get_settings(&self_clone.app_handle);
            if let Err(e) = self_clone.load_model(&settings.selected_model) {
                error!("Failed to load model: {}", e);
            }
            let mut is_loading = self_clone.is_loading.lock().unwrap();
            *is_loading = false;
            self_clone.loading_condvar.notify_all();
        });
    }

    pub fn get_current_model(&self) -> Option<String> {
        let current_model = self.current_model_id.lock().unwrap();
        current_model.clone()
    }

    pub fn transcribe(&self, audio: Vec<f32>) -> Result<String> {
        #[cfg(debug_assertions)]
        if std::env::var("VERBATIM_FORCE_TRANSCRIPTION_FAILURE").is_ok() {
            return Err(anyhow::anyhow!(
                "Simulated transcription failure (VERBATIM_FORCE_TRANSCRIPTION_FAILURE)"
            ));
        }

        // Update last activity timestamp
        self.touch_activity();

        let st = std::time::Instant::now();

        debug!("Audio vector length: {}", audio.len());

        if audio.is_empty() {
            debug!("Empty audio vector");
            self.maybe_unload_immediately("empty audio");
            return Ok(String::new());
        }

        // Check if model is loaded, if not try to load it
        {
            // If the model is loading, wait for it to complete.
            let mut is_loading = self.is_loading.lock().unwrap();
            while *is_loading {
                is_loading = self.loading_condvar.wait(is_loading).unwrap();
            }

            let engine_guard = self.lock_engine();
            if engine_guard.is_none() {
                return Err(anyhow::anyhow!("Model is not loaded for transcription."));
            }
        }

        // Get current settings for configuration
        let settings = get_settings(&self.app_handle);
        let current_model_info = self.model_manager.get_model_info(&settings.selected_model);
        let effective_translate_to_english = effective_english_translation(
            settings.translate_to_english,
            current_model_info
                .as_ref()
                .map(|info| info.supports_translation)
                .unwrap_or(false),
        );

        // Validate selected language against the model's supported languages.
        // If the language isn't supported, fall back to "auto" to prevent errors.
        let validated_language = validate_selected_language(
            &settings.selected_language,
            &current_model_info
                .as_ref()
                .map(|info| info.supported_languages.clone())
                .unwrap_or_default(),
        );

        if validated_language == "auto" && settings.selected_language != "auto" {
            warn!(
                "Language '{}' not supported by current model, falling back to auto-detect",
                settings.selected_language
            );
        }

        let request = build_transcription_request(
            audio,
            &validated_language,
            effective_translate_to_english,
            &settings.adaptive_language_shortlist,
            &settings.dictionary_phrases(),
        );

        // Perform transcription with the appropriate provider.
        // We use catch_unwind to prevent engine panics from poisoning the mutex,
        // which would make the app hang indefinitely on subsequent operations.
        let result = {
            let mut engine_guard = self.lock_engine();

            // Take the provider out so we own it during transcription.
            // If the provider panics, we simply don't put it back (effectively unloading it)
            // instead of poisoning the mutex.
            let mut provider = match engine_guard.take() {
                Some(provider) => provider,
                None => {
                    return Err(anyhow::anyhow!(
                        "Model failed to load after auto-load attempt. Please check your model settings."
                    ));
                }
            };

            // Release the lock before transcribing — no mutex held during the engine call
            drop(engine_guard);

            let transcribe_result = catch_unwind(AssertUnwindSafe(
                || -> Result<crate::providers::SpeechResponse> { provider.run(request) },
            ));

            match transcribe_result {
                Ok(inner_result) => {
                    // Success or normal error — put the provider back
                    let mut engine_guard = self.lock_engine();
                    *engine_guard = Some(provider);
                    inner_result?
                }
                Err(panic_payload) => {
                    // Provider panicked — do NOT put it back (it's in an unknown state).
                    // The provider is unloaded and dropped here.
                    provider.unload();
                    let panic_msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    error!(
                        "Transcription engine panicked: {}. Model has been unloaded.",
                        panic_msg
                    );

                    // Clear the model ID so it will be reloaded on next attempt
                    {
                        let mut current_model = self
                            .current_model_id
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        *current_model = None;
                    }

                    let _ = self.app_handle.emit(
                        "model-state-changed",
                        ModelStateEvent {
                            event_type: "unloaded".to_string(),
                            model_id: None,
                            model_name: None,
                            error: Some(format!("Engine panicked: {}", panic_msg)),
                        },
                    );

                    return Err(anyhow::anyhow!(
                        "Transcription engine panicked: {}. The model has been unloaded and will reload on next attempt.",
                        panic_msg
                    ));
                }
            }
        };

        // Apply dictionary corrections after transcription. Dictionary phrases are also passed
        // to Whisper as prompt context, but explicit correction mappings still belong here.
        let is_whisper = self
            .model_manager
            .get_model_info(&settings.selected_model)
            .map(|info| matches!(info.engine_type, EngineType::Whisper))
            .unwrap_or(false);

        let final_result = apply_local_text_transforms(result.text, &settings, is_whisper);

        let et = std::time::Instant::now();
        let translation_note = if effective_translate_to_english {
            " (translated)"
        } else {
            ""
        };
        info!(
            "Transcription completed in {}ms{}",
            (et - st).as_millis(),
            translation_note
        );

        if final_result.is_empty() {
            info!("Transcription result is empty");
        } else {
            info!("Transcription result: {}", final_result);
        }

        self.maybe_unload_immediately("transcription");

        Ok(final_result)
    }
}

fn apply_local_text_transforms(
    raw_text: String,
    settings: &AppSettings,
    is_whisper: bool,
) -> String {
    let corrected_result = if !settings.dictionary_entries.is_empty() {
        if is_whisper {
            debug!("Applying dictionary entries after Whisper transcription");
        }
        apply_dictionary_entries(
            &raw_text,
            &settings.dictionary_entries,
            settings.word_correction_threshold,
        )
    } else {
        raw_text
    };

    let filtered_result = filter_transcription_output(
        &corrected_result,
        &settings.app_language,
        &settings.custom_filler_words,
    );

    if settings.snippets.is_empty() {
        filtered_result
    } else {
        crate::snippets::expand_snippets(&filtered_result, &settings.snippets)
    }
}

/// Apply the user's accelerator preferences to the transcribe-rs global atomics.
/// Called on startup and whenever the user changes the setting.
pub fn apply_accelerator_settings(app: &tauri::AppHandle) {
    use transcribe_rs::accel;

    let settings = get_settings(app);

    let whisper_pref = match settings.whisper_accelerator {
        WhisperAcceleratorSetting::Auto => accel::WhisperAccelerator::Auto,
        WhisperAcceleratorSetting::Cpu => accel::WhisperAccelerator::CpuOnly,
        WhisperAcceleratorSetting::Gpu => accel::WhisperAccelerator::Gpu,
    };
    accel::set_whisper_accelerator(whisper_pref);
    accel::set_whisper_gpu_device(settings.whisper_gpu_device);
    info!(
        "Whisper accelerator set to: {}, gpu_device: {}",
        whisper_pref,
        if settings.whisper_gpu_device == accel::GPU_DEVICE_AUTO {
            "auto".to_string()
        } else {
            settings.whisper_gpu_device.to_string()
        }
    );

    let ort_pref = match settings.ort_accelerator {
        OrtAcceleratorSetting::Auto => accel::OrtAccelerator::Auto,
        OrtAcceleratorSetting::Cpu => accel::OrtAccelerator::CpuOnly,
        OrtAcceleratorSetting::Cuda => accel::OrtAccelerator::Cuda,
        OrtAcceleratorSetting::DirectMl => accel::OrtAccelerator::DirectMl,
        OrtAcceleratorSetting::Rocm => accel::OrtAccelerator::Rocm,
    };
    accel::set_ort_accelerator(ort_pref);
    info!("ORT accelerator set to: {}", ort_pref);
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct GpuDeviceOption {
    pub id: i32,
    pub name: String,
    pub total_vram_mb: usize,
}

static GPU_DEVICES: OnceLock<Vec<GpuDeviceOption>> = OnceLock::new();

fn cached_gpu_devices() -> &'static [GpuDeviceOption] {
    use transcribe_rs::whisper_cpp::gpu::list_gpu_devices;

    GPU_DEVICES.get_or_init(|| {
        // ggml's Vulkan backend uses FMA3 instructions internally.
        // On older CPUs without FMA3 (e.g. Sandy Bridge Xeons) this causes
        // a SIGILL crash that cannot be caught. Skip enumeration entirely
        // on those CPUs — GPU-accelerated whisper won't work there anyway.
        #[cfg(target_arch = "x86_64")]
        if !std::arch::is_x86_feature_detected!("fma") {
            warn!("CPU lacks FMA3 support — skipping GPU device enumeration");
            return Vec::new();
        }

        list_gpu_devices()
            .into_iter()
            .map(|d| GpuDeviceOption {
                id: d.id,
                name: d.name,
                total_vram_mb: d.total_vram / (1024 * 1024),
            })
            .collect()
    })
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct AvailableAccelerators {
    pub whisper: Vec<String>,
    pub ort: Vec<String>,
    pub gpu_devices: Vec<GpuDeviceOption>,
}

/// Return which accelerators are compiled into this build.
pub fn get_available_accelerators() -> AvailableAccelerators {
    use transcribe_rs::accel::OrtAccelerator;

    let ort_options: Vec<String> = OrtAccelerator::available()
        .into_iter()
        .map(|a| a.to_string())
        .collect();

    let whisper_options = vec!["auto".to_string(), "cpu".to_string(), "gpu".to_string()];

    AvailableAccelerators {
        whisper: whisper_options,
        ort: ort_options,
        gpu_devices: cached_gpu_devices().to_vec(),
    }
}

impl Drop for TranscriptionManager {
    fn drop(&mut self) {
        // Skip shutdown unless this is the very last clone. TranscriptionManager
        // is cloned by initiate_model_load() and the watcher thread — those
        // clones dropping must not kill the watcher. The watcher thread holds
        // its own clone, so engine's strong_count is always >= 2 while the
        // watcher is alive. When it reaches 1, only this instance remains
        // and we can safely shut down.
        if Arc::strong_count(&self.engine) > 1 {
            return;
        }

        // Signal the watcher thread to shutdown
        self.shutdown_signal.store(true, Ordering::Relaxed);

        // Wait for the thread to finish gracefully
        if let Some(handle) = self.watcher_handle.lock().unwrap().take() {
            if let Err(e) = handle.join() {
                warn!("Failed to join idle watcher thread: {:?}", e);
            } else {
                debug!("Idle watcher thread joined successfully");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_translation_requires_user_toggle_and_model_support() {
        assert!(effective_english_translation(true, true));
        assert!(!effective_english_translation(true, false));
        assert!(!effective_english_translation(false, true));
        assert!(!effective_english_translation(false, false));
    }

    #[test]
    fn unsupported_selected_language_falls_back_to_auto() {
        assert_eq!(
            validate_selected_language("ar", &["en".to_string(), "fr".to_string()]),
            "auto"
        );
        assert_eq!(
            validate_selected_language("ar", &["en".to_string(), "ar".to_string()]),
            "ar"
        );
        assert_eq!(
            validate_selected_language("auto", &["en".to_string()]),
            "auto"
        );
    }

    #[test]
    fn empty_supported_language_list_accepts_any_selected_language() {
        assert_eq!(validate_selected_language("ar", &[]), "ar");
    }

    #[test]
    fn speech_request_translation_is_absent_when_legacy_toggle_is_off() {
        let request = build_transcription_request(
            vec![0.0, 1.0],
            "auto",
            false,
            &["en".to_string(), "ar".to_string()],
            &[],
        );

        assert!(request.translation.is_none());
    }

    #[test]
    fn speech_request_preserves_source_language_without_translation() {
        let request = build_transcription_request(
            vec![0.0, 1.0],
            "ar",
            false,
            &["en".to_string(), "ar".to_string()],
            &[],
        );

        assert_eq!(
            request.source_language,
            crate::providers::LanguageSelection::Language("ar".to_string())
        );
    }

    #[test]
    fn speech_request_uses_english_target_for_legacy_translation() {
        let request = build_transcription_request(
            vec![0.0, 1.0],
            "auto",
            true,
            &["en".to_string(), "ar".to_string()],
            &[],
        );

        let translation = request.translation.expect("translation should be present");
        assert_eq!(translation.target_language, "en");
    }

    #[test]
    fn local_text_transforms_expand_snippets_after_filtering_dictated_text() {
        let mut settings = crate::settings::get_default_settings();
        settings.custom_filler_words = Some(vec!["please use".to_string()]);
        settings.snippets = vec![crate::snippets::SnippetEntry {
            id: "snippet_1_email".to_string(),
            trigger: "email signature".to_string(),
            content: "please use Regards,\nAbdullah".to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
        }];

        let result =
            apply_local_text_transforms("please use email signature".to_string(), &settings, false);

        assert_eq!(result, "please use Regards,\nAbdullah");
    }
}
