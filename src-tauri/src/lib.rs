mod actions;
pub mod adaptive;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple_intelligence;
mod audio_feedback;
pub mod audio_toolkit;
pub mod cli;
mod clipboard;
mod commands;
mod credentials;
mod dictation_transaction;
mod dictionary;
mod dictionary_learning;
mod helpers;
mod input;
mod insertion;
mod linux_readiness;
mod llm_client;
pub mod local_llm;
mod managers;
mod operation_cancellation;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod overlay;
#[cfg(any(target_os = "android", target_os = "ios"))]
#[path = "overlay_stub.rs"]
mod overlay;
pub mod portable;
mod post_paste_learning;
mod private_session;
pub mod providers;
mod runtime_settings;
mod selection;
mod settings;
mod shortcut;
mod signal_handle;
mod snippets;
mod text_processing;
mod transcription_coordinator;
mod transform_mode;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod tray;
#[cfg(any(target_os = "android", target_os = "ios"))]
#[path = "tray_stub.rs"]
mod tray;
mod tray_i18n;
mod utils;

pub use cli::CliArgs;
#[cfg(all(debug_assertions, not(any(target_os = "android", target_os = "ios"))))]
use specta_typescript::{BigIntExportBehavior, Typescript};
use tauri_specta::{collect_commands, collect_events, Builder};
use transcription_coordinator::CoordinatorHealthSnapshot;

use env_filter::Builder as EnvFilterBuilder;
use local_llm::download::LocalLlmManager;
use managers::audio::AudioRecordingManager;
use managers::history::HistoryManager;
use managers::model::ModelManager;
use managers::transcription::TranscriptionManager;
use serde::Serialize;
#[cfg(unix)]
use signal_hook::consts::{SIGUSR1, SIGUSR2};
#[cfg(unix)]
use signal_hook::iterator::Signals;
use specta::Type;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::image::Image;
pub use transcription_coordinator::TranscriptionCoordinator;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::tray::TrayIconBuilder;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri::Listener;
use tauri::{AppHandle, Emitter, Manager};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_log::{Builder as LogBuilder, RotationStrategy, Target, TargetKind};

use crate::settings::get_settings;

// Global atomic to store the file log level filter
// We use u8 to store the log::LevelFilter as a number
pub static FILE_LOG_LEVEL: AtomicU8 = AtomicU8::new(log::LevelFilter::Debug as u8);

#[derive(Clone, Debug, Serialize, Type)]
#[serde(tag = "status", rename_all = "snake_case")]
enum StartupStatus {
    Starting,
    Ready,
    Failed { step: String, message: String },
}

#[derive(Default)]
struct StartupState {
    status: Mutex<StartupStatus>,
}

impl Default for StartupStatus {
    fn default() -> Self {
        Self::Starting
    }
}

impl StartupState {
    fn set_ready(&self) {
        *self.status.lock().unwrap() = StartupStatus::Ready;
    }

    fn set_failed(&self, step: impl Into<String>, error: impl std::fmt::Display) {
        *self.status.lock().unwrap() = StartupStatus::Failed {
            step: step.into(),
            message: error.to_string(),
        };
    }

    fn snapshot(&self) -> StartupStatus {
        self.status.lock().unwrap().clone()
    }
}

#[derive(Clone, Debug, Serialize)]
struct NativeSmokeStatus {
    startup_status: StartupStatus,
    settings_loaded: bool,
    main_window_created: bool,
    tray_initialized: bool,
    tray_visible_requested: bool,
    no_tray_cli: bool,
    updater_plugin_registered: bool,
    single_instance_plugin_registered: bool,
    close_to_tray_handler_registered: bool,
    debug_mode_enabled: bool,
    selected_microphone: String,
    selected_model_configured: bool,
    selected_model_id: String,
    selected_model_downloaded: bool,
    selected_model_custom: bool,
    selected_model_has_remote_url: bool,
    coordinator_health_events: Vec<CoordinatorHealthSnapshot>,
    audio_fixture_path: Option<String>,
    audio_fixture_sample_count: usize,
    audio_fixture_verified: bool,
    resource_probe_checked: bool,
    resource_probe_failures: Vec<String>,
    retention: Option<NativeSmokeRetentionStatus>,
    linux_environment: crate::linux_readiness::LinuxEnvironmentStatus,
    credential_store: crate::credentials::CredentialStoreStatus,
    credential_migration: Option<NativeSmokeCredentialMigrationStatus>,
    model_load_fallback_drill: Vec<managers::transcription::ModelLoadFallbackDrillCase>,
    insertion_safety_drill: Vec<NativeSmokeInsertionSafetyDrillCase>,
    clipboard_safety_drill: Vec<crate::clipboard::NativeSmokeClipboardSafetyDrillCase>,
}

