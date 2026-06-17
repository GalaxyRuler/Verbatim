use crate::local_llm::catalog::{load_builtin_local_llm_models, LocalLlmModelInfo};
use crate::local_llm::runtime::{
    build_llama_server_args, build_managed_local_endpoint, resolve_llama_server_executable,
    select_runtime_port, ManagedLocalLlmEndpoint,
};
use crate::local_llm::LocalLlmSettings;
use anyhow::Result;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

pub const LOCAL_LLM_MODELS_SUBDIR: &str = "models/post-processing";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LocalLlmDownloadProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

pub struct LocalLlmManager {
    app_handle: AppHandle,
    models_dir: PathBuf,
    cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    runtime: Arc<Mutex<Option<ManagedLocalRuntime>>>,
}

struct ManagedLocalRuntime {
    model_id: String,
    port: u16,
    child: Child,
}

struct DownloadCleanup<'a> {
    cancel_flags: &'a Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    model_id: String,
    disarmed: bool,
}

impl Drop for DownloadCleanup<'_> {
    fn drop(&mut self) {
        if !self.disarmed {
            self.cancel_flags.lock().unwrap().remove(&self.model_id);
        }
    }
}

impl LocalLlmManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        let models_dir = models_dir_for_app(app_handle)?;
        fs::create_dir_all(&models_dir)?;

        Ok(Self {
            app_handle: app_handle.clone(),
            models_dir,
            cancel_flags: Arc::new(Mutex::new(HashMap::new())),
            runtime: Arc::new(Mutex::new(None)),
        })
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    pub fn list_models(&self) -> Result<Vec<LocalLlmModelInfo>> {
        let mut models = list_models_for_dir(&self.models_dir)?;
        let active_downloads = self.cancel_flags.lock().unwrap();

        for model in &mut models {
            if active_downloads.contains_key(&model.id) {
                model.is_downloading = true;
            }
        }

        Ok(models)
    }

    pub async fn download_model(&self, model_id: &str) -> Result<()> {
        fs::create_dir_all(&self.models_dir)?;
        let model = self.model_by_id(model_id)?;
        let url = model
            .url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No download URL for local LLM model {}", model_id))?;
        let model_path = model_path(&self.models_dir, &model);
        let partial_path = partial_path(&self.models_dir, &model);

        if model_path.exists() {
            let _ = fs::remove_file(&partial_path);
            let _ = self.app_handle.emit("local-llm-model-changed", model_id);
            return Ok(());
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut flags = self.cancel_flags.lock().unwrap();
            if flags.contains_key(model_id) {
                return Err(anyhow::anyhow!(
                    "Local LLM model {} is already downloading",
                    model_id
                ));
            }
            flags.insert(model_id.to_string(), cancel_flag.clone());
        }

        let mut cleanup = DownloadCleanup {
            cancel_flags: &self.cancel_flags,
            model_id: model_id.to_string(),
            disarmed: false,
        };

        let mut resume_from = partial_path
            .metadata()
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let client = reqwest::Client::new();
        let mut request = client.get(&url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }

        let mut response = request.send().await?;
        if resume_from > 0 && response.status() == reqwest::StatusCode::OK {
            drop(response);
            let _ = fs::remove_file(&partial_path);
            resume_from = 0;
            response = client.get(&url).send().await?;
        }

        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(anyhow::anyhow!(
                "Failed to download local LLM model {}: HTTP {}",
                model_id,
                response.status()
            ));
        }

        let total_size = if resume_from > 0 {
            resume_from + response.content_length().unwrap_or(0)
        } else {
            response.content_length().unwrap_or(0)
        };
        let mut downloaded = resume_from;
        let mut file = if resume_from > 0 {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial_path)?
        } else {
            File::create(&partial_path)?
        };

        self.emit_download_progress(model_id, downloaded, total_size);
        let mut last_emit = Instant::now();
        let throttle = Duration::from_millis(100);
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            if cancel_flag.load(Ordering::Relaxed) {
                drop(file);
                self.emit_download_progress(model_id, downloaded, total_size);
                return Ok(());
            }

            let chunk = chunk?;
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;

            if last_emit.elapsed() >= throttle {
                self.emit_download_progress(model_id, downloaded, total_size);
                last_emit = Instant::now();
            }
        }

        file.flush()?;
        drop(file);
        self.emit_download_progress(model_id, downloaded, total_size);

        if total_size > 0 {
            let actual_size = partial_path.metadata()?.len();
            if actual_size != total_size {
                let _ = fs::remove_file(&partial_path);
                return Err(anyhow::anyhow!(
                    "Local LLM download incomplete for {}: expected {} bytes, got {} bytes",
                    model_id,
                    total_size,
                    actual_size
                ));
            }
        }

        let verify_path = partial_path.clone();
        let expected_sha256 = model.sha256.clone();
        let verify_model_id = model_id.to_string();
        tokio::task::spawn_blocking(move || {
            verify_sha256(&verify_path, expected_sha256.as_deref(), &verify_model_id)
        })
        .await
        .map_err(|err| anyhow::anyhow!("Local LLM SHA-256 task failed: {}", err))??;

        fs::rename(&partial_path, &model_path)?;
        cleanup.disarmed = true;
        self.cancel_flags.lock().unwrap().remove(model_id);
        let _ = self.app_handle.emit("local-llm-model-changed", model_id);
        Ok(())
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        let flags = self.cancel_flags.lock().unwrap();
        let Some(flag) = flags.get(model_id) else {
            return Err(anyhow::anyhow!(
                "No active local LLM download for {}",
                model_id
            ));
        };

        flag.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<()> {
        if let Some(flag) = self.cancel_flags.lock().unwrap().get(model_id) {
            flag.store(true, Ordering::Relaxed);
        }

        self.stop_runtime_for_model(model_id);
        let model = self.model_by_id(model_id)?;
        delete_model_from_dir(&self.models_dir, &model)?;
        let _ = self.app_handle.emit("local-llm-model-changed", model_id);
        Ok(())
    }

    pub async fn ensure_runtime(
        &self,
        settings: &LocalLlmSettings,
    ) -> Result<ManagedLocalLlmEndpoint> {
        let (model, model_path) =
            selected_downloaded_model_for_runtime(&self.models_dir, settings)?.ok_or_else(
                || anyhow::anyhow!("Select and download a local post-processing model first"),
            )?;

        if let Some(endpoint) = self.reuse_running_runtime(settings, &model)? {
            return Ok(endpoint);
        }

        let port = select_runtime_port(settings.runtime_port)?;
        let executable = resolve_llama_server_executable().ok_or_else(|| {
            anyhow::anyhow!(
                "llama-server was not found. Install llama.cpp or set LLAMA_SERVER_PATH."
            )
        })?;

        self.emit_runtime_status("starting");
        let mut command = Command::new(executable);
        command
            .args(build_llama_server_args(&model_path, port))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        let child = command.spawn()?;
        {
            let mut runtime = self.runtime.lock().unwrap();
            *runtime = Some(ManagedLocalRuntime {
                model_id: model.id.clone(),
                port,
                child,
            });
        }

        let endpoint = build_managed_local_endpoint(port, &model.id, &model.filename);
        if let Err(err) = wait_for_runtime_health(&endpoint.provider.base_url).await {
            self.stop_runtime();
            self.emit_runtime_status("failed");
            return Err(err);
        }

        self.emit_runtime_status("ready");
        Ok(endpoint)
    }

    pub fn stop_runtime(&self) {
        if let Some(mut runtime) = self.runtime.lock().unwrap().take() {
            runtime.stop();
            self.emit_runtime_status("stopped");
        }
    }

    pub fn model_by_id(&self, model_id: &str) -> Result<LocalLlmModelInfo> {
        load_builtin_local_llm_models()?
            .remove(model_id)
            .ok_or_else(|| anyhow::anyhow!("Local LLM model not found: {}", model_id))
    }

    fn emit_download_progress(&self, model_id: &str, downloaded: u64, total: u64) {
        let progress = LocalLlmDownloadProgress {
            model_id: model_id.to_string(),
            downloaded,
            total,
            percentage: if total > 0 {
                (downloaded as f64 / total as f64) * 100.0
            } else {
                0.0
            },
        };
        let _ = self
            .app_handle
            .emit("local-llm-download-progress", progress);
    }

    fn reuse_running_runtime(
        &self,
        settings: &LocalLlmSettings,
        model: &LocalLlmModelInfo,
    ) -> Result<Option<ManagedLocalLlmEndpoint>> {
        let mut runtime = self.runtime.lock().unwrap();
        let Some(existing) = runtime.as_mut() else {
            return Ok(None);
        };

        let port_matches = settings.runtime_port == 0 || existing.port == settings.runtime_port;
        if existing.model_id == model.id && port_matches && existing.is_running()? {
            return Ok(Some(build_managed_local_endpoint(
                existing.port,
                &model.id,
                &model.filename,
            )));
        }

        if let Some(mut stale) = runtime.take() {
            stale.stop();
        }
        Ok(None)
    }

    fn stop_runtime_for_model(&self, model_id: &str) {
        let should_stop = self
            .runtime
            .lock()
            .unwrap()
            .as_ref()
            .map(|runtime| runtime.model_id == model_id)
            .unwrap_or(false);

        if should_stop {
            self.stop_runtime();
        }
    }

    fn emit_runtime_status(&self, status: &str) {
        let _ = self.app_handle.emit("local-llm-runtime-status", status);
    }
}

