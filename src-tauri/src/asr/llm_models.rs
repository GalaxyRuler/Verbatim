//! Android on-device LLM cleanup model-pack metadata and filesystem helpers.

use anyhow::Result;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const MODELS_SUBDIR: &str = "models/android-llm-postproc";
const ACTIVE_MODEL_FILE: &str = "active-model.txt";
const DOWNLOADS_DIR: &str = ".downloads";
const INSTALLING_DIR: &str = ".installing";
const REPLACING_DIR: &str = ".replacing";
const DEFAULT_PACK_ID: &str = "g4-qwen2_5-0_5b-litert-q8";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AndroidLlmModelFile {
    pub target_path: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AndroidLlmModelPack {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub runtime: String,
    pub license: String,
    pub quantization: String,
    pub size_mb: u64,
    pub min_ram_mb: u64,
    pub files: Vec<AndroidLlmModelFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AndroidLlmModelPackState {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub runtime: String,
    pub license: String,
    pub quantization: String,
    pub size_mb: u64,
    pub min_ram_mb: u64,
    pub installed_dir: String,
    pub model_path: String,
    pub is_installed: bool,
    pub is_downloading: bool,
    pub is_active: bool,
    pub is_selectable: bool,
    pub download_phase: String,
    pub download_progress: f64,
    pub missing_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AndroidLlmDownloadProgress {
    pub model_id: String,
    pub phase: String,
    pub file: Option<String>,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

#[derive(Default)]
pub struct AndroidLlmModelManager {
    cancel_flags: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

struct DownloadCleanup<'a> {
    cancel_flags: &'a Mutex<HashMap<String, Arc<AtomicBool>>>,
    model_id: String,
    disarmed: bool,
}

impl Drop for DownloadCleanup<'_> {
    fn drop(&mut self) {
        if !self.disarmed {
            if let Ok(mut flags) = self.cancel_flags.lock() {
                flags.remove(&self.model_id);
            }
        }
    }
}

pub fn default_model_pack_id() -> &'static str {
    DEFAULT_PACK_ID
}

pub fn builtin_model_packs() -> Vec<AndroidLlmModelPack> {
    vec![AndroidLlmModelPack {
        id: DEFAULT_PACK_ID.to_string(),
        display_name: "Qwen2.5 cleanup 0.5B".to_string(),
        description: "LiteRT-LM Qwen2.5 0.5B Instruct q8 for punctuation and grammar cleanup."
            .to_string(),
        runtime: "LiteRT-LM 0.13.1".to_string(),
        license: "Apache-2.0".to_string(),
        quantization: "q8 ekv1280".to_string(),
        size_mb: 522,
        min_ram_mb: 8192,
        files: vec![AndroidLlmModelFile {
            target_path: "qwen2.5-0.5b-instruct-q8.task".to_string(),
            url: "https://huggingface.co/litert-community/Qwen2.5-0.5B-Instruct/resolve/6c237a59eedeb06a821b21f0a59b03d346ac8bc3/Qwen2.5-0.5B-Instruct_multi-prefill-seq_q8_ekv1280.task".to_string(),
            sha256: "e608953f169aeb1bd7b9155fec2559825e08453fc209b84eda3a781ed0452fd2".to_string(),
            size_bytes: 546_660_344,
        }],
    }]
}

impl AndroidLlmModelManager {
    pub fn list_for_app(&self, app: &AppHandle) -> Result<Vec<AndroidLlmModelPackState>> {
        let root = models_root_for_app(app)?;
        self.list_for_dir(&root)
    }

    pub fn list_for_dir(&self, root: &Path) -> Result<Vec<AndroidLlmModelPackState>> {
        let active = active_model_id_for_dir(root)?;
        builtin_model_packs()
            .iter()
            .map(|pack| self.pack_state_for_dir(root, active.as_deref(), &pack.id))
            .collect()
    }

    pub fn pack_state_for_app(
        &self,
        app: &AppHandle,
        model_id: &str,
    ) -> Result<AndroidLlmModelPackState> {
        let root = models_root_for_app(app)?;
        let active = active_model_id_for_dir(&root)?;
        self.pack_state_for_dir(&root, active.as_deref(), model_id)
    }

    pub fn pack_state_for_dir(
        &self,
        root: &Path,
        active_model_id: Option<&str>,
        model_id: &str,
    ) -> Result<AndroidLlmModelPackState> {
        let pack = pack_by_id(model_id)?;
        let pack_dir = installed_pack_dir_for_root(root, &pack.id);
        let missing_files = missing_files(&pack, &pack_dir)?;
        let is_downloading = self.is_downloading(&pack.id);
        let is_installed = missing_files.is_empty();
        let model_path = primary_model_path(&pack, &pack_dir)?;

        Ok(AndroidLlmModelPackState {
            id: pack.id,
            display_name: pack.display_name,
            description: pack.description,
            runtime: pack.runtime,
            license: pack.license,
            quantization: pack.quantization,
            size_mb: pack.size_mb,
            min_ram_mb: pack.min_ram_mb,
            installed_dir: pack_dir.to_string_lossy().into_owned(),
            model_path: model_path.to_string_lossy().into_owned(),
            is_installed,
            is_downloading,
            is_active: active_model_id == Some(model_id) && is_installed,
            is_selectable: is_installed && !is_downloading,
            download_phase: if is_downloading {
                "downloading".to_string()
            } else if is_installed {
                "ready".to_string()
            } else {
                "available".to_string()
            },
            download_progress: if is_installed { 100.0 } else { 0.0 },
            missing_files,
        })
    }

    pub async fn download_pack(&self, app: &AppHandle, model_id: &str) -> Result<()> {
        let pack = pack_by_id(model_id)?;
        let root = models_root_for_app(app)?;
        fs::create_dir_all(&root)?;

        if self
            .pack_state_for_dir(&root, active_model_id_for_dir(&root)?.as_deref(), &pack.id)?
            .is_installed
        {
            emit_model_changed(app, &pack.id);
            return Ok(());
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut flags = self
                .cancel_flags
                .lock()
                .map_err(|_| anyhow::anyhow!("Android LLM download lock is poisoned"))?;
            if flags.contains_key(&pack.id) {
                return Err(anyhow::anyhow!(
                    "Android LLM model pack {} is already downloading",
                    pack.id
                ));
            }
            flags.insert(pack.id.clone(), cancel_flag.clone());
        }

        let mut cleanup = DownloadCleanup {
            cancel_flags: &self.cancel_flags,
            model_id: pack.id.clone(),
            disarmed: false,
        };

        let staging_dir = staging_dir(&root, &pack.id);
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir)?;
        }
        fs::create_dir_all(&staging_dir)?;

        let download_dir = download_dir(&root, &pack.id);
        fs::create_dir_all(&download_dir)?;

        for file in &pack.files {
            download_component(
                app,
                &pack.id,
                file,
                &download_dir,
                &staging_dir,
                &cancel_flag,
            )
            .await?;
        }

        emit_progress(app, &pack.id, "installing", None, 0, 0);
        ensure_pack_layout(&pack, &staging_dir)?;
        replace_pack_dir(&root, &pack.id, &staging_dir)?;
        let _ = fs::remove_dir_all(download_dir);

        cleanup.disarmed = true;
        self.cancel_flags
            .lock()
            .map_err(|_| anyhow::anyhow!("Android LLM download lock is poisoned"))?
            .remove(&pack.id);
        emit_model_changed(app, &pack.id);

        Ok(())
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        let flags = self
            .cancel_flags
            .lock()
            .map_err(|_| anyhow::anyhow!("Android LLM download lock is poisoned"))?;
        let Some(flag) = flags.get(model_id) else {
            return Err(anyhow::anyhow!(
                "No active Android LLM model download for {}",
                model_id
            ));
        };

        flag.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn select_pack(&self, app: &AppHandle, model_id: &str) -> Result<AndroidLlmModelPackState> {
        let root = models_root_for_app(app)?;
        let state = self.pack_state_for_dir(&root, Some(model_id), model_id)?;
        if !state.is_selectable {
            return Err(anyhow::anyhow!(
                "Android LLM model pack {} is not installed",
                model_id
            ));
        }

        write_active_model_id(&root, model_id)?;
        Ok(AndroidLlmModelPackState {
            is_active: true,
            ..state
        })
    }

    pub fn delete_pack(&self, app: &AppHandle, model_id: &str) -> Result<()> {
        let pack = pack_by_id(model_id)?;
        if let Ok(flags) = self.cancel_flags.lock() {
            if let Some(flag) = flags.get(&pack.id) {
                flag.store(true, Ordering::Relaxed);
            }
        }

        let root = models_root_for_app(app)?;
        let final_dir = pack_dir(&root, &pack.id);
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir)?;
        }
        let download_dir = download_dir(&root, &pack.id);
        if download_dir.exists() {
            fs::remove_dir_all(download_dir)?;
        }
        let staging_dir = staging_dir(&root, &pack.id);
        if staging_dir.exists() {
            fs::remove_dir_all(staging_dir)?;
        }

        if active_model_id_for_dir(&root)?.as_deref() == Some(&pack.id) {
            clear_active_model_id(&root)?;
        }

        emit_model_changed(app, &pack.id);
        Ok(())
    }

    fn is_downloading(&self, model_id: &str) -> bool {
        self.cancel_flags
            .lock()
            .map(|flags| flags.contains_key(model_id))
            .unwrap_or(false)
    }
}

