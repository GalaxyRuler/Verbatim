use crate::audio_toolkit::{apply_dictionary_entries, filter_transcription_output};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::model::{EngineType, ModelManager};
use crate::providers::{
    resolve_whisper_gpu_device, CancellationToken, EngineProvider, ModelLocator,
    TranscribeRsProvider,
};
use crate::settings::{
    get_settings, AppSettings, ModelUnloadTimeout, OrtAcceleratorSetting, WhisperAcceleratorSetting,
};
use anyhow::Result;
use log::{debug, error, info, warn};
use serde::Serialize;
use specta::Type;
use std::collections::HashSet;
use std::io::Read;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
    pub diagnostic_code: Option<String>,
    pub fallback: Option<String>,
}

fn model_load_diagnostic_code(error: &anyhow::Error) -> &'static str {
    let message = error.to_string().to_ascii_lowercase();
    if [
        "accelerator",
        "cuda",
        "directml",
        "gpu",
        "metal",
        "ort",
        "vulkan",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        "accelerator_load_failed"
    } else {
        "provider_load_failed"
    }
}

fn engine_label(engine_type: EngineType) -> &'static str {
    match engine_type {
        EngineType::Whisper => "whisper",
        EngineType::Parakeet => "parakeet",
        EngineType::Moonshine => "moonshine",
        EngineType::MoonshineStreaming => "moonshine streaming",
        EngineType::SenseVoice => "SenseVoice",
        EngineType::GigaAM => "gigaam",
        EngineType::Canary => "canary",
        EngineType::Cohere => "cohere",
    }
}