#[derive(Clone, Debug, Serialize)]
struct NativeSmokeRetentionStatus {
    history_enabled: bool,
    recordings_enabled: bool,
    history_limit: usize,
    recording_retention_period: settings::RecordingRetentionPeriod,
    history_entry_count: usize,
    recording_file_count: usize,
    storage_policy_drill_verified: bool,
    storage_policy_drill: Vec<NativeSmokeStoragePolicyDrillCase>,
    clean_profile_verified: bool,
    failures: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct NativeSmokeStoragePolicyDrillCase {
    case: String,
    history_enabled: bool,
    recordings_enabled: bool,
    expected_history_enabled: bool,
    expected_recordings_enabled: bool,
    passed: bool,
}

#[derive(Clone, Debug, Serialize)]
struct NativeSmokeCredentialMigrationStatus {
    checked: bool,
    skipped: bool,
    available: bool,
    retained_legacy_api_key_count: usize,
    legacy_key_removed_from_settings: bool,
    credential_round_trip_verified: bool,
    cleanup_succeeded: bool,
    leaked_probe_secret: bool,
    failures: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct NativeSmokeInsertionSafetyDrillCase {
    case: String,
    paste_callback_invoked: bool,
    attempted: bool,
    target_verified: bool,
    error: Option<String>,
    passed: bool,
}

fn native_smoke_forced_startup_failure() -> Option<StartupError> {
    if std::env::var("VERBATIM_SMOKE_FORCE_STARTUP_FAILURE").as_deref() != Ok("1") {
        return None;
    }

    Some(StartupError::new(
        "native smoke forced startup failure",
        anyhow::anyhow!("forced startup failure for packaged smoke recovery drill"),
    ))
}

fn apply_native_smoke_microphone_selection(app: &AppHandle, settings: &mut settings::AppSettings) {
    let Ok(value) = std::env::var("VERBATIM_SMOKE_SELECTED_MICROPHONE") else {
        return;
    };
    let selected_microphone = value.trim();
    if selected_microphone.is_empty() {
        log::warn!("Ignoring empty VERBATIM_SMOKE_SELECTED_MICROPHONE value");
        return;
    }

    match settings::mutate_settings_domain(
        settings,
        settings::SettingsWriteDomain::Audio,
        |settings| {
            settings.selected_microphone = if selected_microphone.eq_ignore_ascii_case("default") {
                None
            } else {
                Some(selected_microphone.to_string())
            };
        },
    ) {
        Ok(()) => settings::write_settings(app, settings.clone()),
        Err(error) => log::warn!("Failed to apply native smoke microphone selection: {error}"),
    }
}

fn level_filter_from_u8(value: u8) -> log::LevelFilter {
    match value {
        0 => log::LevelFilter::Off,
        1 => log::LevelFilter::Error,
        2 => log::LevelFilter::Warn,
        3 => log::LevelFilter::Info,
        4 => log::LevelFilter::Debug,
        5 => log::LevelFilter::Trace,
        _ => log::LevelFilter::Trace,
    }
}

fn build_console_filter() -> env_filter::Filter {
    let mut builder = EnvFilterBuilder::new();

    match std::env::var("RUST_LOG") {
        Ok(spec) if !spec.trim().is_empty() => {
            if let Err(err) = builder.try_parse(&spec) {
                log::warn!(
                    "Ignoring invalid RUST_LOG value '{}': {}. Falling back to info-level console logging",
                    spec,
                    err
                );
                builder.filter_level(log::LevelFilter::Info);
            }
        }
        _ => {
            builder.filter_level(log::LevelFilter::Info);
        }
    }

    builder.build()
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn show_main_window(app: &AppHandle) {
    if let Some(main_window) = app.get_webview_window("main") {
        if let Err(e) = main_window.unminimize() {
            log::error!("Failed to unminimize webview window: {}", e);
        }
        if let Err(e) = main_window.show() {
            log::error!("Failed to show webview window: {}", e);
        }
        if let Err(e) = main_window.set_focus() {
            log::error!("Failed to focus webview window: {}", e);
        }
        #[cfg(target_os = "macos")]
        {
            if let Err(e) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
                log::error!("Failed to set activation policy to Regular: {}", e);
            }
        }
        return;
    }

    let webview_labels = app.webview_windows().keys().cloned().collect::<Vec<_>>();
    log::error!(
        "Main window not found. Webview labels: {:?}",
        webview_labels
    );
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn show_main_window(_app: &AppHandle) {}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn should_force_show_permissions_window(app: &AppHandle) -> bool {
    #[cfg(target_os = "windows")]
    {
        let model_manager = app.state::<Arc<ModelManager>>();
        let has_downloaded_models = model_manager
            .get_available_models()
            .iter()
            .any(|model| model.is_downloaded);

        if !has_downloaded_models {
            return false;
        }

        let status = commands::audio::get_windows_microphone_permission_status();
        if status.supported && status.overall_access == commands::audio::PermissionAccess::Denied {
            log::info!(
                "Windows microphone permissions are denied; forcing main window visible for onboarding"
            );
            return true;
        }
    }

    false
}

#[derive(Debug)]
struct StartupError {
    step: &'static str,
    source: anyhow::Error,
}

impl StartupError {
    fn new(step: &'static str, source: impl Into<anyhow::Error>) -> Self {
        Self {
            step,
            source: source.into(),
        }
    }
}

impl std::fmt::Display for StartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.step, self.source)
    }
}

impl std::error::Error for StartupError {}

fn init_step<T>(
    step: &'static str,
    result: Result<T, impl Into<anyhow::Error>>,
) -> Result<T, StartupError> {
    result.map_err(|source| StartupError::new(step, source))
}

