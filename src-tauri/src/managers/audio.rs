use super::mic_diagnostics::{MicDiagnosticState, SilenceDiagnostic};
use crate::audio_toolkit::{list_input_devices, vad::SmoothedVad, AudioRecorder, SileroVad};
use crate::helpers::clamshell;
use crate::settings::{get_settings, write_settings_domain, AppSettings, SettingsWriteDomain};
use crate::utils;
use log::{debug, error, info, warn};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const SELECTED_MICROPHONE_UNAVAILABLE_PREFIX: &str = "Selected microphone unavailable";

fn set_mute(mute: bool) {
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let _ = mute;

    // Expected behavior:
    // - Windows: works on most systems using standard audio drivers.
    // - Linux: works on many systems (PipeWire, PulseAudio, ALSA),
    //   but some distros may lack the tools used.
    // - macOS: works on most standard setups via AppleScript.
    // If unsupported, fails silently.

    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows::Win32::{
                Media::Audio::{
                    eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
                    MMDeviceEnumerator,
                },
                System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
            };

            macro_rules! unwrap_or_return {
                ($expr:expr) => {
                    match $expr {
                        Ok(val) => val,
                        Err(_) => return,
                    }
                };
            }

            // Initialize the COM library for this thread.
            // If already initialized (e.g., by another library like Tauri), this does nothing.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let all_devices: IMMDeviceEnumerator =
                unwrap_or_return!(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL));
            let default_device =
                unwrap_or_return!(all_devices.GetDefaultAudioEndpoint(eRender, eMultimedia));
            let volume_interface = unwrap_or_return!(
                default_device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            );

            let _ = volume_interface.SetMute(mute, std::ptr::null());
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let mute_val = if mute { "1" } else { "0" };
        let amixer_state = if mute { "mute" } else { "unmute" };

        // Try multiple backends to increase compatibility
        // 1. PipeWire (wpctl)
        if Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }

        // 2. PulseAudio (pactl)
        if Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }

        // 3. ALSA (amixer)
        let _ = Command::new("amixer")
            .args(["set", "Master", amixer_state])
            .output();
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let script = format!(
            "set volume output muted {}",
            if mute { "true" } else { "false" }
        );
        let _ = Command::new("osascript").args(["-e", &script]).output();
    }
}

const WHISPER_SAMPLE_RATE: usize = 16000;

/* ──────────────────────────────────────────────────────────────── */

