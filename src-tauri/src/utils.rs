use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::shortcut;
use crate::TranscriptionCoordinator;
use log::info;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

// Re-export all utility modules for easy access
// pub use crate::audio_feedback::*;
pub use crate::clipboard::*;
pub use crate::overlay::*;
pub use crate::tray::*;

/// Centralized cancellation function that can be called from anywhere in the app.
/// Handles cancelling both recording and transcription operations and updates UI state.
pub fn cancel_current_operation(app: &AppHandle) {
    info!("Initiating operation cancellation...");

    // Unregister the cancel shortcut asynchronously
    shortcut::unregister_cancel_shortcut(app);

    // Cancel any ongoing recording
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    let recording_was_active = audio_manager.is_recording();
    audio_manager.cancel_recording();

    // Update tray icon and hide overlay
    change_tray_icon(app, crate::tray::TrayIconState::Idle);
    hide_recording_overlay(app);

    // Unload model if immediate unload is enabled
    let tm = app.state::<Arc<TranscriptionManager>>();
    tm.maybe_unload_immediately("cancellation");

    // Notify coordinator so it can keep lifecycle state coherent.
    if let Some(coordinator) = app.try_state::<TranscriptionCoordinator>() {
        coordinator.notify_cancel(recording_was_active);
    }

    info!("Operation cancellation completed - returned to idle state");
}

pub fn resolve_resource_path(app: &AppHandle, relative_path: &str) -> anyhow::Result<PathBuf> {
    let resolved = app
        .path()
        .resolve(relative_path, tauri::path::BaseDirectory::Resource)
        .map_err(|e| anyhow::anyhow!("Failed to resolve resource path {relative_path}: {e}"))?;

    if resolved.exists() {
        return Ok(resolved);
    }

    if let Some(flattened_path) = flattened_model_resource_path(relative_path) {
        let flattened = app
            .path()
            .resolve(&flattened_path, tauri::path::BaseDirectory::Resource)
            .map_err(|e| {
                anyhow::anyhow!("Failed to resolve flattened resource path {flattened_path}: {e}")
            })?;

        if flattened.exists() {
            return Ok(flattened);
        }
    }

    Ok(resolved)
}

fn flattened_model_resource_path(relative_path: &str) -> Option<String> {
    relative_path
        .strip_prefix("resources/models/")
        .map(|path| format!("resources/{path}"))
}

/// Check if using the Wayland display server protocol
#[cfg(target_os = "linux")]
pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.to_lowercase() == "wayland")
            .unwrap_or(false)
}

/// Check if running on KDE Plasma desktop environment
#[cfg(target_os = "linux")]
pub fn is_kde_plasma() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_uppercase().contains("KDE"))
        .unwrap_or(false)
        || std::env::var("KDE_SESSION_VERSION").is_ok()
}

/// Check if running on KDE Plasma with Wayland
#[cfg(target_os = "linux")]
pub fn is_kde_wayland() -> bool {
    is_wayland() && is_kde_plasma()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flattened_model_resource_path_maps_bundled_model_resources() {
        assert_eq!(
            flattened_model_resource_path("resources/models/silero_vad_v4.onnx").as_deref(),
            Some("resources/silero_vad_v4.onnx")
        );
        assert_eq!(
            flattened_model_resource_path("resources/models/gigaam_vocab.txt").as_deref(),
            Some("resources/gigaam_vocab.txt")
        );
    }

    #[test]
    fn flattened_model_resource_path_ignores_other_resources() {
        assert_eq!(
            flattened_model_resource_path("resources/model_catalog.json"),
            None
        );
        assert_eq!(flattened_model_resource_path("models/example.bin"), None);
    }
}