fn initialize_core_logic(app_handle: &AppHandle) -> Result<(), StartupError> {
    if let Some(error) = native_smoke_forced_startup_failure() {
        return Err(error);
    }

    // Note: Enigo (keyboard/mouse simulation) is NOT initialized here.
    // The frontend is responsible for calling the `initialize_enigo` command
    // after onboarding completes. This avoids triggering permission dialogs
    // on macOS before the user is ready.

    // Initialize the managers
    let recording_manager = Arc::new(init_step(
        "recording manager",
        AudioRecordingManager::new(app_handle),
    )?);
    let model_manager = Arc::new(init_step("model manager", ModelManager::new(app_handle))?);
    let transcription_manager = Arc::new(init_step(
        "transcription manager",
        TranscriptionManager::new(app_handle, model_manager.clone()),
    )?);
    let history_manager = Arc::new(init_step(
        "history manager",
        HistoryManager::new(app_handle),
    )?);
    let local_llm_manager = Arc::new(init_step(
        "local LLM manager",
        LocalLlmManager::new(app_handle),
    )?);

    // Apply accelerator preferences before any model loads
    managers::transcription::apply_accelerator_settings(app_handle);

    // Add managers to Tauri's managed state
    app_handle.manage(recording_manager.clone());
    app_handle.manage(model_manager.clone());
    app_handle.manage(transcription_manager.clone());
    app_handle.manage(history_manager.clone());
    app_handle.manage(local_llm_manager.clone());
    app_handle.manage(operation_cancellation::OperationCancellationState::default());
    app_handle.manage(adaptive::session::ActiveDictationContext::default());

    // Note: Shortcuts are NOT initialized here.
    // The frontend is responsible for calling the `initialize_shortcuts` command
    // after permissions are confirmed (on macOS) or after onboarding completes.
    // This matches the pattern used for Enigo initialization.

    #[cfg(unix)]
    let signals = init_step("signal handlers", Signals::new(&[SIGUSR1, SIGUSR2]))?;
    // Set up signal handlers for toggling transcription
    #[cfg(unix)]
    signal_handle::setup_signal_handler(app_handle.clone(), signals);

    // Apply macOS Accessory policy if starting hidden and tray is available.
    // If the tray icon is disabled, keep the dock icon so the user can reopen.
    #[cfg(target_os = "macos")]
    {
        let settings = settings::get_settings(app_handle);
        if settings.start_hidden && settings.show_tray_icon {
            let _ = app_handle.set_activation_policy(tauri::ActivationPolicy::Accessory);
        }
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        // Get the current theme to set the appropriate initial icon
        let initial_theme = tray::get_current_theme(app_handle);

        // Choose the appropriate initial icon based on theme
        let initial_icon_path = tray::get_icon_path(initial_theme, tray::TrayIconState::Idle);
        let initial_icon_path = init_step(
            "tray icon path",
            app_handle
                .path()
                .resolve(initial_icon_path, tauri::path::BaseDirectory::Resource),
        )?;
        let initial_icon = init_step("tray icon image", Image::from_path(initial_icon_path))?;

        let tray = init_step(
            "tray icon",
            TrayIconBuilder::new()
                .icon(initial_icon)
                .tooltip(tray::tray_tooltip())
                .show_menu_on_left_click(true)
                .icon_as_template(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        show_main_window(app);
                    }
                    "check_updates" => {
                        let settings = settings::get_settings(app);
                        if settings.update_checks_enabled {
                            show_main_window(app);
                            let _ = app.emit("check-for-updates", ());
                        }
                    }
                    "copy_last_transcript" => {
                        tray::copy_last_transcript(app);
                    }
                    "unload_model" => {
                        let transcription_manager = app.state::<Arc<TranscriptionManager>>();
                        if !transcription_manager.is_model_loaded() {
                            log::warn!("No model is currently loaded.");
                            return;
                        }
                        match transcription_manager.unload_model() {
                            Ok(()) => log::info!("Model unloaded via tray."),
                            Err(e) => log::error!("Failed to unload model via tray: {}", e),
                        }
                    }
                    "cancel" => {
                        use crate::utils::cancel_current_operation;

                        // Use centralized cancellation that handles all operations
                        cancel_current_operation(app);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    id if id.starts_with("model_select:") => {
                        let model_id = id.strip_prefix("model_select:").unwrap().to_string();
                        let current_model = settings::get_settings(app).selected_model;
                        if model_id == current_model {
                            return;
                        }
                        let app_clone = app.clone();
                        std::thread::spawn(move || {
                            match commands::models::switch_active_model(&app_clone, &model_id) {
                                Ok(()) => {
                                    log::info!("Model switched to {} via tray.", model_id);
                                }
                                Err(e) => {
                                    log::error!("Failed to switch model via tray: {}", e);
                                }
                            }
                            tray::update_tray_menu(&app_clone, &tray::TrayIconState::Idle, None);
                        });
                    }
                    _ => {}
                })
                .build(app_handle),
        )?;
        app_handle.manage(tray);

        // Initialize tray menu with idle state
        utils::update_tray_menu(app_handle, &utils::TrayIconState::Idle, None);

        // Apply show_tray_icon setting
        let settings = settings::get_settings(app_handle);
        if !settings.show_tray_icon {
            tray::set_tray_visibility(app_handle, false);
        }

        // Refresh tray menu when model state changes
        let app_handle_for_listener = app_handle.clone();
        app_handle.listen("model-state-changed", move |_| {
            tray::update_tray_menu(&app_handle_for_listener, &tray::TrayIconState::Idle, None);
        });

        // Get the autostart manager and configure based on user setting
        let autostart_manager = app_handle.autolaunch();
        let settings = settings::get_settings(&app_handle);

        if settings.autostart_enabled {
            // Enable autostart if user has opted in
            let _ = autostart_manager.enable();
        } else {
            // Disable autostart if user has opted out
            let _ = autostart_manager.disable();
        }

        // Create the recording overlay window (hidden by default)
        utils::create_recording_overlay(app_handle);
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
fn trigger_update_check(app: AppHandle) -> Result<(), String> {
    let settings = settings::get_settings(&app);
    if !settings.update_checks_enabled {
        return Ok(());
    }
    app.emit("check-for-updates", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn show_main_window_command(app: AppHandle) -> Result<(), String> {
    show_main_window(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
fn get_startup_status(app: AppHandle) -> StartupStatus {
    app.state::<StartupState>().snapshot()
}

fn write_native_smoke_status(status: &NativeSmokeStatus) {
    let Ok(path) = std::env::var("VERBATIM_SMOKE_STATUS_PATH") else {
        return;
    };

    let path = std::path::PathBuf::from(path);
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            log::warn!("Failed to create native smoke status directory: {error}");
            return;
        }
    }

    match serde_json::to_string_pretty(status) {
        Ok(json) => {
            if let Err(error) = std::fs::write(&path, json) {
                log::warn!(
                    "Failed to write native smoke status to {:?}: {}",
                    path,
                    error
                );
            }
        }
        Err(error) => log::warn!("Failed to serialize native smoke status: {error}"),
    }
}

fn schedule_native_smoke_exit(app: AppHandle, status: NativeSmokeStatus) {
    let Ok(value) = std::env::var("VERBATIM_SMOKE_EXIT_AFTER_MS") else {
        return;
    };
    let Ok(delay_ms) = value.parse::<u64>() else {
        log::warn!("Ignoring invalid VERBATIM_SMOKE_EXIT_AFTER_MS value: {value}");
        return;
    };

    write_native_smoke_status(&status);

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        log::info!("Native smoke startup check completed; exiting after {delay_ms}ms");
        app.exit(0);
    });
}

fn run_native_smoke_coordinator_panic_drill(app: &AppHandle) -> Vec<CoordinatorHealthSnapshot> {
    if std::env::var("VERBATIM_SMOKE_COORDINATOR_PANIC_DRILL").as_deref() != Ok("1") {
        return Vec::new();
    }

    let coordinator = app.state::<TranscriptionCoordinator>();
    coordinator.inject_worker_panic_for_smoke();
    wait_for_coordinator_health_events(&coordinator, 1);
    coordinator.inject_worker_panic_for_smoke();
    wait_for_coordinator_health_events(&coordinator, 2);
    coordinator.health_snapshot()
}

fn wait_for_coordinator_health_events(
    coordinator: &tauri::State<'_, TranscriptionCoordinator>,
    expected_count: usize,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1000);
    while std::time::Instant::now() < deadline {
        if coordinator.health_snapshot().len() >= expected_count {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[derive(Clone, Copy, Debug)]
enum NativeSmokeResourceKind {
    File,
    Image,
}

#[derive(Clone, Copy, Debug)]
struct NativeSmokeResource {
    relative_path: &'static str,
    kind: NativeSmokeResourceKind,
}

fn native_smoke_required_resources() -> &'static [NativeSmokeResource] {
    &[
        NativeSmokeResource {
            relative_path: "resources/model_catalog.json",
            kind: NativeSmokeResourceKind::File,
        },
        NativeSmokeResource {
            relative_path: "resources/local_llm_catalog.json",
            kind: NativeSmokeResourceKind::File,
        },
        NativeSmokeResource {
            relative_path: "resources/models/gigaam_vocab.txt",
            kind: NativeSmokeResourceKind::File,
        },
        NativeSmokeResource {
            relative_path: "resources/models/silero_vad_v4.onnx",
            kind: NativeSmokeResourceKind::File,
        },
        NativeSmokeResource {
            relative_path: "resources/marimba_start.wav",
            kind: NativeSmokeResourceKind::File,
        },
        NativeSmokeResource {
            relative_path: "resources/marimba_stop.wav",
            kind: NativeSmokeResourceKind::File,
        },
        NativeSmokeResource {
            relative_path: "resources/pop_start.wav",
            kind: NativeSmokeResourceKind::File,
        },
        NativeSmokeResource {
            relative_path: "resources/pop_stop.wav",
            kind: NativeSmokeResourceKind::File,
        },
        NativeSmokeResource {
            relative_path: "resources/tray_idle.png",
            kind: NativeSmokeResourceKind::Image,
        },
        NativeSmokeResource {
            relative_path: "resources/tray_recording.png",
            kind: NativeSmokeResourceKind::Image,
        },
        NativeSmokeResource {
            relative_path: "resources/tray_transcribing.png",
            kind: NativeSmokeResourceKind::Image,
        },
        NativeSmokeResource {
            relative_path: "resources/tray_idle_dark.png",
            kind: NativeSmokeResourceKind::Image,
        },
        NativeSmokeResource {
            relative_path: "resources/tray_recording_dark.png",
            kind: NativeSmokeResourceKind::Image,
        },
        NativeSmokeResource {
            relative_path: "resources/tray_transcribing_dark.png",
            kind: NativeSmokeResourceKind::Image,
        },
        NativeSmokeResource {
            relative_path: "resources/verbatim.png",
            kind: NativeSmokeResourceKind::Image,
        },
        NativeSmokeResource {
            relative_path: "resources/recording.png",
            kind: NativeSmokeResourceKind::Image,
        },
        NativeSmokeResource {
            relative_path: "resources/transcribing.png",
            kind: NativeSmokeResourceKind::Image,
        },
    ]
}

fn run_native_smoke_resource_probe(app: &AppHandle) -> Vec<String> {
    let mut failures = Vec::new();

    for resource in native_smoke_required_resources() {
        let path = match crate::utils::resolve_resource_path(app, resource.relative_path) {
            Ok(path) => path,
            Err(error) => {
                failures.push(format!("{}: {error}", resource.relative_path));
                continue;
            }
        };

        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => {}
            Ok(metadata) => {
                failures.push(format!(
                    "{}: resolved to non-file or empty resource {} ({} bytes)",
                    resource.relative_path,
                    path.display(),
                    metadata.len()
                ));
                continue;
            }
            Err(error) => {
                failures.push(format!(
                    "{}: missing resolved resource {} ({error})",
                    resource.relative_path,
                    path.display()
                ));
                continue;
            }
        }

        if matches!(resource.kind, NativeSmokeResourceKind::Image) {
            if let Err(error) = Image::from_path(&path) {
                failures.push(format!(
                    "{}: failed to decode image {} ({error})",
                    resource.relative_path,
                    path.display()
                ));
            }
        }
    }

    failures
}

fn collect_native_smoke_retention_status(
    app: &AppHandle,
    settings: &settings::AppSettings,
) -> NativeSmokeRetentionStatus {
    let mut failures = Vec::new();
    let storage_policy_drill = native_smoke_storage_policy_drill(settings);
    let storage_policy_drill_verified = storage_policy_drill.iter().all(|case| case.passed);
    if !storage_policy_drill_verified {
        failures.push("storage_policy_drill_failed".to_string());
    }

    if !settings.history_enabled {
        failures.push("history_enabled=false".to_string());
    }
    if !settings.recordings_enabled {
        failures.push("recordings_enabled=false".to_string());
    }
    if settings.history_limit != 5 {
        failures.push(format!("history_limit={}", settings.history_limit));
    }
    if settings.recording_retention_period != settings::RecordingRetentionPeriod::PreserveLimit {
        failures.push(format!(
            "recording_retention_period={:?}",
            settings.recording_retention_period
        ));
    }

    let (history_entry_count, recording_file_count) = match app.try_state::<Arc<HistoryManager>>() {
        Some(history_manager) => {
            let history_entry_count =
                tauri::async_runtime::block_on(history_manager.get_history_entries(None, None))
                    .map(|history| history.entries.len())
                    .unwrap_or_else(|error| {
                        failures.push(format!("history_query_failed={error}"));
                        usize::MAX
                    });

            let recording_file_count = std::fs::read_dir(history_manager.recordings_dir())
                .map(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .filter(|entry| {
                            entry.file_type().is_ok_and(|file_type| file_type.is_file())
                        })
                        .count()
                })
                .unwrap_or_else(|error| {
                    failures.push(format!("recordings_read_dir_failed={error}"));
                    usize::MAX
                });

            (history_entry_count, recording_file_count)
        }
        None => {
            failures.push("history_manager_missing".to_string());
            (usize::MAX, usize::MAX)
        }
    };

    if history_entry_count != 0 {
        failures.push(format!("history_entry_count={history_entry_count}"));
    }
    if recording_file_count != 0 {
        failures.push(format!("recording_file_count={recording_file_count}"));
    }

    NativeSmokeRetentionStatus {
        history_enabled: settings.history_enabled,
        recordings_enabled: settings.recordings_enabled,
        history_limit: settings.history_limit,
        recording_retention_period: settings.recording_retention_period,
        history_entry_count,
        recording_file_count,
        storage_policy_drill_verified,
        storage_policy_drill,
        clean_profile_verified: failures.is_empty(),
        failures,
    }
}

