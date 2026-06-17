use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::shortcut;
use crate::TranscriptionCoordinator;
use log::{info, warn};
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

// Re-export all utility modules for easy access
// pub use crate::audio_feedback::*;
pub use crate::clipboard::*;
pub use crate::overlay::*;
pub use crate::tray::*;

const SILERO_VAD_RESOURCE_PATH: &str = "resources/models/silero_vad_v4.onnx";
const SILERO_VAD_ASSET_URL: &str = "https://verbatim-assets.galaxyruler.space/silero_vad_v4.onnx";
const SILERO_VAD_SHA256: &str = "a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28";

struct RequiredResourceAsset {
    relative_path: &'static str,
    url: &'static str,
    sha256: &'static str,
}

const SILERO_VAD_ASSET: RequiredResourceAsset = RequiredResourceAsset {
    relative_path: SILERO_VAD_RESOURCE_PATH,
    url: SILERO_VAD_ASSET_URL,
    sha256: SILERO_VAD_SHA256,
};

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
    crate::overlay::emit_overlay_state_changed(app, "cancelled");
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
    let candidates = resource_resolver_paths(app, relative_path)?;
    let fallback_path = candidates
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("No resource path candidate for {relative_path}"))?;

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Ok(fallback_path)
}

pub fn resolve_silero_vad_model_path(app: &AppHandle) -> anyhow::Result<PathBuf> {
    resolve_required_resource_asset(app, &SILERO_VAD_ASSET).map_err(|e| {
        anyhow::anyhow!(
            "Silero VAD model is missing or invalid. Verbatim checked bundled resources and tried {}. {}",
            SILERO_VAD_ASSET.url,
            e
        )
    })
}

fn resolve_required_resource_asset(
    app: &AppHandle,
    asset: &RequiredResourceAsset,
) -> anyhow::Result<PathBuf> {
    let target_path = crate::portable::resolve_app_data(app, asset.relative_path)
        .map_err(|e| anyhow::anyhow!("Failed to resolve app-data resource path: {e}"))?;

    if file_matches_sha256(&target_path, asset.sha256) {
        return Ok(target_path);
    }

    if target_path.exists() {
        warn!(
            "Ignoring invalid bundled resource copy at {}",
            target_path.display()
        );
    }

    for source_path in resource_resolver_paths(app, asset.relative_path)? {
        if source_path == target_path || !file_matches_sha256(&source_path, asset.sha256) {
            continue;
        }

        info!(
            "Provisioning required resource from {} to {}",
            source_path.display(),
            target_path.display()
        );
        return copy_verified_resource_to_target(&source_path, &target_path, asset.sha256);
    }

    download_verified_resource_to_target(asset.url, &target_path, asset.sha256)
}

fn resource_resolver_paths(app: &AppHandle, relative_path: &str) -> anyhow::Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    let resolved = app
        .path()
        .resolve(relative_path, tauri::path::BaseDirectory::Resource)
        .map_err(|e| anyhow::anyhow!("Failed to resolve resource path {relative_path}: {e}"))?;
    push_unique_path(&mut candidates, resolved);

    if let Some(flattened_path) = flattened_model_resource_path(relative_path) {
        let flattened = app
            .path()
            .resolve(&flattened_path, tauri::path::BaseDirectory::Resource)
            .map_err(|e| {
                anyhow::anyhow!("Failed to resolve flattened resource path {flattened_path}: {e}")
            })?;
        push_unique_path(&mut candidates, flattened);
    }

    let exe_path = std::env::current_exe().ok();
    let current_dir = std::env::current_dir().ok();
    for candidate in adjacent_resource_candidate_paths(
        relative_path,
        exe_path.as_deref(),
        current_dir.as_deref(),
    ) {
        push_unique_path(&mut candidates, candidate);
    }

    Ok(candidates)
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

fn flattened_model_resource_path(relative_path: &str) -> Option<String> {
    relative_path
        .strip_prefix("resources/models/")
        .map(|path| format!("resources/{path}"))
}

fn adjacent_resource_candidate_paths(
    relative_path: &str,
    exe_path: Option<&Path>,
    current_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(exe_dir) = exe_path.and_then(Path::parent) {
        roots.push(exe_dir.to_path_buf());
    }
    if let Some(current_dir) = current_dir {
        roots.push(current_dir.to_path_buf());
    }

    let mut relative_paths = vec![PathBuf::from(relative_path)];
    if let Some(flattened) = flattened_model_resource_path(relative_path) {
        relative_paths.push(PathBuf::from(flattened));
    }

    let mut candidates = Vec::new();
    for root in roots {
        for relative in &relative_paths {
            let candidate = root.join(relative);
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }

    candidates
}

fn copy_verified_resource_to_target(
    source_path: &Path,
    target_path: &Path,
    expected_sha256: &str,
) -> anyhow::Result<PathBuf> {
    if !file_matches_sha256(source_path, expected_sha256) {
        return Err(anyhow::anyhow!(
            "Resource hash mismatch for {}",
            source_path.display()
        ));
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let partial_path = partial_path_for(target_path);
    if partial_path.exists() {
        fs::remove_file(&partial_path)?;
    }

    fs::copy(source_path, &partial_path)?;
    if !file_matches_sha256(&partial_path, expected_sha256) {
        let _ = fs::remove_file(&partial_path);
        return Err(anyhow::anyhow!(
            "Copied resource hash mismatch for {}",
            target_path.display()
        ));
    }

    if target_path.exists() {
        fs::remove_file(target_path)?;
    }
    fs::rename(&partial_path, target_path)?;

    Ok(target_path.to_path_buf())
}

fn download_verified_resource_to_target(
    url: &str,
    target_path: &Path,
    expected_sha256: &str,
) -> anyhow::Result<PathBuf> {
    info!(
        "Downloading required resource from {} to {}",
        url,
        target_path.display()
    );

    let bytes = tauri::async_runtime::block_on(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        let response = client.get(url).send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Failed to download required resource: HTTP {}",
                status
            ));
        }

        let bytes = response.bytes().await?;
        Ok::<Vec<u8>, anyhow::Error>(bytes.to_vec())
    })?;

    write_verified_resource_bytes_to_target(&bytes, target_path, expected_sha256)
}