impl ManagedLocalRuntime {
    fn is_running(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_none())
    }

    fn stop(&mut self) {
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
            Err(_) => {
                let _ = self.child.kill();
            }
        }
    }
}

impl Drop for ManagedLocalRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn models_dir_for_app(app: &tauri::AppHandle) -> Result<PathBuf> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|err| anyhow::anyhow!("Failed to get app data dir: {}", err))?
        .join(LOCAL_LLM_MODELS_SUBDIR))
}

pub fn list_models_for_dir(models_dir: &Path) -> Result<Vec<LocalLlmModelInfo>> {
    let mut models = load_builtin_local_llm_models()?
        .into_values()
        .map(|mut model| {
            apply_download_status(models_dir, &mut model);
            model
        })
        .collect::<Vec<_>>();

    models.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(models)
}

pub fn model_path(models_dir: &Path, model: &LocalLlmModelInfo) -> PathBuf {
    models_dir.join(&model.filename)
}

pub fn partial_path(models_dir: &Path, model: &LocalLlmModelInfo) -> PathBuf {
    models_dir.join(format!("{}.partial", model.filename))
}

pub fn delete_model_from_dir(models_dir: &Path, model: &LocalLlmModelInfo) -> Result<()> {
    let model_path = model_path(models_dir, model);
    let partial_path = partial_path(models_dir, model);

    if model_path.exists() {
        fs::remove_file(model_path)?;
    }
    if partial_path.exists() {
        fs::remove_file(partial_path)?;
    }

    Ok(())
}