fn native_smoke_storage_policy_drill(
    settings: &settings::AppSettings,
) -> Vec<NativeSmokeStoragePolicyDrillCase> {
    let mut recordings_disabled = settings.clone();
    recordings_disabled.recordings_enabled = false;

    let mut history_disabled = settings.clone();
    history_disabled.history_enabled = false;
    history_disabled.recordings_enabled = true;

    let cases = [
        ("default", settings.clone(), false, true, true),
        (
            "recordings_disabled",
            recordings_disabled,
            false,
            true,
            false,
        ),
        ("history_disabled", history_disabled, false, false, false),
        ("private_session", settings.clone(), true, false, false),
    ];

    cases
        .into_iter()
        .map(
            |(
                case,
                case_settings,
                private_session_enabled,
                expected_history_enabled,
                expected_recordings_enabled,
            )| {
                let (history_enabled, recordings_enabled) =
                    crate::actions::dictation_storage_policy(
                        &case_settings,
                        private_session_enabled,
                    );
                NativeSmokeStoragePolicyDrillCase {
                    case: case.to_string(),
                    history_enabled,
                    recordings_enabled,
                    expected_history_enabled,
                    expected_recordings_enabled,
                    passed: history_enabled == expected_history_enabled
                        && recordings_enabled == expected_recordings_enabled,
                }
            },
        )
        .collect()
}

fn native_smoke_credential_migration_drill(
    credential_store: &crate::credentials::CredentialStoreStatus,
) -> NativeSmokeCredentialMigrationStatus {
    const SMOKE_LEGACY_API_KEY: &str = "__verbatim_native_smoke_legacy_api_key__";
    let smoke_provider_id = format!("__verbatim_native_smoke_migration_{}__", std::process::id());

    let mut failures = Vec::new();
    if !credential_store.available {
        return NativeSmokeCredentialMigrationStatus {
            checked: true,
            skipped: true,
            available: false,
            retained_legacy_api_key_count: 0,
            legacy_key_removed_from_settings: false,
            credential_round_trip_verified: false,
            cleanup_succeeded: true,
            leaked_probe_secret: credential_store
                .message
                .as_deref()
                .is_some_and(|message| message.contains(SMOKE_LEGACY_API_KEY)),
            failures,
        };
    }

    let _ = crate::credentials::delete_post_process_api_key(&smoke_provider_id);

    let mut settings = settings::get_default_settings();
    settings
        .post_process_api_keys
        .insert(smoke_provider_id.clone(), SMOKE_LEGACY_API_KEY.to_string());

    let changed = crate::credentials::prepare_post_process_api_keys_for_store(
        &mut settings,
        crate::credentials::CredentialStoreFailurePolicy::PreserveLegacyValue,
    );
    if !changed {
        failures.push("migration_did_not_rewrite_settings".to_string());
    }

    let retained_legacy_api_key_count =
        crate::credentials::retained_legacy_api_key_count(&settings);
    if retained_legacy_api_key_count != 0 {
        failures.push(format!(
            "retained_legacy_api_key_count={retained_legacy_api_key_count}"
        ));
    }

    let stored_settings_value = settings
        .post_process_api_keys
        .get(&smoke_provider_id)
        .map(String::as_str)
        .unwrap_or_default();
    let legacy_key_removed_from_settings = stored_settings_value.trim().is_empty();
    if !legacy_key_removed_from_settings {
        failures.push("legacy_key_remaining_in_settings".to_string());
    }

    let credential_round_trip_verified =
        match crate::credentials::get_post_process_api_key(&smoke_provider_id) {
            Ok(Some(value)) if value == SMOKE_LEGACY_API_KEY => true,
            Ok(Some(_)) => {
                failures.push("credential_round_trip_value_mismatch".to_string());
                false
            }
            Ok(None) => {
                failures.push("credential_round_trip_missing".to_string());
                false
            }
            Err(error) => {
                failures.push(format!("credential_round_trip_failed={error}"));
                false
            }
        };

    let cleanup_succeeded =
        match crate::credentials::delete_post_process_api_key(&smoke_provider_id) {
            Ok(()) => true,
            Err(error) => {
                failures.push(format!("credential_cleanup_failed={error}"));
                false
            }
        };

    let serialized_settings = serde_json::to_string(&settings)
        .unwrap_or_else(|error| format!("serialize_failed={error}"));
    let leaked_probe_secret = serialized_settings.contains(SMOKE_LEGACY_API_KEY)
        || credential_store
            .message
            .as_deref()
            .is_some_and(|message| message.contains(SMOKE_LEGACY_API_KEY));
    if leaked_probe_secret {
        failures.push("legacy_api_key_leaked_in_status_or_settings".to_string());
    }

    NativeSmokeCredentialMigrationStatus {
        checked: true,
        skipped: false,
        available: true,
        retained_legacy_api_key_count,
        legacy_key_removed_from_settings,
        credential_round_trip_verified,
        cleanup_succeeded,
        leaked_probe_secret,
        failures,
    }
}