pub(crate) struct RecordingStopResult {
    pub(crate) samples: Vec<f32>,
    pub(crate) captured_sample_count: usize,
    pub(crate) observed_active_signal: bool,
    pub(crate) diagnostic_state: MicDiagnosticState,
    pub(crate) device_error: bool,
    pub(crate) vad_fallback: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RecordingStopOutcome {
    Complete,
    Empty,
    DeviceError,
}

fn stop_result_outcome(device_error: bool, samples_len: usize) -> RecordingStopOutcome {
    if device_error {
        RecordingStopOutcome::DeviceError
    } else if samples_len == 0 {
        RecordingStopOutcome::Empty
    } else {
        RecordingStopOutcome::Complete
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StopSamples {
    Gated,
    RawFallback,
    Empty,
}

fn select_stop_samples(
    gated: impl AsRef<[f32]>,
    raw: impl AsRef<[f32]>,
    observed_active_signal: bool,
) -> StopSamples {
    if !gated.as_ref().is_empty() {
        StopSamples::Gated
    } else if observed_active_signal && !raw.as_ref().is_empty() {
        StopSamples::RawFallback
    } else {
        StopSamples::Empty
    }
}

#[derive(Clone, Serialize)]
struct RecordingErrorEvent {
    error_type: String,
    detail: Option<String>,
}

#[derive(Clone, Debug)]
pub enum RecordingState {
    Idle,
    Recording { binding_id: String },
}

#[derive(Clone, Debug)]
pub enum MicrophoneMode {
    AlwaysOn,
    OnDemand,
}

#[cfg(test)]
pub fn selected_microphone_unavailable_error(device_name: &str) -> anyhow::Error {
    anyhow::anyhow!("{SELECTED_MICROPHONE_UNAVAILABLE_PREFIX}: {device_name}")
}

pub fn is_selected_microphone_unavailable_error(message: &str) -> bool {
    message.starts_with(SELECTED_MICROPHONE_UNAVAILABLE_PREFIX)
}

fn is_default_microphone_selection(device_name: &str) -> bool {
    device_name.eq_ignore_ascii_case("default")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceSelectionInfo<'a> {
    stable_id: Option<&'a str>,
    name: &'a str,
}

fn resolve_device(
    devices: &[DeviceSelectionInfo<'_>],
    stored_id: Option<&str>,
    stored_name: Option<&str>,
) -> Option<usize> {
    if stored_name.is_some_and(is_default_microphone_selection) {
        return None;
    }

    if let Some(stored_id) = stored_id {
        if let Some(index) = devices
            .iter()
            .position(|device| device.stable_id == Some(stored_id))
        {
            return Some(index);
        }
    }

    stored_name.and_then(|stored_name| devices.iter().position(|device| device.name == stored_name))
}

fn stable_id_write_back_is_current(
    current_name: Option<&str>,
    current_id: Option<&str>,
    resolved_name: &str,
    resolved_from_id: Option<&str>,
) -> bool {
    current_name == Some(resolved_name) && current_id == resolved_from_id
}

/* ──────────────────────────────────────────────────────────────── */

fn create_audio_recorder(
    vad_path: &str,
    app_handle: &tauri::AppHandle,
    is_recording: Arc<Mutex<bool>>,
    recording_started_at: Arc<Mutex<Option<Instant>>>,
    mic_diagnostic: Arc<Mutex<SilenceDiagnostic>>,
    mic_diagnostic_state: Arc<Mutex<MicDiagnosticState>>,
) -> Result<AudioRecorder, anyhow::Error> {
    let builder = AudioRecorder::new()
        .map_err(|e| anyhow::anyhow!("Failed to create AudioRecorder: {}", e))?;
    let builder = match SileroVad::new(vad_path, 0.3) {
        Ok(silero) => builder.with_vad(Box::new(SmoothedVad::new(Box::new(silero), 15, 15, 2))),
        Err(err) => {
            error!("VAD init failed ({err}); recording WITHOUT VAD gating");
            let _ = app_handle.emit(
                "recording-error",
                RecordingErrorEvent {
                    error_type: "vad_unavailable_degraded".into(),
                    detail: Some(err.to_string()),
                },
            );
            builder
        }
    };

    // Recorder with optional VAD plus a spectrum-level callback that forwards
    // updates to the frontend.
    let recorder = builder.with_level_callback({
        let app_handle = app_handle.clone();
        move |levels| {
            utils::emit_levels(&app_handle, &levels);

            if !*is_recording.lock().unwrap_or_else(|e| e.into_inner()) {
                return;
            }

            let elapsed = match *recording_started_at
                .lock()
                .unwrap_or_else(|e| e.into_inner())
            {
                Some(started_at) => started_at.elapsed(),
                None => return,
            };

            let next_state = {
                let mut diagnostic = mic_diagnostic.lock().unwrap_or_else(|e| e.into_inner());
                diagnostic.observe_at(&levels, elapsed)
            };

            let mut last_state = mic_diagnostic_state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if next_state != *last_state {
                *last_state = next_state;
                utils::emit_overlay_state_changed(&app_handle, next_state.overlay_state());
            }
        }
    });

    Ok(recorder)
}

/* ──────────────────────────────────────────────────────────────── */

#[derive(Clone)]
pub struct AudioRecordingManager {
    state: Arc<Mutex<RecordingState>>,
    mode: Arc<Mutex<MicrophoneMode>>,
    app_handle: tauri::AppHandle,

    recorder: Arc<Mutex<Option<AudioRecorder>>>,
    is_open: Arc<Mutex<bool>>,
    is_recording: Arc<Mutex<bool>>,
    recording_started_at: Arc<Mutex<Option<Instant>>>,
    mic_diagnostic: Arc<Mutex<SilenceDiagnostic>>,
    mic_diagnostic_state: Arc<Mutex<MicDiagnosticState>>,
    did_mute: Arc<Mutex<bool>>,
    close_generation: Arc<AtomicU64>,
}

impl AudioRecordingManager {
    /* ---------- construction ------------------------------------------------ */

    pub fn new(app: &tauri::AppHandle) -> Result<Self, anyhow::Error> {
        let settings = get_settings(app);
        let mode = if settings.always_on_microphone {
            MicrophoneMode::AlwaysOn
        } else {
            MicrophoneMode::OnDemand
        };

        let manager = Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            mode: Arc::new(Mutex::new(mode.clone())),
            app_handle: app.clone(),

            recorder: Arc::new(Mutex::new(None)),
            is_open: Arc::new(Mutex::new(false)),
            is_recording: Arc::new(Mutex::new(false)),
            recording_started_at: Arc::new(Mutex::new(None)),
            mic_diagnostic: Arc::new(Mutex::new(SilenceDiagnostic::default())),
            mic_diagnostic_state: Arc::new(Mutex::new(MicDiagnosticState::Recording)),
            did_mute: Arc::new(Mutex::new(false)),
            close_generation: Arc::new(AtomicU64::new(0)),
        };

        // Always-on?  Open immediately.
        if matches!(mode, MicrophoneMode::AlwaysOn) {
            manager.start_microphone_stream()?;
        }

        Ok(manager)
    }

    /* ---------- helper methods --------------------------------------------- */

    fn get_effective_microphone_device(
        &self,
        settings: &AppSettings,
    ) -> Result<Option<cpal::Device>, anyhow::Error> {
        // Check if we're in clamshell mode and have a clamshell microphone configured
        let use_clamshell_mic = if let Ok(is_clamshell) = clamshell::is_clamshell() {
            is_clamshell && settings.clamshell_microphone.is_some()
        } else {
            false
        };

        let device_name = if use_clamshell_mic {
            settings.clamshell_microphone.as_ref().map(String::as_str)
        } else {
            settings.selected_microphone.as_deref()
        };
        let stored_id = if use_clamshell_mic {
            None
        } else {
            settings.selected_microphone_id.as_deref()
        };

        let Some(device_name) = device_name else {
            return Ok(None);
        };
        if is_default_microphone_selection(device_name) {
            return Ok(None);
        }

        match list_input_devices() {
            Ok(mut devices) => {
                let resolved_index = {
                    let candidates = devices
                        .iter()
                        .map(|device| DeviceSelectionInfo {
                            stable_id: device.stable_id.as_deref(),
                            name: &device.name,
                        })
                        .collect::<Vec<_>>();
                    resolve_device(&candidates, stored_id, Some(device_name))
                };

                if let Some(index) = resolved_index {
                    let device = devices.swap_remove(index);

                    if !use_clamshell_mic {
                        if let Some(stable_id) = device.stable_id.clone() {
                            if settings.selected_microphone_id.as_deref()
                                != Some(stable_id.as_str())
                            {
                                let stored_stable_id = stable_id.clone();
                                let mut did_persist = false;
                                if let Err(err) = write_settings_domain(
                                    &self.app_handle,
                                    SettingsWriteDomain::Audio,
                                    |current_settings| {
                                        if stable_id_write_back_is_current(
                                            current_settings.selected_microphone.as_deref(),
                                            current_settings.selected_microphone_id.as_deref(),
                                            device_name,
                                            stored_id,
                                        ) {
                                            current_settings.selected_microphone_id =
                                                Some(stored_stable_id);
                                            did_persist = true;
                                        }
                                    },
                                ) {
                                    warn!(
                                        "Failed to persist stable ID for microphone '{}': {}",
                                        device_name, err
                                    );
                                } else if did_persist {
                                    debug!(
                                        "Persisted stable ID for microphone '{}': {}",
                                        device_name, stable_id
                                    );
                                } else {
                                    debug!(
                                        "Skipped stable ID write-back because microphone selection changed from '{}'",
                                        device_name
                                    );
                                }
                            }
                        }
                    }

                    Ok(Some(device.device))
                } else {
                    warn!(
                        "Selected microphone '{}' is unavailable; falling back to default",
                        device_name
                    );
                    let _ = self.app_handle.emit(
                        "recording-error",
                        RecordingErrorEvent {
                            error_type: "selected_microphone_unavailable".to_string(),
                            detail: Some(format!(
                                "selected_microphone={device_name}; fallback_to_default=true"
                            )),
                        },
                    );
                    Ok(None)
                }
            }
            Err(e) => {
                warn!(
                    "Failed to list devices while resolving '{}'; using default: {}",
                    device_name, e
                );
                let _ = self.app_handle.emit(
                    "recording-error",
                    RecordingErrorEvent {
                        error_type: "selected_microphone_unavailable".to_string(),
                        detail: Some(format!(
                            "selected_microphone={device_name}; fallback_to_default=true; error={e}"
                        )),
                    },
                );
                Ok(None)
            }
        }
    }

    fn schedule_lazy_close(&self) {
        let gen = self.close_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let app = self.app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(STREAM_IDLE_TIMEOUT);
            let rm = app.state::<Arc<AudioRecordingManager>>();
            // Hold state lock across the check AND close to serialize against
            // try_start_recording, preventing a race where the stream is closed
            // under an active recording.
            let state = rm.state.lock().unwrap_or_else(|e| e.into_inner());
            if rm.close_generation.load(Ordering::SeqCst) == gen
                && matches!(*state, RecordingState::Idle)
            {
                // stop_microphone_stream does not acquire the state lock,
                // so holding it here is safe (no deadlock).
                info!(
                    "Closing idle microphone stream after {:?}",
                    STREAM_IDLE_TIMEOUT
                );
                rm.stop_microphone_stream();
            }
        });
    }

    fn start_mic_diagnostic(&self) {
        self.mic_diagnostic
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reset();
        *self
            .mic_diagnostic_state
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = MicDiagnosticState::Recording;
        *self
            .recording_started_at
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    fn reset_mic_diagnostic(&self) {
        self.mic_diagnostic
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .reset();
        *self
            .mic_diagnostic_state
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = MicDiagnosticState::Recording;
        *self
            .recording_started_at
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    /* ---------- microphone life-cycle -------------------------------------- */

    /// Applies mute if mute_while_recording is enabled and stream is open
    pub fn apply_mute(&self) {
        let settings = get_settings(&self.app_handle);
        let mut did_mute_guard = self.did_mute.lock().unwrap_or_else(|e| e.into_inner());

        if settings.mute_while_recording && *self.is_open.lock().unwrap_or_else(|e| e.into_inner())
        {
            set_mute(true);
            *did_mute_guard = true;
            debug!("Mute applied");
        }
    }

    /// Removes mute if it was applied
    pub fn remove_mute(&self) {
        let mut did_mute_guard = self.did_mute.lock().unwrap_or_else(|e| e.into_inner());
        if *did_mute_guard {
            set_mute(false);
            *did_mute_guard = false;
            debug!("Mute removed");
        }
    }

    pub fn preload_vad(&self) -> Result<(), anyhow::Error> {
        let mut recorder_opt = self.recorder.lock().unwrap_or_else(|e| e.into_inner());
        if recorder_opt.is_none() {
            let vad_path = crate::utils::resolve_silero_vad_model_path(&self.app_handle)?;
            let vad_path = vad_path.to_string_lossy();
            *recorder_opt = Some(create_audio_recorder(
                &vad_path,
                &self.app_handle,
                Arc::clone(&self.is_recording),
                Arc::clone(&self.recording_started_at),
                Arc::clone(&self.mic_diagnostic),
                Arc::clone(&self.mic_diagnostic_state),
            )?);
        }
        Ok(())
    }

    pub fn start_microphone_stream(&self) -> Result<(), anyhow::Error> {
        let mut open_flag = self.is_open.lock().unwrap_or_else(|e| e.into_inner());
        if *open_flag {
            debug!("Microphone stream already active");
            return Ok(());
        }

        let start_time = Instant::now();

        // Don't mute immediately - caller will handle muting after audio feedback
        let mut did_mute_guard = self.did_mute.lock().unwrap_or_else(|e| e.into_inner());
        *did_mute_guard = false;

        // Get the selected device from settings, considering clamshell mode
        let settings = get_settings(&self.app_handle);
        let selected_device = self.get_effective_microphone_device(&settings)?;

        // Pre-flight check: if no device was selected/configured AND no devices
        // exist at all, fail early with a clear error instead of letting cpal
        // produce a cryptic backend-specific message.
        if selected_device.is_none() {
            let has_any_device = list_input_devices()
                .map(|devices| !devices.is_empty())
                .unwrap_or(false);
            if !has_any_device {
                return Err(anyhow::anyhow!("No input device found"));
            }
        }

        // Ensure VAD is loaded if it wasn't for whatever reason
        self.preload_vad()?;

        let mut recorder_opt = self.recorder.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(rec) = recorder_opt.as_mut() {
            rec.open(selected_device)
                .map_err(|e| anyhow::anyhow!("Failed to open recorder: {}", e))?;
        }

        *open_flag = true;
        // This timing covers through cpal's stream.play() returning — i.e. the
        // point cpal surfaces as "stream running." It does NOT guarantee the
        // host audio device is producing samples yet; the first input callback
        // fires asynchronously one buffer period later (hardware dependent,
        // typically ~10–200ms on macOS, longer on Bluetooth/USB).
        info!(
            "Microphone stream initialized in {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    pub fn stop_microphone_stream(&self) {
        let mut open_flag = self.is_open.lock().unwrap_or_else(|e| e.into_inner());
        if !*open_flag {
            return;
        }

        let mut did_mute_guard = self.did_mute.lock().unwrap_or_else(|e| e.into_inner());
        if *did_mute_guard {
            set_mute(false);
        }
        *did_mute_guard = false;

        if let Some(rec) = self
            .recorder
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            // If still recording, stop first.
            if *self.is_recording.lock().unwrap_or_else(|e| e.into_inner()) {
                let _ = rec.stop();
                *self.is_recording.lock().unwrap_or_else(|e| e.into_inner()) = false;
                self.reset_mic_diagnostic();
            }
            let _ = rec.close();
        }

        *open_flag = false;
        debug!("Microphone stream stopped");
    }

    /* ---------- mode switching --------------------------------------------- */

    pub fn update_mode(&self, new_mode: MicrophoneMode) -> Result<(), anyhow::Error> {
        let cur_mode = self.mode.lock().unwrap_or_else(|e| e.into_inner()).clone();

        match (cur_mode, &new_mode) {
            (MicrophoneMode::AlwaysOn, MicrophoneMode::OnDemand) => {
                if matches!(
                    *self.state.lock().unwrap_or_else(|e| e.into_inner()),
                    RecordingState::Idle
                ) {
                    self.close_generation.fetch_add(1, Ordering::SeqCst);
                    self.stop_microphone_stream();
                }
            }
            (MicrophoneMode::OnDemand, MicrophoneMode::AlwaysOn) => {
                self.close_generation.fetch_add(1, Ordering::SeqCst);
                self.start_microphone_stream()?;
            }
            _ => {}
        }

        *self.mode.lock().unwrap_or_else(|e| e.into_inner()) = new_mode;
        Ok(())
    }

    /* ---------- recording --------------------------------------------------- */

    pub fn try_start_recording(&self, binding_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());

        if let RecordingState::Idle = *state {
            // Ensure microphone is open in on-demand mode
            if matches!(
                *self.mode.lock().unwrap_or_else(|e| e.into_inner()),
                MicrophoneMode::OnDemand
            ) {
                // Cancel any pending lazy close
                self.close_generation.fetch_add(1, Ordering::SeqCst);
                if let Err(e) = self.start_microphone_stream() {
                    let msg = format!("{e}");
                    error!("Failed to open microphone stream: {msg}");
                    return Err(msg);
                }
            }

            if let Some(rec) = self
                .recorder
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                if rec.start().is_ok() {
                    *self.is_recording.lock().unwrap_or_else(|e| e.into_inner()) = true;
                    self.start_mic_diagnostic();
                    *state = RecordingState::Recording {
                        binding_id: binding_id.to_string(),
                    };
                    debug!("Recording started for binding {binding_id}");
                    return Ok(());
                }
            }
            Err("Recorder not available".to_string())
        } else {
            Err("Already recording".to_string())
        }
    }

    pub fn update_selected_device(&self) -> Result<(), anyhow::Error> {
        // If currently open, restart the microphone stream to use the new device
        if *self.is_open.lock().unwrap_or_else(|e| e.into_inner()) {
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            self.stop_microphone_stream();
            self.start_microphone_stream()?;
        }
        Ok(())
    }

    pub(crate) fn stop_recording(&self, binding_id: &str) -> Option<RecordingStopResult> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());