pub fn selected_downloaded_model_for_runtime(
    models_dir: &Path,
    settings: &LocalLlmSettings,
) -> Result<Option<(LocalLlmModelInfo, PathBuf)>> {
    if !settings.enabled || settings.selected_model_id.trim().is_empty() {
        return Ok(None);
    }

    let mut models = list_models_for_dir(models_dir)?;
    let Some(model) = models
        .iter_mut()
        .find(|model| model.id == settings.selected_model_id)
        .cloned()
    else {
        return Ok(None);
    };

    if !model.is_downloaded {
        return Ok(None);
    }

    let path = model_path(models_dir, &model);
    if !path.is_file() {
        return Ok(None);
    }

    Ok(Some((model, path)))
}

async fn wait_for_runtime_health(base_url: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()?;
    let deadline = Instant::now() + Duration::from_secs(90);
    let url = format!("{}/models", base_url.trim_end_matches('/'));

    loop {
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => return Ok(()),
            _ if Instant::now() >= deadline => {
                return Err(anyhow::anyhow!(
                    "Local LLM runtime did not become ready before timeout"
                ));
            }
            _ => sleep(Duration::from_millis(500)).await,
        }
    }
}

fn apply_download_status(models_dir: &Path, model: &mut LocalLlmModelInfo) {
    let model_path = model_path(models_dir, model);
    let partial_path = partial_path(models_dir, model);

    model.is_downloaded = model_path.is_file();
    model.is_downloading = false;
    model.partial_size = partial_path
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
}

pub fn verify_sha256(path: &Path, expected_sha256: Option<&str>, model_id: &str) -> Result<()> {
    let Some(expected) = expected_sha256 else {
        return Ok(());
    };

    match compute_sha256(path) {
        Ok(actual) if actual.eq_ignore_ascii_case(expected) => Ok(()),
        Ok(actual) => {
            let _ = fs::remove_file(path);
            Err(anyhow::anyhow!(
                "Download verification failed for local LLM model {}: expected {}, got {}",
                model_id,
                expected,
                actual
            ))
        }
        Err(err) => {
            let _ = fs::remove_file(path);
            Err(anyhow::anyhow!(
                "Failed to verify local LLM model {}: {}",
                model_id,
                err
            ))
        }
    }
}

pub fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