fn native_smoke_audio_fixture_samples() -> Vec<f32> {
    const SAMPLE_RATE: usize = 16_000;
    const DURATION_SECONDS: usize = 2;
    const TONE_HZ: f32 = 440.0;

    (0..(SAMPLE_RATE * DURATION_SECONDS))
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE as f32;
            let envelope = if index < SAMPLE_RATE / 10 {
                index as f32 / (SAMPLE_RATE / 10) as f32
            } else if index > SAMPLE_RATE * DURATION_SECONDS - SAMPLE_RATE / 10 {
                (SAMPLE_RATE * DURATION_SECONDS - index) as f32 / (SAMPLE_RATE / 10) as f32
            } else {
                1.0
            };
            (std::f32::consts::TAU * TONE_HZ * t).sin() * 0.2 * envelope
        })
        .collect()
}

fn native_smoke_insertion_safety_drill() -> Vec<NativeSmokeInsertionSafetyDrillCase> {
    use crate::adaptive::types::{InsertionMethod, InsertionReceipt};
    use crate::insertion::{InsertionAttempt, InsertionTransaction};

    fn run_case(case_name: &str, attempt: InsertionAttempt) -> NativeSmokeInsertionSafetyDrillCase {
        let mut paste_callback_invoked = false;
        let mut transaction = InsertionTransaction::new(|request| {
            paste_callback_invoked = true;
            InsertionReceipt {
                attempted: true,
                succeeded: true,
                method: InsertionMethod::Clipboard,
                target_verified: request.target_verified,
                error: None,
            }
        });
        let outcome = transaction.run(attempt);
        let error = outcome.receipt.error.clone();
        let passed = !paste_callback_invoked
            && !outcome.receipt.attempted
            && !outcome.receipt.target_verified
            && error.as_deref() == Some("target changed before insertion");

        NativeSmokeInsertionSafetyDrillCase {
            case: case_name.to_string(),
            paste_callback_invoked,
            attempted: outcome.receipt.attempted,
            target_verified: outcome.receipt.target_verified,
            error,
            passed,
        }
    }

    vec![
        run_case(
            "adaptive_target_changed_blocks_paste",
            InsertionAttempt::adaptive_target_changed(),
        ),
        run_case(
            "classic_target_changed_blocks_paste",
            InsertionAttempt::classic_target_changed(),
        ),
    ]
}

fn prepare_native_smoke_audio_fixture() -> (Option<String>, usize, bool) {
    let Ok(path) = std::env::var("VERBATIM_SMOKE_AUDIO_FIXTURE_PATH") else {
        return (None, 0, false);
    };
    let path = std::path::PathBuf::from(path);
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            log::warn!("Failed to create native smoke audio fixture directory: {error}");
            return (Some(path.display().to_string()), 0, false);
        }
    }

    let samples = native_smoke_audio_fixture_samples();
    let sample_count = samples.len();
    let verified = crate::audio_toolkit::save_wav_file(&path, &samples)
        .and_then(|_| crate::audio_toolkit::verify_wav_file(&path, sample_count))
        .and_then(|_| crate::audio_toolkit::read_wav_samples(&path).map(|read| read.len()))
        .map(|read_count| read_count == sample_count)
        .unwrap_or_else(|error| {
            log::warn!("Failed to prepare native smoke audio fixture: {error}");
            false
        });

    (Some(path.display().to_string()), sample_count, verified)
}

pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            shortcut::change_binding,
            shortcut::reset_binding,
            shortcut::change_ptt_setting,
            shortcut::change_audio_feedback_setting,
            shortcut::change_audio_feedback_volume_setting,
            shortcut::change_sound_theme_setting,
            shortcut::change_start_hidden_setting,
            shortcut::change_autostart_setting,
            shortcut::change_translate_to_english_setting,
            shortcut::change_translation_target_language_setting,
            shortcut::change_selected_language_setting,
            shortcut::change_dictation_language_mode_setting,
            shortcut::change_overlay_position_setting,
            shortcut::change_docked_pill_setting,
            shortcut::set_recording_overlay_expanded,
            shortcut::change_debug_mode_setting,
            shortcut::change_word_correction_threshold_setting,
            shortcut::change_extra_recording_buffer_setting,
            shortcut::change_paste_delay_ms_setting,
            shortcut::change_paste_method_setting,
            shortcut::get_available_typing_tools,
            shortcut::change_typing_tool_setting,
            shortcut::change_external_script_path_setting,
            shortcut::change_clipboard_handling_setting,
            shortcut::change_auto_submit_setting,
            shortcut::change_auto_submit_key_setting,
            shortcut::change_post_process_enabled_setting,
            shortcut::change_formatting_level_setting,
            shortcut::change_experimental_enabled_setting,
            shortcut::change_post_process_base_url_setting,
            shortcut::change_post_process_api_key_setting,
            shortcut::change_post_process_model_setting,
            shortcut::set_post_process_provider,
            shortcut::fetch_post_process_models,
            shortcut::add_post_process_prompt,
            shortcut::update_post_process_prompt,
            shortcut::delete_post_process_prompt,
            shortcut::set_post_process_selected_prompt,
            shortcut::update_custom_words,
            shortcut::change_auto_add_dictionary_words_setting,
            shortcut::change_adaptive_profiles_enabled_setting,
            shortcut::change_context_awareness_enabled_setting,
            shortcut::change_context_nearby_text_enabled_setting,
            shortcut::change_adaptive_language_shortlist_setting,
            shortcut::change_adaptive_default_profile_setting,
            shortcut::reset_adaptive_profiles,
            shortcut::suspend_binding,
            shortcut::resume_binding,
            shortcut::change_mute_while_recording_setting,
            shortcut::change_append_trailing_space_setting,
            shortcut::change_lazy_stream_close_setting,
            shortcut::change_app_language_setting,
            shortcut::change_update_checks_setting,
            shortcut::change_keyboard_implementation_setting,
            shortcut::get_keyboard_implementation,
            shortcut::change_show_tray_icon_setting,
            shortcut::change_whisper_accelerator_setting,
            shortcut::change_ort_accelerator_setting,
            shortcut::change_whisper_gpu_device,
            shortcut::get_available_accelerators,
            shortcut::verbatim_keys::start_verbatim_keys_recording,
            shortcut::verbatim_keys::stop_verbatim_keys_recording,
            trigger_update_check,
            show_main_window_command,
            get_startup_status,
            commands::cancel_operation,
            commands::is_portable,
            commands::get_app_dir_path,
            commands::get_app_settings,
            commands::get_credential_store_status,
            commands::get_linux_environment_status,
            commands::get_default_settings,
            commands::get_log_dir_path,
            commands::set_log_level,
            commands::open_recordings_folder,
            commands::open_log_dir,
            commands::open_app_data_dir,
            commands::reset_settings_to_defaults,
            commands::get_private_session_status,
            commands::set_private_session_enabled,
            commands::check_apple_intelligence_available,
            commands::initialize_enigo,
            commands::initialize_shortcuts,
            commands::adaptive::get_adaptive_profiles,
            commands::adaptive::reset_adaptive_correction_memory,
            commands::adaptive::set_adaptive_correction_memory_enabled,
            commands::adaptive::reprocess_last_adaptive_entry,
            commands::dictionary::list_dictionary_entries,
            commands::dictionary::add_dictionary_entry,
            commands::dictionary::update_dictionary_entry,
            commands::dictionary::delete_dictionary_entry,
            commands::dictionary::undo_dictionary_entries,
            commands::dictionary::learn_custom_words_from_correction,
            commands::snippets::list_snippet_entries,
            commands::snippets::add_snippet_entry,
            commands::snippets::update_snippet_entry,
            commands::snippets::delete_snippet_entry,
            commands::local_llm::list_local_llm_models,
            commands::local_llm::download_local_llm_model,
            commands::local_llm::cancel_local_llm_download,
            commands::local_llm::delete_local_llm_model,
            commands::local_llm::select_local_llm_model,
            commands::local_llm::set_local_llm_enabled,
            commands::models::get_available_models,
            commands::models::get_model_info,
            commands::models::download_model,
            commands::models::delete_model,
            commands::models::cancel_download,
            commands::models::set_active_model,
            commands::models::get_current_model,
            commands::models::get_transcription_model_status,
            commands::models::is_model_loading,
            commands::models::has_any_models_available,
            commands::models::has_any_models_or_downloads,
            commands::audio::update_microphone_mode,
            commands::audio::get_microphone_mode,
            commands::audio::get_windows_microphone_permission_status,
            commands::audio::open_microphone_privacy_settings,
            commands::audio::get_available_microphones,
            commands::audio::set_selected_microphone,
            commands::audio::get_selected_microphone,
            commands::audio::start_microphone_test,
            commands::audio::stop_microphone_test,
            commands::audio::start_onboarding_dictation_test,
            commands::audio::stop_onboarding_dictation_test,
            commands::audio::cancel_onboarding_dictation_test,
            commands::audio::copy_onboarding_dictation_text,
            commands::audio::get_available_output_devices,
            commands::audio::set_selected_output_device,
            commands::audio::get_selected_output_device,
            commands::audio::play_test_sound,
            commands::audio::check_custom_sounds,
            commands::audio::set_clamshell_microphone,
            commands::audio::get_clamshell_microphone,
            commands::audio::is_recording,
            commands::audio::retry_current_recording,
            commands::transcription::set_model_unload_timeout,
            commands::transcription::get_model_load_status,
            commands::transcription::unload_model_manually,
            commands::history::get_history_entries,
            commands::history::toggle_history_entry_saved,
            commands::history::get_audio_file_path,
            commands::history::delete_history_entry,
            commands::history::clear_history,
            commands::history::clear_recordings,
            commands::history::retry_history_entry_transcription,
            commands::history::update_history_enabled,
            commands::history::update_history_limit,
            commands::history::update_recordings_enabled,
            commands::history::update_recording_retention_period,
            commands::transcript::copy_last_transcript,
            commands::transcript::copy_last_transform_result,
            commands::transcript::paste_last_transcript,
            commands::transform::transform_selected_text,
            helpers::clamshell::is_laptop,
        ])
        .events(collect_events![managers::history::HistoryUpdatePayload,])
}