async fn download_component(
    app: &AppHandle,
    model_id: &str,
    file: &AndroidLlmModelFile,
    download_dir: &Path,
    staging_dir: &Path,
    cancel_flag: &Arc<AtomicBool>,
) -> Result<()> {
    let target_rel = safe_relative_path(&file.target_path)?;
    let partial_path = download_dir.join(&target_rel).with_file_name(format!(
        "{}.partial",
        target_rel
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid Android LLM component path"))?
    ));
    let staging_path = staging_dir.join(&target_rel);
    if let Some(parent) = partial_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = staging_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut resume_from = partial_path
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let client = reqwest::Client::new();
    let mut request = client.get(&file.url);
    if resume_from > 0 {
        request = request.header("Range", format!("bytes={resume_from}-"));
    }

    let mut response = request.send().await?;
    if resume_from > 0 && response.status() == reqwest::StatusCode::OK {
        drop(response);
        let _ = fs::remove_file(&partial_path);
        resume_from = 0;
        response = client.get(&file.url).send().await?;
    }

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
    {
        return Err(anyhow::anyhow!(
            "Failed to download Android LLM component {}: HTTP {}",
            file.target_path,
            response.status()
        ));
    }

    let total_size = if resume_from > 0 {
        resume_from + response.content_length().unwrap_or(0)
    } else {
        response.content_length().unwrap_or(file.size_bytes)
    };
    let mut downloaded = resume_from;
    let mut output = if resume_from > 0 {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&partial_path)?
    } else {
        File::create(&partial_path)?
    };
    emit_progress(
        app,
        model_id,
        "downloading",
        Some(file.target_path.clone()),
        downloaded,
        total_size,
    );

    let mut last_emit = Instant::now();
    let throttle = Duration::from_millis(100);
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        if cancel_flag.load(Ordering::Relaxed) {
            drop(output);
            emit_progress(
                app,
                model_id,
                "cancelled",
                Some(file.target_path.clone()),
                downloaded,
                total_size,
            );
            return Err(anyhow::anyhow!(
                "Android LLM model download cancelled for {}",
                model_id
            ));
        }

        let chunk = chunk?;
        output.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= throttle {
            emit_progress(
                app,
                model_id,
                "downloading",
                Some(file.target_path.clone()),
                downloaded,
                total_size,
            );
            last_emit = Instant::now();
        }
    }
    output.flush()?;
    drop(output);

    if total_size > 0 {
        let actual_size = partial_path.metadata()?.len();
        if actual_size != total_size {
            let _ = fs::remove_file(&partial_path);
            return Err(anyhow::anyhow!(
                "Android LLM component {} incomplete: expected {} bytes, got {} bytes",
                file.target_path,
                total_size,
                actual_size
            ));
        }
    }

    emit_progress(
        app,
        model_id,
        "verifying",
        Some(file.target_path.clone()),
        downloaded,
        total_size,
    );
    let verify_path = partial_path.clone();
    let expected = file.sha256.clone();
    let component = file.target_path.clone();
    tokio::task::spawn_blocking(move || verify_sha256(&verify_path, &expected, &component))
        .await
        .map_err(|err| anyhow::anyhow!("Android LLM SHA-256 task failed: {}", err))??;

    fs::rename(&partial_path, &staging_path)?;
    Ok(())
}