fn should_retry_model_load_on_cpu(
    settings: &AppSettings,
    engine_type: EngineType,
    error: &anyhow::Error,
) -> bool {
    if model_load_diagnostic_code(error) != "accelerator_load_failed" {
        return false;
    }

    match engine_type {
        EngineType::Whisper => settings.whisper_accelerator != WhisperAcceleratorSetting::Cpu,
        EngineType::Parakeet
        | EngineType::Moonshine
        | EngineType::MoonshineStreaming
        | EngineType::SenseVoice
        | EngineType::GigaAM
        | EngineType::Canary
        | EngineType::Cohere => settings.ort_accelerator != OrtAcceleratorSetting::Cpu,
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ModelLoadFallbackDrillCase {
    pub case: String,
    pub diagnostic_code: String,
    pub retry_on_cpu: bool,
    pub expected_retry_on_cpu: bool,
    pub success_fallback: Option<String>,
    pub passed: bool,
}

pub(crate) fn model_load_cpu_fallback_drill() -> Vec<ModelLoadFallbackDrillCase> {
    let mut whisper_gpu = crate::settings::get_default_settings();
    whisper_gpu.whisper_accelerator = WhisperAcceleratorSetting::Gpu;

    let mut whisper_cpu = crate::settings::get_default_settings();
    whisper_cpu.whisper_accelerator = WhisperAcceleratorSetting::Cpu;

    let mut ort_directml = crate::settings::get_default_settings();
    ort_directml.ort_accelerator = OrtAcceleratorSetting::DirectMl;

    let generic = crate::settings::get_default_settings();

    let cases = [
        (
            "whisper_gpu_accelerator_failure",
            whisper_gpu,
            EngineType::Whisper,
            anyhow::anyhow!("Vulkan initialization failed"),
            true,
        ),
        (
            "whisper_cpu_accelerator_failure",
            whisper_cpu,
            EngineType::Whisper,
            anyhow::anyhow!("Vulkan initialization failed"),
            false,
        ),
        (
            "ort_directml_accelerator_failure",
            ort_directml,
            EngineType::Parakeet,
            anyhow::anyhow!("DirectML provider failed to initialize"),
            true,
        ),
        (
            "generic_provider_failure",
            generic,
            EngineType::Whisper,
            anyhow::anyhow!("model file is unreadable"),
            false,
        ),
    ];

    cases
        .into_iter()
        .map(
            |(case, settings, engine_type, error, expected_retry_on_cpu)| {
                let diagnostic_code = model_load_diagnostic_code(&error).to_string();
                let retry_on_cpu = should_retry_model_load_on_cpu(&settings, engine_type, &error);
                ModelLoadFallbackDrillCase {
                    case: case.to_string(),
                    diagnostic_code,
                    retry_on_cpu,
                    expected_retry_on_cpu,
                    success_fallback: retry_on_cpu
                        .then(|| "cpu_after_accelerator_load_failed".to_string()),
                    passed: retry_on_cpu == expected_retry_on_cpu,
                }
            },
        )
        .collect()
}

fn apply_cpu_accelerator_fallback(engine_type: EngineType) {
    use transcribe_rs::accel;

    match engine_type {
        EngineType::Whisper => {
            accel::set_whisper_accelerator(accel::WhisperAccelerator::CpuOnly);
            accel::set_whisper_gpu_device(accel::GPU_DEVICE_AUTO);
        }
        EngineType::Parakeet
        | EngineType::Moonshine
        | EngineType::MoonshineStreaming
        | EngineType::SenseVoice
        | EngineType::GigaAM
        | EngineType::Canary
        | EngineType::Cohere => {
            accel::set_ort_accelerator(accel::OrtAccelerator::CpuOnly);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum GpuPreflightOutcome {
    Passed,
    Failed(String),
    Skipped(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GpuPreflightFallbackDecision {
    persist_cpu: bool,
    fallback_code: &'static str,
}

fn should_run_whisper_gpu_preflight(settings: &AppSettings, engine_type: EngineType) -> bool {
    matches!(engine_type, EngineType::Whisper)
        && settings.whisper_accelerator != WhisperAcceleratorSetting::Cpu
}

fn gpu_preflight_cpu_fallback_decision(
    settings: &AppSettings,
    engine_type: EngineType,
    outcome: &GpuPreflightOutcome,
) -> Option<GpuPreflightFallbackDecision> {
    if !should_run_whisper_gpu_preflight(settings, engine_type) {
        return None;
    }

    match outcome {
        GpuPreflightOutcome::Failed(_) => Some(GpuPreflightFallbackDecision {
            persist_cpu: true,
            fallback_code: "cpu_after_gpu_preflight_failed",
        }),
        GpuPreflightOutcome::Passed | GpuPreflightOutcome::Skipped(_) => None,
    }
}

static WHISPER_GPU_PREFLIGHT_OK: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn whisper_gpu_preflight_ok_cache() -> &'static Mutex<HashSet<String>> {
    WHISPER_GPU_PREFLIGHT_OK.get_or_init(|| Mutex::new(HashSet::new()))
}

fn whisper_gpu_preflight_cache_key(model_path: &Path, gpu_device: i32) -> String {
    let model_key = model_path
        .canonicalize()
        .unwrap_or_else(|_| model_path.to_path_buf())
        .display()
        .to_string();
    format!("{model_key}|gpu_device={gpu_device}|flash_attn=false")
}

fn join_preflight_stderr(stderr_reader: Option<thread::JoinHandle<String>>) -> String {
    stderr_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default()
}

fn run_whisper_gpu_preflight_child(
    model_path: &Path,
    gpu_device: i32,
    timeout: Duration,
) -> GpuPreflightOutcome {
    let exe_path = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            return GpuPreflightOutcome::Failed(format!("resolve current executable: {error}"));
        }
    };

    let mut child = match Command::new(exe_path)
        .arg("--whisper-gpu-preflight")
        .arg(model_path)
        .arg("--whisper-gpu-preflight-device")
        .arg(gpu_device.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return GpuPreflightOutcome::Failed(format!("spawn GPU preflight process: {error}"));
        }
    };

    let mut stderr_reader = child.stderr.take().map(|mut pipe| {
        thread::spawn(move || {
            let mut stderr = String::new();
            let _ = pipe.read_to_string(&mut stderr);
            stderr
        })
    });

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let _ = child.wait();
                let stderr = join_preflight_stderr(stderr_reader.take());
                if status.success() {
                    return GpuPreflightOutcome::Passed;
                }
                let stderr = stderr.trim();
                let detail = if stderr.is_empty() {
                    status.to_string()
                } else {
                    format!("{status}: {stderr}")
                };
                return GpuPreflightOutcome::Failed(detail);
            }
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_preflight_stderr(stderr_reader.take());
                return GpuPreflightOutcome::Failed(format!(
                    "GPU preflight timed out after {}s",
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = join_preflight_stderr(stderr_reader.take());
                return GpuPreflightOutcome::Failed(format!("poll GPU preflight process: {error}"));
            }
        }
    }
}

fn run_cached_whisper_gpu_preflight(model_path: &Path, gpu_device: i32) -> GpuPreflightOutcome {
    let cache_key = whisper_gpu_preflight_cache_key(model_path, gpu_device);
    {
        let cache = whisper_gpu_preflight_ok_cache().lock().unwrap();
        if cache.contains(&cache_key) {
            return GpuPreflightOutcome::Passed;
        }
    }

    let outcome = run_whisper_gpu_preflight_child(model_path, gpu_device, Duration::from_secs(30));
    if outcome == GpuPreflightOutcome::Passed {
        whisper_gpu_preflight_ok_cache()
            .lock()
            .unwrap()
            .insert(cache_key);
    }
    outcome
}

fn whisper_preflight_model_path(asset: &crate::providers::ModelAsset) -> Option<&Path> {
    match &asset.locator {
        ModelLocator::File(path) => Some(path.as_path()),
        ModelLocator::Directory(_)
        | ModelLocator::ManagedServer { .. }
        | ModelLocator::ExternalHttp { .. } => None,
    }
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

fn transcription_result_log_message(final_result: &str) -> String {
    format!(
        "Transcription result ready ({} chars)",
        final_result.chars().count()
    )
}

fn build_transcription_request(
    audio: Vec<f32>,
    selected_language: &str,
    translate_to_english: bool,
    language_shortlist: &[String],
    custom_words: &[String],
    cancellation: CancellationToken,
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
        cancellation,
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

fn wait_for_model_loading_to_finish(
    is_loading: &Mutex<bool>,
    loading_condvar: &Condvar,
    cancellation: &CancellationToken,
) -> Result<()> {
    if cancellation.is_cancelled() {
        return Err(anyhow::anyhow!("transcription cancelled before model load"));
    }

    let mut is_loading = is_loading.lock().unwrap();
    while *is_loading {
        let wait_result = loading_condvar
            .wait_timeout(is_loading, Duration::from_millis(50))
            .unwrap();
        is_loading = wait_result.0;

        if cancellation.is_cancelled() {
            return Err(anyhow::anyhow!("transcription cancelled during model load"));
        }
    }

    Ok(())
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
                diagnostic_code: None,
                fallback: None,
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
                diagnostic_code: None,
                fallback: None,
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
                    diagnostic_code: Some("model_not_downloaded".to_string()),
                    fallback: None,
                },
            );
            return Err(anyhow::anyhow!(error_msg));
        }

        let emit_loading_failed =
            |error_msg: &str, diagnostic_code: &str, fallback: Option<String>| {
                let _ = self.app_handle.emit(
                    "model-state-changed",
                    ModelStateEvent {
                        event_type: "loading_failed".to_string(),
                        model_id: Some(model_id.to_string()),
                        model_name: Some(model_info.name.clone()),
                        error: Some(error_msg.to_string()),
                        diagnostic_code: Some(diagnostic_code.to_string()),
                        fallback,
                    },
                );
            };

        let model_asset = self.model_manager.get_model_asset(model_id)?;
        let mut settings = get_settings(&self.app_handle);
        let mut preflight_fallback = None;
        if should_run_whisper_gpu_preflight(&settings, model_info.engine_type.clone()) {
            let preflight_outcome =
                if let Some(model_path) = whisper_preflight_model_path(&model_asset) {
                    let gpu_device = resolve_whisper_gpu_device(true, settings.whisper_gpu_device);
                    run_cached_whisper_gpu_preflight(model_path, gpu_device)
                } else {
                    GpuPreflightOutcome::Skipped("whisper model is not a local file")
                };

            if let Some(decision) = gpu_preflight_cpu_fallback_decision(
                &settings,
                model_info.engine_type.clone(),
                &preflight_outcome,
            ) {
                warn!(
                    "Whisper GPU preflight failed for {}; falling back to CPU: {:?}",
                    model_id, preflight_outcome
                );
                if decision.persist_cpu {
                    crate::settings::write_settings_domain(
                        &self.app_handle,
                        crate::settings::SettingsWriteDomain::Models,
                        |settings| {
                            settings.whisper_accelerator = WhisperAcceleratorSetting::Cpu;
                            settings.whisper_gpu_device = transcribe_rs::accel::GPU_DEVICE_AUTO;
                        },
                    )
                    .map_err(|err| anyhow::anyhow!(err))?;
                    settings.whisper_accelerator = WhisperAcceleratorSetting::Cpu;
                    settings.whisper_gpu_device = transcribe_rs::accel::GPU_DEVICE_AUTO;
                }
                apply_cpu_accelerator_fallback(model_info.engine_type.clone());
                preflight_fallback = Some(decision.fallback_code.to_string());
            } else if let GpuPreflightOutcome::Skipped(reason) = preflight_outcome {
                debug!(
                    "Skipping Whisper GPU preflight for {}: {}",
                    model_id, reason
                );
            }
        }
        apply_accelerator_settings(&self.app_handle);
        let mut provider = TranscribeRsProvider::new();
        let load_fallback = match provider.load(&model_asset) {
            Ok(()) => None,
            Err(initial_error)
                if should_retry_model_load_on_cpu(
                    &settings,
                    model_info.engine_type.clone(),
                    &initial_error,
                ) =>
            {
                warn!(
                    "Accelerated model load failed for {}; retrying with CPU fallback: {}",
                    model_id, initial_error
                );
                apply_cpu_accelerator_fallback(model_info.engine_type.clone());

                let mut cpu_provider = TranscribeRsProvider::new();
                match cpu_provider.load(&model_asset) {
                    Ok(()) => {
                        provider = cpu_provider;
                        Some("cpu_after_accelerator_load_failed".to_string())
                    }
                    Err(cpu_error) => {
                        let error_msg = format!(
                            "Failed to load {} model {} after CPU fallback. Accelerated error: {}; CPU error: {}",
                            engine_label(model_info.engine_type.clone()),
                            model_id,
                            initial_error,
                            cpu_error
                        );
                        emit_loading_failed(
                            &error_msg,
                            "accelerator_load_failed",
                            Some("cpu_fallback_failed".to_string()),
                        );
                        return Err(anyhow::anyhow!(error_msg));
                    }
                }
            }
            Err(error) => {
                let error_msg = format!(
                    "Failed to load {} model {}: {}",
                    engine_label(model_info.engine_type.clone()),
                    model_id,
                    error
                );
                let diagnostic_code = model_load_diagnostic_code(&error);
                emit_loading_failed(&error_msg, diagnostic_code, None);
                return Err(anyhow::anyhow!(error_msg));
            }
        };
        let fallback = preflight_fallback.or(load_fallback);

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
                diagnostic_code: None,
                fallback,
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
        self.transcribe_with_cancellation(audio, CancellationToken::default())
    }

    pub fn transcribe_with_cancellation(
        &self,
        audio: Vec<f32>,
        cancellation: CancellationToken,
    ) -> Result<String> {
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
            wait_for_model_loading_to_finish(
                self.is_loading.as_ref(),
                self.loading_condvar.as_ref(),
                &cancellation,
            )?;

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
            cancellation,
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
                            diagnostic_code: Some("provider_panic".to_string()),
                            fallback: Some("model_unloaded_for_reload".to_string()),
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
            info!("{}", transcription_result_log_message(&final_result));
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
    fn model_load_diagnostic_code_classifies_accelerator_failures() {
        assert_eq!(
            model_load_diagnostic_code(&anyhow::anyhow!("Vulkan initialization failed")),
            "accelerator_load_failed"
        );
        assert_eq!(
            model_load_diagnostic_code(&anyhow::anyhow!("model file is unreadable")),
            "provider_load_failed"
        );
    }

    #[test]
    fn cpu_fallback_retries_accelerator_load_failures_for_non_cpu_whisper() {
        let mut settings = crate::settings::get_default_settings();
        settings.whisper_accelerator = WhisperAcceleratorSetting::Gpu;

        assert!(should_retry_model_load_on_cpu(
            &settings,
            EngineType::Whisper,
            &anyhow::anyhow!("Vulkan initialization failed")
        ));
    }

    #[test]
    fn cpu_fallback_does_not_retry_when_whisper_is_already_cpu() {
        let mut settings = crate::settings::get_default_settings();
        settings.whisper_accelerator = WhisperAcceleratorSetting::Cpu;

        assert!(!should_retry_model_load_on_cpu(
            &settings,
            EngineType::Whisper,
            &anyhow::anyhow!("Vulkan initialization failed")
        ));
    }

    #[test]
    fn cpu_fallback_retries_accelerator_load_failures_for_non_cpu_ort_models() {
        let mut settings = crate::settings::get_default_settings();
        settings.ort_accelerator = OrtAcceleratorSetting::DirectMl;

        assert!(should_retry_model_load_on_cpu(
            &settings,
            EngineType::Parakeet,
            &anyhow::anyhow!("DirectML provider failed to initialize")
        ));
    }

    #[test]
    fn cpu_fallback_does_not_retry_generic_provider_failures() {
        let settings = crate::settings::get_default_settings();

        assert!(!should_retry_model_load_on_cpu(
            &settings,
            EngineType::Whisper,
            &anyhow::anyhow!("model file is unreadable")
        ));
    }

    #[test]
    fn gpu_preflight_failure_switches_non_cpu_whisper_to_cpu_fallback() {
        let mut settings = crate::settings::get_default_settings();
        settings.whisper_accelerator = WhisperAcceleratorSetting::Auto;
        settings.whisper_gpu_device = transcribe_rs::accel::GPU_DEVICE_AUTO;

        let decision = gpu_preflight_cpu_fallback_decision(
            &settings,
            EngineType::Whisper,
            &GpuPreflightOutcome::Failed("process exited with 0xc000001d".to_string()),
        );

        assert_eq!(
            decision,
            Some(GpuPreflightFallbackDecision {
                persist_cpu: true,
                fallback_code: "cpu_after_gpu_preflight_failed",
            })
        );
    }

    #[test]
    fn gpu_preflight_failure_does_not_override_explicit_cpu_or_non_whisper_models() {
        let mut settings = crate::settings::get_default_settings();
        settings.whisper_accelerator = WhisperAcceleratorSetting::Cpu;

        assert_eq!(
            gpu_preflight_cpu_fallback_decision(
                &settings,
                EngineType::Whisper,
                &GpuPreflightOutcome::Failed("crashed".to_string()),
            ),
            None
        );

        settings.whisper_accelerator = WhisperAcceleratorSetting::Auto;
        assert_eq!(
            gpu_preflight_cpu_fallback_decision(
                &settings,
                EngineType::Parakeet,
                &GpuPreflightOutcome::Failed("crashed".to_string()),
            ),
            None
        );
    }

    #[test]
    fn speech_request_translation_is_absent_when_legacy_toggle_is_off() {
        let request = build_transcription_request(
            vec![0.0, 1.0],
            "auto",
            false,
            &["en".to_string(), "ar".to_string()],
            &[],
            CancellationToken::default(),
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
            CancellationToken::default(),
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
            CancellationToken::default(),
        );

        let translation = request.translation.expect("translation should be present");
        assert_eq!(translation.target_language, "en");
    }

    #[test]
    fn cancelled_transcription_does_not_wait_for_model_load() {
        let is_loading = Mutex::new(true);
        let loading_condvar = Condvar::new();
        let cancellation = CancellationToken::default();
        cancellation.cancel();

        let error = wait_for_model_loading_to_finish(&is_loading, &loading_condvar, &cancellation)
            .expect_err("cancelled operation should not wait for model load");

        assert!(error.to_string().contains("cancelled before model load"));
    }

    #[test]
    fn cancellation_during_model_load_wait_returns_promptly() {
        let is_loading = Arc::new(Mutex::new(true));
        let loading_condvar = Arc::new(Condvar::new());
        let cancellation = CancellationToken::default();
        let cancellation_for_thread = cancellation.clone();
        let loading_condvar_for_thread = Arc::clone(&loading_condvar);

        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            cancellation_for_thread.cancel();
            loading_condvar_for_thread.notify_all();
        });

        let error = wait_for_model_loading_to_finish(
            is_loading.as_ref(),
            loading_condvar.as_ref(),
            &cancellation,
        )
        .expect_err("cancelled operation should abort the model load wait");

        handle.join().expect("cancellation thread should finish");
        assert!(error.to_string().contains("cancelled during model load"));
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

    #[test]
    fn transcription_result_log_message_does_not_include_transcript_text() {
        let transcript = "Private dictated sentence with a customer name";
        let message = transcription_result_log_message(transcript);

        assert!(!message.contains(transcript));
        assert!(message.contains("ready"));
        assert!(message.contains("46 chars"));
    }
}