#[cfg_attr(
    any(target_os = "android", target_os = "ios"),
    tauri::mobile_entry_point
)]
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn run() {
    run_inner(CliArgs::default());
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn run(cli_args: CliArgs) {
    run_inner(cli_args);
}

fn run_inner(cli_args: CliArgs) {
    // Detect portable mode before anything else
    portable::init();

    // Parse console logging directives from RUST_LOG, falling back to info-level logging
    // when the variable is unset
    let console_filter = build_console_filter();

    let specta_builder = specta_builder();

    #[cfg(all(debug_assertions, not(any(target_os = "android", target_os = "ios"))))]
    specta_builder
        .export(
            Typescript::default().bigint(BigIntExportBehavior::Number),
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    let invoke_handler = specta_builder.invoke_handler();

    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .device_event_filter(tauri::DeviceEventFilter::Always)
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            LogBuilder::new()
                .level(log::LevelFilter::Trace) // Set to most verbose level globally
                .max_file_size(500_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .clear_targets()
                .targets([
                    // Console output respects RUST_LOG environment variable
                    Target::new(TargetKind::Stdout).filter({
                        let console_filter = console_filter.clone();
                        move |metadata| console_filter.enabled(metadata)
                    }),
                    // File logs respect the user's settings (stored in FILE_LOG_LEVEL atomic)
                    Target::new(if let Some(data_dir) = portable::data_dir() {
                        TargetKind::Folder {
                            path: data_dir.join("logs"),
                            file_name: Some("verbatim".into()),
                        }
                    } else {
                        TargetKind::LogDir {
                            file_name: Some("verbatim".into()),
                        }
                    })
                    .filter(|metadata| {
                        let file_level = FILE_LOG_LEVEL.load(Ordering::Relaxed);
                        metadata.level() <= level_filter_from_u8(file_level)
                    }),
                ])
                .build(),
        );

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_nspanel::init());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--toggle-transcription") {
                signal_handle::send_transcription_input(app, "transcribe", "CLI");
            } else if args.iter().any(|a| a == "--toggle-post-process") {
                signal_handle::send_transcription_input(app, "transcribe_with_post_process", "CLI");
            } else if args.iter().any(|a| a == "--cancel") {
                crate::utils::cancel_current_operation(app);
            } else {
                show_main_window(app);
            }
        }));
    }

    builder = builder
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build());

    #[cfg(target_os = "macos")]
    {
        builder = builder.plugin(tauri_plugin_macos_permissions::init());
    }

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_global_shortcut::Builder::new().build())
            .plugin(tauri_plugin_autostart::init(
                MacosLauncher::LaunchAgent,
                Some(vec![]),
            ));
    }

    builder
        .manage(cli_args.clone())
        .manage(StartupState::default())
        .manage(private_session::PrivateSessionState::default())
        .manage(credentials::SessionCredentialState::default())
        .setup(move |app| {
            specta_builder.mount_events(app);

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                // Create main window programmatically so we can set data_directory
                // for portable mode (redirects WebView2 cache to portable Data dir)
                let mut win_builder = tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("/".into()),
                )
                .title("Verbatim")
                .inner_size(680.0, 570.0)
                .min_inner_size(680.0, 570.0)
                .resizable(true)
                .maximizable(false)
                .visible(false);

                if let Some(data_dir) = portable::data_dir() {
                    win_builder = win_builder.data_directory(data_dir.join("webview"));
                }

                win_builder.build()?;
            }

            let mut settings = get_settings(&app.handle());
            apply_native_smoke_microphone_selection(&app.handle(), &mut settings);

            // CLI --debug flag overrides debug_mode and log level (runtime-only, not persisted)
            if cli_args.debug {
                settings.debug_mode = true;
                settings.log_level = settings::LogLevel::Trace;
            }

            let tauri_log_level: tauri_plugin_log::LogLevel = settings.log_level.into();
            let file_log_level: log::Level = tauri_log_level.into();
            // Store the file log level in the atomic for the filter to use
            FILE_LOG_LEVEL.store(file_log_level.to_level_filter() as u8, Ordering::Relaxed);
            let app_handle = app.handle().clone();
            app.manage(TranscriptionCoordinator::new(app_handle.clone()));

            let startup_state = app.state::<StartupState>();
            match initialize_core_logic(&app_handle) {
                Ok(()) => {
                    startup_state.set_ready();
                }
                Err(error) => {
                    log::error!("Verbatim startup failed: {}", error);
                    startup_state.set_failed(error.step, error.source);
                    show_main_window(&app_handle);
                    schedule_native_smoke_exit(
                        app_handle,
                        NativeSmokeStatus {
                            startup_status: startup_state.snapshot(),
                            settings_loaded: true,
                            main_window_created: true,
                            tray_initialized: false,
                            tray_visible_requested: false,
                            no_tray_cli: cli_args.no_tray,
                            updater_plugin_registered: true,
                            single_instance_plugin_registered: true,
                            close_to_tray_handler_registered: false,
                            debug_mode_enabled: settings.debug_mode,
                            selected_microphone: settings
                                .selected_microphone
                                .clone()
                                .unwrap_or_else(|| "default".to_string()),
                            selected_model_configured: false,
                            selected_model_id: String::new(),
                            selected_model_downloaded: false,
                            selected_model_custom: false,
                            selected_model_has_remote_url: false,
                            coordinator_health_events: Vec::new(),
                            audio_fixture_path: None,
                            audio_fixture_sample_count: 0,
                            audio_fixture_verified: false,
                            resource_probe_checked: false,
                            resource_probe_failures: Vec::new(),
                            retention: None,
                            linux_environment: crate::linux_readiness::linux_environment_status(),
                            credential_store:
                                crate::credentials::credential_store_status_for_settings(&settings),
                            credential_migration: None,
                            model_load_fallback_drill:
                                managers::transcription::model_load_cpu_fallback_drill(),
                            insertion_safety_drill: Vec::new(),
                            clipboard_safety_drill: Vec::new(),
                        },
                    );
                    return Ok(());
                }
            }

            // Pre-warm GPU/accelerator enumeration on a background thread.
            // The first call into transcribe_rs::whisper_cpp::gpu::list_gpu_devices
            // loads the Metal/Vulkan backend and probes devices, which can take
            // several seconds. Without this, that cost is paid synchronously the
            // first time the user opens the Advanced settings page (which calls
            // the get_available_accelerators command), causing a UI freeze.
            // Result is cached in a OnceLock inside the transcription manager.
            std::thread::spawn(|| {
                let _ = crate::managers::transcription::get_available_accelerators();
            });

            // If start_hidden but tray is disabled, we must show the window
            // anyway. Without a tray icon, the dock is the only way back in.
            let tray_available = settings.show_tray_icon && !cli_args.no_tray;

            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            {
                // Hide tray icon if --no-tray was passed
                if cli_args.no_tray {
                    tray::set_tray_visibility(&app_handle, false);
                }

                // Show main window only if not starting hidden.
                // CLI --start-hidden flag overrides the setting.
                // But if permission onboarding is required, always show the window.
                let should_hide = settings.start_hidden || cli_args.start_hidden;
                let should_force_show = should_force_show_permissions_window(&app_handle);

                if should_force_show || !should_hide || !tray_available {
                    show_main_window(&app_handle);
                }
            }

            let selected_model_info = if settings.selected_model.trim().is_empty() {
                None
            } else {
                app_handle
                    .state::<Arc<ModelManager>>()
                    .get_model_info(&settings.selected_model)
            };
            let coordinator_health_events = run_native_smoke_coordinator_panic_drill(&app_handle);
            let (audio_fixture_path, audio_fixture_sample_count, audio_fixture_verified) =
                prepare_native_smoke_audio_fixture();
            let resource_probe_failures = run_native_smoke_resource_probe(&app_handle);
            let retention = collect_native_smoke_retention_status(&app_handle, &settings);
            let linux_environment = crate::linux_readiness::linux_environment_status();
            let credential_store =
                crate::credentials::credential_store_status_for_settings(&settings);
            let credential_migration = native_smoke_credential_migration_drill(&credential_store);
            let model_load_fallback_drill =
                managers::transcription::model_load_cpu_fallback_drill();
            let insertion_safety_drill = native_smoke_insertion_safety_drill();
            let clipboard_safety_drill = crate::clipboard::native_smoke_clipboard_safety_drill();

            schedule_native_smoke_exit(
                app_handle,
                NativeSmokeStatus {
                    startup_status: startup_state.snapshot(),
                    settings_loaded: true,
                    main_window_created: true,
                    tray_initialized: true,
                    tray_visible_requested: tray_available,
                    no_tray_cli: cli_args.no_tray,
                    updater_plugin_registered: true,
                    single_instance_plugin_registered: true,
                    close_to_tray_handler_registered: true,
                    debug_mode_enabled: settings.debug_mode,
                    selected_microphone: settings
                        .selected_microphone
                        .clone()
                        .unwrap_or_else(|| "default".to_string()),
                    selected_model_configured: !settings.selected_model.trim().is_empty(),
                    selected_model_id: settings.selected_model.clone(),
                    selected_model_downloaded: selected_model_info
                        .as_ref()
                        .is_some_and(|model| model.is_downloaded),
                    selected_model_custom: selected_model_info
                        .as_ref()
                        .is_some_and(|model| model.is_custom),
                    selected_model_has_remote_url: selected_model_info
                        .as_ref()
                        .and_then(|model| model.url.as_ref())
                        .is_some(),
                    coordinator_health_events,
                    audio_fixture_path,
                    audio_fixture_sample_count,
                    audio_fixture_verified,
                    resource_probe_checked: true,
                    resource_probe_failures,
                    retention: Some(retention),
                    linux_environment,
                    credential_store,
                    credential_migration: Some(credential_migration),
                    model_load_fallback_drill,
                    insertion_safety_drill,
                    clipboard_safety_drill,
                },
            );

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                #[cfg(any(target_os = "android", target_os = "ios"))]
                let _ = api;

                #[cfg(not(any(target_os = "android", target_os = "ios")))]
                {
                    api.prevent_close();
                    let _res = window.hide();

                    #[cfg(target_os = "macos")]
                    {
                        let settings = get_settings(&window.app_handle());
                        let tray_visible = settings.show_tray_icon
                            && !window.app_handle().state::<CliArgs>().no_tray;
                        if tray_visible {
                            // Tray is available: hide the dock icon, app lives in the tray
                            let res = window
                                .app_handle()
                                .set_activation_policy(tauri::ActivationPolicy::Accessory);
                            if let Err(e) = res {
                                log::error!("Failed to set activation policy: {}", e);
                            }
                        }
                        // No tray: keep the dock icon visible so the user can reopen
                    }
                }
            }
            tauri::WindowEvent::ThemeChanged(theme) => {
                log::info!("Theme changed to: {:?}", theme);
                // Update tray icon to match new theme, maintaining idle state
                utils::change_tray_icon(&window.app_handle(), utils::TrayIconState::Idle);
            }
            _ => {}
        })
        .invoke_handler(invoke_handler)
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = &event {
                show_main_window(app);
            }
            let _ = (app, event); // suppress unused warnings on non-macOS
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_state_records_failed_step_and_sanitized_message() {
        let state = StartupState::default();

        state.set_failed("model manager", "failed to initialize test model");

        match state.snapshot() {
            StartupStatus::Failed { step, message } => {
                assert_eq!(step, "model manager");
                assert_eq!(message, "failed to initialize test model");
            }
            status => panic!("expected failed startup status, got {status:?}"),
        }
    }

    #[test]
    fn native_smoke_failed_status_serializes_recovery_fields() {
        let settings = settings::get_default_settings();
        let status = NativeSmokeStatus {
            startup_status: StartupStatus::Failed {
                step: "native smoke forced startup failure".to_string(),
                message: "forced startup failure for packaged smoke recovery drill".to_string(),
            },
            settings_loaded: true,
            main_window_created: true,
            tray_initialized: false,
            tray_visible_requested: false,
            no_tray_cli: true,
            updater_plugin_registered: true,
            single_instance_plugin_registered: true,
            close_to_tray_handler_registered: false,
            debug_mode_enabled: true,
            selected_microphone: "default".to_string(),
            selected_model_configured: false,
            selected_model_id: String::new(),
            selected_model_downloaded: false,
            selected_model_custom: false,
            selected_model_has_remote_url: false,
            coordinator_health_events: Vec::new(),
            audio_fixture_path: None,
            audio_fixture_sample_count: 0,
            audio_fixture_verified: false,
            resource_probe_checked: false,
            resource_probe_failures: Vec::new(),
            retention: None,
            linux_environment: crate::linux_readiness::linux_environment_status(),
            credential_store: crate::credentials::CredentialStoreStatus {
                available: false,
                platform: "test".to_string(),
                message: Some("not checked".to_string()),
                retained_legacy_api_key_count: 0,
            },
            credential_migration: None,
            model_load_fallback_drill: Vec::new(),
            insertion_safety_drill: Vec::new(),
            clipboard_safety_drill: Vec::new(),
        };

        let json = serde_json::to_value(&status).expect("status should serialize");

        assert_eq!(json["startup_status"]["status"], "failed");
        assert_eq!(
            json["startup_status"]["step"],
            "native smoke forced startup failure"
        );
        assert_eq!(json["settings_loaded"], true);
        assert_eq!(json["main_window_created"], true);
        assert_eq!(json["tray_initialized"], false);
    }

    #[test]
    fn native_smoke_audio_fixture_samples_are_deterministic_and_non_silent() {
        let samples = native_smoke_audio_fixture_samples();

        assert_eq!(samples.len(), 32_000);
        assert_eq!(samples[0], 0.0);
        assert!(
            samples.iter().any(|sample| sample.abs() > 0.1),
            "fixture should contain a clear tone"
        );
        assert!(
            samples.iter().all(|sample| sample.abs() <= 0.2),
            "fixture should stay in a conservative amplitude range"
        );
    }

    #[test]
    fn native_smoke_resource_probe_covers_packaged_security_sensitive_assets() {
        let paths = native_smoke_required_resources()
            .iter()
            .map(|resource| resource.relative_path)
            .collect::<Vec<_>>();

        assert!(paths.contains(&"resources/model_catalog.json"));
        assert!(paths.contains(&"resources/local_llm_catalog.json"));
        assert!(paths.contains(&"resources/models/silero_vad_v4.onnx"));
        assert!(paths.contains(&"resources/marimba_start.wav"));
        assert!(paths.contains(&"resources/tray_idle.png"));
        assert!(paths.contains(&"resources/tray_idle_dark.png"));
        assert!(paths.contains(&"resources/verbatim.png"));
    }

    #[test]
    fn native_smoke_insertion_safety_drill_blocks_target_changed_paste() {
        let cases = native_smoke_insertion_safety_drill();

        assert_eq!(cases.len(), 2);
        assert!(cases.iter().all(|case| case.passed));
        for case in &cases {
            assert!(!case.paste_callback_invoked, "{}", case.case);
            assert!(!case.attempted, "{}", case.case);
            assert!(!case.target_verified, "{}", case.case);
            assert_eq!(
                case.error.as_deref(),
                Some("target changed before insertion"),
                "{}",
                case.case
            );
        }
    }

    #[test]
    fn native_smoke_failed_status_does_not_claim_retention_probe() {
        let settings = settings::get_default_settings();
        let status = NativeSmokeStatus {
            startup_status: StartupStatus::Failed {
                step: "native smoke forced startup failure".to_string(),
                message: "forced startup failure for packaged smoke recovery drill".to_string(),
            },
            settings_loaded: true,
            main_window_created: true,
            tray_initialized: false,
            tray_visible_requested: false,
            no_tray_cli: true,
            updater_plugin_registered: true,
            single_instance_plugin_registered: true,
            close_to_tray_handler_registered: false,
            debug_mode_enabled: true,
            selected_microphone: "default".to_string(),
            selected_model_configured: false,
            selected_model_id: String::new(),
            selected_model_downloaded: false,
            selected_model_custom: false,
            selected_model_has_remote_url: false,
            coordinator_health_events: Vec::new(),
            audio_fixture_path: None,
            audio_fixture_sample_count: 0,
            audio_fixture_verified: false,
            resource_probe_checked: false,
            resource_probe_failures: Vec::new(),
            retention: None,
            linux_environment: crate::linux_readiness::linux_environment_status(),
            credential_store: crate::credentials::CredentialStoreStatus {
                available: false,
                platform: "test".to_string(),
                message: Some("not checked".to_string()),
                retained_legacy_api_key_count: 0,
            },
            credential_migration: None,
            model_load_fallback_drill: Vec::new(),
            insertion_safety_drill: Vec::new(),
            clipboard_safety_drill: Vec::new(),
        };

        let json = serde_json::to_value(&status).expect("status should serialize");

        assert_eq!(json["retention"], serde_json::Value::Null);
    }

    #[test]
    fn native_smoke_storage_policy_drill_covers_retention_controls() {
        let settings = settings::get_default_settings();

        let cases = native_smoke_storage_policy_drill(&settings);

        assert_eq!(cases.len(), 4);
        assert!(cases.iter().all(|case| case.passed));
        assert!(cases.iter().any(|case| {
            case.case == "private_session" && !case.history_enabled && !case.recordings_enabled
        }));
        assert!(cases.iter().any(|case| {
            case.case == "recordings_disabled" && case.history_enabled && !case.recordings_enabled
        }));
    }

    #[test]
    fn native_smoke_credential_migration_drill_skips_when_store_unavailable() {
        let credential_store = crate::credentials::CredentialStoreStatus {
            available: false,
            platform: "test".to_string(),
            message: Some("OS credential store probe failed".to_string()),
            retained_legacy_api_key_count: 0,
        };

        let drill = native_smoke_credential_migration_drill(&credential_store);

        assert!(drill.checked);
        assert!(drill.skipped);
        assert!(!drill.available);
        assert!(drill.cleanup_succeeded);
        assert!(!drill.leaked_probe_secret);
        assert!(drill.failures.is_empty());
    }
}