fn write_verified_resource_bytes_to_target(
    bytes: &[u8],
    target_path: &Path,
    expected_sha256: &str,
) -> anyhow::Result<PathBuf> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let partial_path = partial_path_for(target_path);
    if partial_path.exists() {
        fs::remove_file(&partial_path)?;
    }

    fs::write(&partial_path, bytes)?;
    if !file_matches_sha256(&partial_path, expected_sha256) {
        let _ = fs::remove_file(&partial_path);
        return Err(anyhow::anyhow!(
            "Downloaded resource hash mismatch for {}",
            target_path.display()
        ));
    }

    if target_path.exists() {
        fs::remove_file(target_path)?;
    }
    fs::rename(&partial_path, target_path)?;

    Ok(target_path.to_path_buf())
}

fn partial_path_for(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name() else {
        return path.with_extension("partial");
    };

    let mut partial_name = file_name.to_os_string();
    partial_name.push(".partial");
    path.with_file_name(partial_name)
}

fn file_matches_sha256(path: &Path, expected_sha256: &str) -> bool {
    if !path.is_file() {
        return false;
    }

    compute_sha256_hex(path)
        .map(|actual| actual.eq_ignore_ascii_case(expected_sha256))
        .unwrap_or(false)
}

fn compute_sha256_hex(path: &Path) -> anyhow::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
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
    use sha2::{Digest, Sha256};
    use std::fs;
    use tempfile::TempDir;

    fn sha256_hex(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

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

    #[cfg(windows)]
    #[test]
    fn adjacent_resource_candidates_include_installed_windows_exe_resource_layouts() {
        let exe_path =
            PathBuf::from(r"C:\Users\Example\AppData\Local\Programs\Verbatim\Verbatim.exe");
        let candidates = adjacent_resource_candidate_paths(
            "resources/models/silero_vad_v4.onnx",
            Some(&exe_path),
            None,
        );

        assert!(candidates.contains(&PathBuf::from(
            r"C:\Users\Example\AppData\Local\Programs\Verbatim\resources\models\silero_vad_v4.onnx",
        )));
        assert!(candidates.contains(&PathBuf::from(
            r"C:\Users\Example\AppData\Local\Programs\Verbatim\resources\silero_vad_v4.onnx",
        )));
    }

    #[cfg(not(windows))]
    #[test]
    fn adjacent_resource_candidates_include_installed_unix_exe_resource_layouts() {
        let exe_path = PathBuf::from("/opt/verbatim/verbatim");
        let candidates = adjacent_resource_candidate_paths(
            "resources/models/silero_vad_v4.onnx",
            Some(&exe_path),
            None,
        );

        assert!(candidates.contains(&PathBuf::from(
            "/opt/verbatim/resources/models/silero_vad_v4.onnx",
        )));
        assert!(candidates.contains(&PathBuf::from("/opt/verbatim/resources/silero_vad_v4.onnx",)));
    }

    #[test]
    fn copy_verified_resource_to_target_creates_app_data_resource_copy() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("bundle/resources/silero_vad_v4.onnx");
        let target_path = temp_dir
            .path()
            .join("app-data/resources/models/silero_vad_v4.onnx");
        let model_bytes = b"valid vad model bytes";
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        fs::write(&source_path, model_bytes).unwrap();

        let copied_path =
            copy_verified_resource_to_target(&source_path, &target_path, &sha256_hex(model_bytes))
                .unwrap();

        assert_eq!(copied_path, target_path);
        assert_eq!(fs::read(&target_path).unwrap(), model_bytes);
    }

    #[test]
    fn write_verified_resource_bytes_rejects_hash_mismatch_without_target_file() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir
            .path()
            .join("app-data/resources/models/silero_vad_v4.onnx");

        let result = write_verified_resource_bytes_to_target(
            b"corrupt bytes",
            &target_path,
            &sha256_hex(b"valid bytes"),
        );

        assert!(result.is_err());
        assert!(!target_path.exists());
        assert!(!partial_path_for(&target_path).exists());
    }

    #[test]
    fn write_verified_resource_bytes_creates_target_file() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir
            .path()
            .join("app-data/resources/models/silero_vad_v4.onnx");
        let model_bytes = b"downloaded vad model bytes";

        let written_path = write_verified_resource_bytes_to_target(
            model_bytes,
            &target_path,
            &sha256_hex(model_bytes),
        )
        .unwrap();

        assert_eq!(written_path, target_path);
        assert_eq!(fs::read(&target_path).unwrap(), model_bytes);
        assert!(!partial_path_for(&target_path).exists());
    }
}