fn emit_progress(
    app: &AppHandle,
    model_id: &str,
    phase: &str,
    file: Option<String>,
    downloaded: u64,
    total: u64,
) {
    let percentage = if total > 0 {
        (downloaded as f64 / total as f64) * 100.0
    } else if phase == "verifying" || phase == "installing" {
        100.0
    } else {
        0.0
    };

    let _ = app.emit(
        "android-llm-model-progress",
        AndroidLlmDownloadProgress {
            model_id: model_id.to_string(),
            phase: phase.to_string(),
            file,
            downloaded,
            total,
            percentage,
        },
    );
}

fn emit_model_changed(app: &AppHandle, model_id: &str) {
    let _ = app.emit("android-llm-model-changed", model_id);
}

fn pack_by_id(model_id: &str) -> Result<AndroidLlmModelPack> {
    builtin_model_packs()
        .into_iter()
        .find(|pack| pack.id == model_id)
        .ok_or_else(|| anyhow::anyhow!("Android LLM model pack not found: {}", model_id))
}

fn models_root_for_app(app: &AppHandle) -> Result<PathBuf> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|err| anyhow::anyhow!("Failed to get app data dir: {}", err))?
        .join(MODELS_SUBDIR))
}

fn installed_pack_dir_for_root(root: &Path, model_id: &str) -> PathBuf {
    pack_dir(root, model_id)
}

