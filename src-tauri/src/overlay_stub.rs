use tauri::{AppHandle, Emitter};

/// Android/iOS stub mirror of the desktop `overlay::OverlayState`.
///
/// The real overlay UI is a desktop-only Tauri webview window; on mobile the
/// status is surfaced through a native floating bubble instead. We still keep
/// this enum API-compatible with the desktop one so cross-platform modules
/// (`actions`, `managers`, `utils`) compile unchanged on these targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayState {
    Idle,
    Recording,
    Silence,
    Transcribing,
    Processing,
    Inserted,
    MicFailed,
    Cancelled,
}

impl OverlayState {
    pub fn as_payload(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Recording => "recording",
            Self::Silence => "silence",
            Self::Transcribing => "transcribing",
            Self::Processing => "processing",
            Self::Inserted => "inserted",
            Self::MicFailed => "mic_failed",
            Self::Cancelled => "cancelled",
        }
    }
}

pub fn set_recording_overlay_expanded(_app_handle: &AppHandle, _expanded: bool) {}

pub fn create_recording_overlay(_app_handle: &AppHandle) {}

pub fn show_recording_overlay(app_handle: &AppHandle) {
    emit_overlay_state_changed(app_handle, OverlayState::Recording);
}

pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    emit_overlay_state_changed(app_handle, OverlayState::Transcribing);
}

pub fn show_processing_overlay(app_handle: &AppHandle) {
    emit_overlay_state_changed(app_handle, OverlayState::Processing);
}

pub fn show_docked_overlay(_app_handle: &AppHandle) {}

pub fn update_overlay_position(_app_handle: &AppHandle) {}

pub fn hide_recording_overlay(_app_handle: &AppHandle) {}

pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    let _ = app_handle.emit("mic-level", levels);
}

pub fn emit_overlay_state_changed(app_handle: &AppHandle, state: OverlayState) {
    // No desktop overlay window on mobile; the native floating bubble owns the
    // UI. We still emit the event so any listeners (and the bridge) stay in sync.
    let _ = app_handle.emit("overlay-state-changed", state.as_payload());
}
