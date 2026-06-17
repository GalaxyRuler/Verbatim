use tauri::{AppHandle, Emitter};

pub fn set_recording_overlay_expanded(_app_handle: &AppHandle, _expanded: bool) {}

pub fn create_recording_overlay(_app_handle: &AppHandle) {}

pub fn show_recording_overlay(app_handle: &AppHandle) {
    emit_overlay_state_changed(app_handle, "recording");
}

pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    emit_overlay_state_changed(app_handle, "transcribing");
}

pub fn show_processing_overlay(app_handle: &AppHandle) {
    emit_overlay_state_changed(app_handle, "processing");
}

pub fn show_docked_overlay(_app_handle: &AppHandle) {}

pub fn update_overlay_position(_app_handle: &AppHandle) {}

pub fn hide_recording_overlay(_app_handle: &AppHandle) {}

pub fn emit_levels(app_handle: &AppHandle, levels: &Vec<f32>) {
    let _ = app_handle.emit("mic-level", levels);
}

pub fn emit_overlay_state_changed(app_handle: &AppHandle, state: &str) {
    let _ = app_handle.emit("overlay-state-changed", state);
}