fn pack_dir(root: &Path, model_id: &str) -> PathBuf {
    root.join(model_id)
}

fn download_dir(root: &Path, model_id: &str) -> PathBuf {
    root.join(DOWNLOADS_DIR).join(model_id)
}

fn staging_dir(root: &Path, model_id: &str) -> PathBuf {
    root.join(INSTALLING_DIR).join(model_id)
}

fn replacing_dir(root: &Path, model_id: &str) -> PathBuf {
    root.join(REPLACING_DIR).join(model_id)
}

fn active_model_path(root: &Path) -> PathBuf {
    root.join(ACTIVE_MODEL_FILE)
}

fn active_model_id_for_dir(root: &Path) -> Result<Option<String>> {
    let path = active_model_path(root);
    if !path.exists() {
        return Ok(None);
    }

    let value = fs::read_to_string(path)?.trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

fn write_active_model_id(root: &Path, model_id: &str) -> Result<()> {
    fs::create_dir_all(root)?;
    fs::write(active_model_path(root), model_id)?;
    Ok(())
}

fn clear_active_model_id(root: &Path) -> Result<()> {
    let path = active_model_path(root);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn primary_model_path(pack: &AndroidLlmModelPack, pack_dir: &Path) -> Result<PathBuf> {
    let file = pack
        .files
        .first()
        .ok_or_else(|| anyhow::anyhow!("Android LLM model pack {} has no files", pack.id))?;
    Ok(pack_dir.join(safe_relative_path(&file.target_path)?))
}

fn missing_files(pack: &AndroidLlmModelPack, pack_dir: &Path) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for file in &pack.files {
        let target_rel = safe_relative_path(&file.target_path)?;
        if !pack_dir.join(target_rel).is_file() {
            missing.push(file.target_path.clone());
        }
    }
    Ok(missing)
}

fn ensure_pack_layout(pack: &AndroidLlmModelPack, pack_dir: &Path) -> Result<()> {
    let missing = missing_files(pack, pack_dir)?;
    if missing.is_empty() {
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Android LLM model pack {} is incomplete; missing {}",
        pack.id,
        missing.join(", ")
    ))
}

fn replace_pack_dir(root: &Path, model_id: &str, staging_dir: &Path) -> Result<()> {
    let final_dir = pack_dir(root, model_id);
    let backup_dir = replacing_dir(root, model_id);
    if backup_dir.exists() {
        fs::remove_dir_all(&backup_dir)?;
    }

    if final_dir.exists() {
        fs::rename(&final_dir, &backup_dir)?;
    }

    match fs::rename(staging_dir, &final_dir) {
        Ok(()) => {
            if backup_dir.exists() {
                let _ = fs::remove_dir_all(backup_dir);
            }
            Ok(())
        }
        Err(err) => {
            if backup_dir.exists() && !final_dir.exists() {
                let _ = fs::rename(&backup_dir, &final_dir);
            }
            Err(err.into())
        }
    }
}