        match *state {
            RecordingState::Recording {
                binding_id: ref active,
            } if active == binding_id => {
                *state = RecordingState::Idle;
                drop(state);

                // Optionally keep recording for a bit longer to capture trailing audio
                let settings = get_settings(&self.app_handle);
                if settings.extra_recording_buffer_ms > 0 {
                    debug!(
                        "Extra recording buffer: sleeping {}ms before stopping",
                        settings.extra_recording_buffer_ms
                    );
                    std::thread::sleep(Duration::from_millis(settings.extra_recording_buffer_ms));
                }

                let stop_output = if let Some(rec) = self
                    .recorder
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                {
                    match rec.stop() {
                        Ok(output) => output,
                        Err(e) => {
                            error!("stop() failed: {e}");
                            crate::audio_toolkit::audio::RecorderStopOutput {
                                samples: Vec::new(),
                                raw_samples: Vec::new(),
                                device_error: false,
                                dropped_resampler_chunks: 0,
                            }
                        }
                    }
                } else {
                    error!("Recorder not available");
                    crate::audio_toolkit::audio::RecorderStopOutput {
                        samples: Vec::new(),
                        raw_samples: Vec::new(),
                        device_error: false,
                        dropped_resampler_chunks: 0,
                    }
                };

                let diagnostic_state = *self
                    .mic_diagnostic_state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let observed_active_signal = self
                    .mic_diagnostic
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .has_observed_active_signal();
                *self.is_recording.lock().unwrap_or_else(|e| e.into_inner()) = false;
                self.reset_mic_diagnostic();
                let crate::audio_toolkit::audio::RecorderStopOutput {
                    samples: gated_samples,
                    raw_samples,
                    device_error,
                    dropped_resampler_chunks,
                } = stop_output;
                if dropped_resampler_chunks > 0 {
                    log::warn!(
                        "Microphone diagnostics: audio resampler dropped {dropped_resampler_chunks} chunk(s) during recording"
                    );
                }
                let sample_selection = if device_error {
                    StopSamples::Empty
                } else {
                    select_stop_samples(&gated_samples, &raw_samples, observed_active_signal)
                };
                let (selected_samples, vad_fallback) = match sample_selection {
                    StopSamples::Gated => (gated_samples, false),
                    StopSamples::RawFallback => {
                        log::warn!(
                            "VAD produced no gated samples despite active mic signal; using {} raw resampled samples",
                            raw_samples.len()
                        );
                        (raw_samples, true)
                    }
                    StopSamples::Empty => (Vec::new(), false),
                };
                let captured_sample_count = selected_samples.len();
                let stop_outcome = stop_result_outcome(device_error, captured_sample_count);
                let device_error = matches!(stop_outcome, RecordingStopOutcome::DeviceError);
                if device_error {
                    error!("Recording stopped after microphone stream error");
                    let _ = self.app_handle.emit(
                        "recording-error",
                        RecordingErrorEvent {
                            error_type: "microphone_disconnected".to_string(),
                            detail: Some("Microphone disconnected during recording".to_string()),
                        },
                    );
                    utils::emit_overlay_state_changed(
                        &self.app_handle,
                        crate::overlay::OverlayState::MicFailed,
                    );
                }

                // In on-demand mode, close the mic (lazily if the setting is enabled)
                if matches!(
                    *self.mode.lock().unwrap_or_else(|e| e.into_inner()),
                    MicrophoneMode::OnDemand
                ) {
                    if get_settings(&self.app_handle).lazy_stream_close {
                        self.schedule_lazy_close();
                    } else {
                        self.stop_microphone_stream();
                    }
                }

                // Pad if very short
                // debug!("Got {} samples", s_len);
                let samples = if device_error {
                    Vec::new()
                } else if captured_sample_count < WHISPER_SAMPLE_RATE && captured_sample_count > 0 {
                    let mut padded = selected_samples;
                    padded.resize(WHISPER_SAMPLE_RATE * 5 / 4, 0.0);
                    padded
                } else {
                    selected_samples
                };

                Some(RecordingStopResult {
                    samples,
                    captured_sample_count,
                    observed_active_signal,
                    diagnostic_state: if device_error {
                        MicDiagnosticState::MicFailed
                    } else {
                        diagnostic_state
                    },
                    device_error,
                    vad_fallback,
                })
            }
            _ => None,
        }
    }
    pub fn is_recording(&self) -> bool {
        matches!(
            *self.state.lock().unwrap_or_else(|e| e.into_inner()),
            RecordingState::Recording { .. }
        )
    }

    pub fn retry_current_recording(&self) -> Result<(), String> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !matches!(*state, RecordingState::Recording { .. }) {
            return Err("No active recording to retry".to_string());
        }

        let recorder_guard = self.recorder.lock().unwrap_or_else(|e| e.into_inner());
        let rec = recorder_guard
            .as_ref()
            .ok_or_else(|| "Recorder not available".to_string())?;

        *self.is_recording.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.reset_mic_diagnostic();

        rec.stop()
            .map_err(|e| format!("Failed to stop current recording: {e}"))?;

        match rec.start() {
            Ok(()) => {
                *self.is_recording.lock().unwrap_or_else(|e| e.into_inner()) = true;
                self.start_mic_diagnostic();
                utils::emit_overlay_state_changed(
                    &self.app_handle,
                    crate::overlay::OverlayState::Recording,
                );
                Ok(())
            }
            Err(e) => {
                *self.is_recording.lock().unwrap_or_else(|e| e.into_inner()) = false;
                self.reset_mic_diagnostic();
                utils::emit_overlay_state_changed(
                    &self.app_handle,
                    crate::overlay::OverlayState::MicFailed,
                );
                Err(format!("Failed to restart recording: {e}"))
            }
        }
    }

    /// Cancel any ongoing recording without returning audio samples
    pub fn cancel_recording(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());

        if let RecordingState::Recording { .. } = *state {
            *state = RecordingState::Idle;
            drop(state);
            self.reset_mic_diagnostic();

            if let Some(rec) = self
                .recorder
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                let _ = rec.stop(); // Discard the result
            }

            *self.is_recording.lock().unwrap_or_else(|e| e.into_inner()) = false;

            // In on-demand mode, close the mic (lazily if the setting is enabled)
            if matches!(
                *self.mode.lock().unwrap_or_else(|e| e.into_inner()),
                MicrophoneMode::OnDemand
            ) {
                if get_settings(&self.app_handle).lazy_stream_close {
                    self.schedule_lazy_close();
                } else {
                    self.stop_microphone_stream();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev<'a>(stable_id: Option<&'a str>, name: &'a str) -> DeviceSelectionInfo<'a> {
        DeviceSelectionInfo { stable_id, name }
    }

    #[test]
    fn device_resolution_prefers_stable_id_over_duplicate_name() {
        let devices = vec![
            dev(Some("wasapi:AAA"), "USB Audio Device"),
            dev(Some("wasapi:BBB"), "USB Audio Device"),
        ];

        assert_eq!(
            resolve_device(&devices, Some("wasapi:BBB"), Some("USB Audio Device")),
            Some(1)
        );
    }

    #[test]
    fn device_resolution_permanently_falls_back_to_name_when_ids_do_not_match_or_error() {
        let devices = vec![
            dev(None, "USB Audio Device"),
            dev(Some("wasapi:BBB"), "USB Audio Device"),
        ];

        assert_eq!(
            resolve_device(&devices, Some("wasapi:GONE"), Some("USB Audio Device")),
            Some(0)
        );
        assert_eq!(
            resolve_device(&devices, None, Some("USB Audio Device")),
            Some(0)
        );
    }

    #[test]
    fn device_resolution_returns_none_for_default_or_no_match() {
        let devices = vec![dev(Some("wasapi:AAA"), "USB Audio Device")];

        assert_eq!(resolve_device(&devices, None, Some("Default")), None);
        assert_eq!(resolve_device(&devices, Some("wasapi:GONE"), None), None);
        assert_eq!(
            resolve_device(&devices, None, Some("Missing Microphone")),
            None
        );
    }

    #[test]
    fn stable_id_write_back_requires_unchanged_name_and_id() {
        assert!(stable_id_write_back_is_current(
            Some("USB Audio Device"),
            Some("wasapi:GONE"),
            "USB Audio Device",
            Some("wasapi:GONE")
        ));
        assert!(!stable_id_write_back_is_current(
            Some("USB Audio Device"),
            Some("wasapi:BBB"),
            "USB Audio Device",
            Some("wasapi:GONE")
        ));
        assert!(!stable_id_write_back_is_current(
            Some("Other Microphone"),
            Some("wasapi:GONE"),
            "USB Audio Device",
            Some("wasapi:GONE")
        ));
    }

    #[test]
    fn empty_vad_output_falls_back_to_raw_when_signal_observed() {
        assert_eq!(
            select_stop_samples(vec![], vec![0.1, 0.2], true),
            StopSamples::RawFallback
        );
        assert_eq!(
            select_stop_samples(vec![], vec![0.1, 0.2], false),
            StopSamples::Empty
        );
        assert_eq!(
            select_stop_samples(vec![0.3], vec![0.1], true),
            StopSamples::Gated
        );
    }

    #[test]
    fn stop_result_outcome_prioritizes_device_error_then_sample_presence() {
        assert_eq!(
            stop_result_outcome(true, 8_000),
            RecordingStopOutcome::DeviceError
        );
        assert_eq!(
            stop_result_outcome(true, 0),
            RecordingStopOutcome::DeviceError
        );
        assert_eq!(stop_result_outcome(false, 0), RecordingStopOutcome::Empty);
        assert_eq!(
            stop_result_outcome(false, 8_000),
            RecordingStopOutcome::Complete
        );
    }

    #[test]
    fn selected_microphone_unavailable_error_names_missing_device() {
        let error = selected_microphone_unavailable_error("OBSBOT Tiny");

        assert!(is_selected_microphone_unavailable_error(&error.to_string()));
        assert!(error.to_string().contains("OBSBOT Tiny"));
    }

    #[test]
    fn default_microphone_selection_is_not_treated_as_missing_device() {
        assert!(is_default_microphone_selection("default"));
        assert!(is_default_microphone_selection("Default"));
        assert!(!is_default_microphone_selection("Default Array Microphone"));
    }
}