fn safe_relative_path(value: &str) -> Result<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(anyhow::anyhow!(
            "Absolute model component path is not allowed"
        ));
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(anyhow::anyhow!(
            "Model component path must stay inside the pack directory"
        ));
    }

    Ok(path.to_path_buf())
}

pub fn verify_sha256(path: &Path, expected_sha256: &str, component: &str) -> Result<()> {
    let actual = compute_sha256(path).map_err(|err| {
        let _ = fs::remove_file(path);
        anyhow::anyhow!(
            "Failed to verify Android LLM component {}: {}",
            component,
            err
        )
    })?;

    if actual.eq_ignore_ascii_case(expected_sha256) {
        return Ok(());
    }

    let _ = fs::remove_file(path);
    Err(anyhow::anyhow!(
        "Android LLM component verification failed for {}: expected {}, got {}",
        component,
        expected_sha256,
        actual
    ))
}

pub fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65_536];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn manifest_contains_sha_pinned_litert_pack() {
        let packs = builtin_model_packs();

        assert_eq!(packs.len(), 1);
        let pack = &packs[0];
        assert_eq!(pack.id, default_model_pack_id());
        assert_eq!(pack.runtime, "LiteRT-LM 0.13.1");
        assert_eq!(pack.license, "Apache-2.0");
        assert_eq!(pack.min_ram_mb, 8192);
        assert_eq!(pack.files.len(), 1);
        assert_eq!(pack.files[0].target_path, "qwen2.5-0.5b-instruct-q8.task");
        assert_eq!(pack.files[0].sha256.len(), 64);
        assert!(pack.files[0].url.contains("litert-community/Qwen2.5"));
    }

    #[test]
    fn installed_pack_state_rejects_missing_litert_task() {
        let temp = tempfile::tempdir().unwrap();
        let manager = AndroidLlmModelManager::default();

        let state = manager
            .pack_state_for_dir(
                temp.path(),
                Some(default_model_pack_id()),
                default_model_pack_id(),
            )
            .unwrap();

        assert!(!state.is_installed);
        assert!(!state.is_selectable);
        assert_eq!(
            state.missing_files,
            vec!["qwen2.5-0.5b-instruct-q8.task".to_string()]
        );
    }

    #[test]
    fn installed_pack_state_accepts_complete_litert_layout() {
        let temp = tempfile::tempdir().unwrap();
        let pack_dir = temp.path().join(default_model_pack_id());
        fs::create_dir_all(&pack_dir).unwrap();
        let task_path = pack_dir.join("qwen2.5-0.5b-instruct-q8.task");
        fs::write(&task_path, b"fixture").unwrap();

        let manager = AndroidLlmModelManager::default();
        let state = manager
            .pack_state_for_dir(
                temp.path(),
                Some(default_model_pack_id()),
                default_model_pack_id(),
            )
            .unwrap();

        assert!(state.is_installed);
        assert!(state.is_selectable);
        assert!(state.is_active);
        assert_eq!(state.installed_dir, pack_dir.to_string_lossy().into_owned());
        assert_eq!(state.model_path, task_path.to_string_lossy().into_owned());
        assert!(state.missing_files.is_empty());
    }

    #[test]
    fn model_component_paths_must_stay_inside_pack_dir() {
        assert_eq!(
            safe_relative_path("qwen2.5-0.5b-instruct-q8.task").unwrap(),
            PathBuf::from("qwen2.5-0.5b-instruct-q8.task")
        );

        assert!(safe_relative_path("../model.task").is_err());
        assert!(safe_relative_path("/tmp/model.task").is_err());
        assert!(safe_relative_path("pack/../model.task").is_err());
    }

    #[test]
    fn sha256_mismatch_rejects_downloaded_component() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("model.task");
        fs::write(&file, b"not the expected model").unwrap();

        let error = verify_sha256(&file, "0".repeat(64).as_str(), "model.task")
            .expect_err("mismatch should reject the file");

        assert!(error.to_string().contains("verification failed"));
        assert!(!file.exists());
    }
}
