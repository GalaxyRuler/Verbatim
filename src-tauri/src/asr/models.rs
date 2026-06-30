//! Model pack metadata and filesystem helpers for Android ASR.

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
use tauri::{AppHandle, Emitter, Runtime};

const MODELS_SUBDIR: &str = "models/android-asr";
const ACTIVE_MODEL_FILE: &str = "active-model.txt";
const DOWNLOADS_DIR: &str = ".downloads";
const INSTALLING_DIR: &str = ".installing";
const REPLACING_DIR: &str = ".replacing";
const DEFAULT_PACK_ID: &str = "g3-zipformer-whisper-tiny-en";
const CANARY_PACK_ID: &str = "canary-180m-flash-en-es-de-fr";
const CANARY_MIN_RAM_MB: u64 = 6144;
const CANARY_REVISION: &str = "9077164e0d3dd1d5353743e89ceaa1d3a770838c";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AndroidAsrModelFile {
    pub target_path: String,
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AndroidAsrEngineKind {
    ZipformerWhisper,
    SenseVoice,
    Canary,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AndroidAsrModelPack {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub language: String,
    pub size_mb: u64,
    pub min_ram_mb: u64,
    pub engine_kind: AndroidAsrEngineKind,
    pub files: Vec<AndroidAsrModelFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AndroidAsrModelPackState {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub language: String,
    pub size_mb: u64,
    pub min_ram_mb: u64,
    pub engine_kind: AndroidAsrEngineKind,
    pub installed_dir: String,
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
pub struct AndroidAsrDownloadProgress {
    pub model_id: String,
    pub phase: String,
    pub file: Option<String>,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

#[derive(Default)]
pub struct AndroidAsrModelManager {
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

pub fn builtin_model_packs() -> Vec<AndroidAsrModelPack> {
    // Use per-file upstream URLs instead of the .tar.bz2 release archives so Android can stream,
    // resume, hash-check, and stage each component without adding a bzip2 extraction dependency.
    vec![
        AndroidAsrModelPack {
            id: DEFAULT_PACK_ID.to_string(),
            display_name: "English On-device Starter".to_string(),
            description: "Streaming Zipformer 20M, Whisper tiny.en int8, and Silero VAD."
                .to_string(),
            language: "en".to_string(),
            size_mb: 141,
            min_ram_mb: 0,
            engine_kind: AndroidAsrEngineKind::ZipformerWhisper,
            files: model_files_with_whisper(whisper_tiny_files()),
        },
        AndroidAsrModelPack {
            id: "g3-zipformer-whisper-base-en".to_string(),
            display_name: "English - higher accuracy".to_string(),
            description: "Streaming Zipformer 20M, Whisper base.en int8, and Silero VAD."
                .to_string(),
            language: "en".to_string(),
            size_mb: 167,
            min_ram_mb: 0,
            engine_kind: AndroidAsrEngineKind::ZipformerWhisper,
            files: model_files_with_whisper(whisper_base_files()),
        },
        AndroidAsrModelPack {
            id: "sensevoice-multilingual-zh-en-ja-ko-yue".to_string(),
            display_name: "SenseVoice multilingual".to_string(),
            description: "Offline SenseVoice for Chinese, English, Japanese, Korean, and Cantonese. Final text only; no live partials.".to_string(),
            language: "auto".to_string(),
            size_mb: 229,
            min_ram_mb: 0,
            engine_kind: AndroidAsrEngineKind::SenseVoice,
            files: model_files_with_sense_voice(),
        },
        AndroidAsrModelPack {
            id: CANARY_PACK_ID.to_string(),
            display_name: "Canary 180M Flash".to_string(),
            description: "Offline Canary for English, Spanish, German, and French. Final text only; no live partials.".to_string(),
            language: "en-es-de-fr".to_string(),
            size_mb: 207,
            min_ram_mb: CANARY_MIN_RAM_MB,
            engine_kind: AndroidAsrEngineKind::Canary,
            files: model_files_with_canary(),
        },
    ]
}

fn model_files_with_whisper(whisper_files: Vec<AndroidAsrModelFile>) -> Vec<AndroidAsrModelFile> {
    let mut files = streaming_zipformer_files();
    files.extend(whisper_files);
    files.push(silero_vad_file());
    files
}

fn model_files_with_sense_voice() -> Vec<AndroidAsrModelFile> {
    vec![
        AndroidAsrModelFile {
            target_path: "sense_voice/model.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/model.int8.onnx".to_string(),
            sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51".to_string(),
            size_bytes: 239_233_841,
        },
        AndroidAsrModelFile {
            target_path: "sense_voice/tokens.txt".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17/resolve/2365baeacb507f821a0c8120fcee3d484dba7a07/tokens.txt".to_string(),
            sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc".to_string(),
            size_bytes: 315_894,
        },
        silero_vad_file(),
    ]
}

fn model_files_with_canary() -> Vec<AndroidAsrModelFile> {
    vec![
        AndroidAsrModelFile {
            target_path: "canary/encoder.onnx".to_string(),
            url: format!(
                "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8/resolve/{CANARY_REVISION}/encoder.int8.onnx"
            ),
            sha256: "7a75b4e2a5857a6dcc0819503bbe3fad66943db4a3ccf21d3f27c633667d303f"
                .to_string(),
            size_bytes: 132_678_643,
        },
        AndroidAsrModelFile {
            target_path: "canary/decoder.onnx".to_string(),
            url: format!(
                "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8/resolve/{CANARY_REVISION}/decoder.int8.onnx"
            ),
            sha256: "e41a2ab9c0c2fe81a1e8ade5a45fb02a74bc4db7d1f91b89a54a25e2cf79cba2"
                .to_string(),
            size_bytes: 74_437_848,
        },
        AndroidAsrModelFile {
            target_path: "canary/tokens.txt".to_string(),
            url: format!(
                "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-canary-180m-flash-en-es-de-fr-int8/resolve/{CANARY_REVISION}/tokens.txt"
            ),
            sha256: "2dae6fc7815f9640645e0c765522b278ee0cef49b482d91f6913e334628d3e77"
                .to_string(),
            size_bytes: 53_555,
        },
        silero_vad_file(),
    ]
}

fn streaming_zipformer_files() -> Vec<AndroidAsrModelFile> {
    vec![
        AndroidAsrModelFile {
            target_path: "streaming/encoder.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/d42f2d9f7ca24806fb667456a18a9f1b60f70d16/encoder-epoch-99-avg-1.int8.onnx".to_string(),
            sha256: "3810755ce7c3ab26b42a8bcf39d191308fa27fb0f53358823ba46141d03b7eb3".to_string(),
            size_bytes: 42_845_182,
        },
        AndroidAsrModelFile {
            target_path: "streaming/decoder.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/d42f2d9f7ca24806fb667456a18a9f1b60f70d16/decoder-epoch-99-avg-1.onnx".to_string(),
            sha256: "45a7f940ecfb53d89fa270ad11b88b961e53a317203eb24b1c8e95ed208b0f30".to_string(),
            size_bytes: 2_092_272,
        },
        AndroidAsrModelFile {
            target_path: "streaming/joiner.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/d42f2d9f7ca24806fb667456a18a9f1b60f70d16/joiner-epoch-99-avg-1.int8.onnx".to_string(),
            sha256: "e085d73b593cf9b0707f370dbd656d58327d3fe36d80d849202ef81df02cb01e".to_string(),
            size_bytes: 259_572,
        },
        AndroidAsrModelFile {
            target_path: "streaming/tokens.txt".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-20M-2023-02-17/resolve/d42f2d9f7ca24806fb667456a18a9f1b60f70d16/tokens.txt".to_string(),
            sha256: "49e3c2646595fd907228b3c6787069658f67b17377c60aeb8619c4551b2316fb".to_string(),
            size_bytes: 5_048,
        },
    ]
}

fn whisper_tiny_files() -> Vec<AndroidAsrModelFile> {
    vec![
        AndroidAsrModelFile {
            target_path: "whisper/encoder.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/d026532c022fa99fd789d6b32446a1df7b6bfc43/tiny.en-encoder.int8.onnx".to_string(),
            sha256: "0ce578b827c94a961aacb8fa14b02f096504b337e5c94be37c36238cbe3e8bc6".to_string(),
            size_bytes: 12_937_772,
        },
        AndroidAsrModelFile {
            target_path: "whisper/decoder.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/d026532c022fa99fd789d6b32446a1df7b6bfc43/tiny.en-decoder.int8.onnx".to_string(),
            sha256: "06c0e6ff6348d427e51839219d1c886c18cfdf411e629e33f5e1679bff9c1527".to_string(),
            size_bytes: 89_853_865,
        },
        AndroidAsrModelFile {
            target_path: "whisper/tokens.txt".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-tiny.en/resolve/d026532c022fa99fd789d6b32446a1df7b6bfc43/tiny.en-tokens.txt".to_string(),
            sha256: "306cd27f03c1a714eca7108e03d66b7dc042abe8c258b44c199a7ed9838dd930".to_string(),
            size_bytes: 835_554,
        },
    ]
}

fn whisper_base_files() -> Vec<AndroidAsrModelFile> {
    vec![
        AndroidAsrModelFile {
            target_path: "whisper/encoder.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base.en/resolve/59eea950fc76df2453efb57e6c0fd334548e8ffe/base.en-encoder.int8.onnx".to_string(),
            sha256: "ef6b936f4c9b1d90a3b68634b60c4ed8576b26172b33c2535ec0e933c9edb823".to_string(),
            size_bytes: 29_120_534,
        },
        AndroidAsrModelFile {
            target_path: "whisper/decoder.onnx".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base.en/resolve/59eea950fc76df2453efb57e6c0fd334548e8ffe/base.en-decoder.int8.onnx".to_string(),
            sha256: "f7162ad6db2dbef16cfaeaa7f945b9d7dd9c1b8d472f6aca82f2273d185e4d41".to_string(),
            size_bytes: 130_669_978,
        },
        AndroidAsrModelFile {
            target_path: "whisper/tokens.txt".to_string(),
            url: "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-base.en/resolve/59eea950fc76df2453efb57e6c0fd334548e8ffe/base.en-tokens.txt".to_string(),
            sha256: "306cd27f03c1a714eca7108e03d66b7dc042abe8c258b44c199a7ed9838dd930".to_string(),
            size_bytes: 835_554,
        },
    ]
}

fn silero_vad_file() -> AndroidAsrModelFile {
    AndroidAsrModelFile {
        target_path: "silero_vad_v4.onnx".to_string(),
        url:
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad_v4.onnx"
                .to_string(),
        sha256: "a35ebf52fd3ce5f1469b2a36158dba761bc47b973ea3382b3186ca15b1f5af28".to_string(),
        size_bytes: 1_807_522,
    }
}

impl AndroidAsrModelManager {
    pub fn list_for_app<R: Runtime>(
        &self,
        app: &AppHandle<R>,
    ) -> Result<Vec<AndroidAsrModelPackState>> {
        let root = models_root_for_app(app)?;
        self.list_for_dir(&root)
    }

    pub fn list_for_dir(&self, root: &Path) -> Result<Vec<AndroidAsrModelPackState>> {
        let active = active_model_id_for_dir(root)?;
        builtin_model_packs()
            .iter()
            .map(|pack| self.pack_state_for_dir(root, active.as_deref(), &pack.id))
            .collect()
    }

    pub fn pack_state_for_app<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        model_id: &str,
    ) -> Result<AndroidAsrModelPackState> {
        let root = models_root_for_app(app)?;
        let active = active_model_id_for_dir(&root)?;
        self.pack_state_for_dir(&root, active.as_deref(), model_id)
    }

    pub fn pack_state_for_dir(
        &self,
        root: &Path,
        active_model_id: Option<&str>,
        model_id: &str,
    ) -> Result<AndroidAsrModelPackState> {
        self.pack_state_for_dir_with_ram(root, active_model_id, model_id, device_total_ram_mb())
    }

    fn pack_state_for_dir_with_ram(
        &self,
        root: &Path,
        active_model_id: Option<&str>,
        model_id: &str,
        total_ram_mb: Option<u64>,
    ) -> Result<AndroidAsrModelPackState> {
        let pack = pack_by_id(model_id)?;
        let pack_dir = installed_pack_dir_for_root(root, &pack.id);
        let missing_files = missing_files(&pack, &pack_dir)?;
        let is_downloading = self.is_downloading(&pack.id);
        let is_installed = missing_files.is_empty();
        let has_enough_ram = ram_gate_satisfied(pack.min_ram_mb, total_ram_mb);
        let is_selectable = is_installed && !is_downloading && has_enough_ram;

        Ok(AndroidAsrModelPackState {
            id: pack.id,
            display_name: pack.display_name,
            description: pack.description,
            language: pack.language,
            size_mb: pack.size_mb,
            min_ram_mb: pack.min_ram_mb,
            engine_kind: pack.engine_kind,
            installed_dir: pack_dir.to_string_lossy().into_owned(),
            is_installed,
            is_downloading,
            is_active: active_model_id == Some(model_id) && is_selectable,
            is_selectable,
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

    pub async fn download_pack<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        model_id: &str,
    ) -> Result<()> {
        let pack = pack_by_id(model_id)?;
        ensure_pack_ram_gate(&pack, device_total_ram_mb())?;
        let root = models_root_for_app(app)?;
        fs::create_dir_all(&root)?;

        if self
            .pack_state_for_dir(&root, active_model_id_for_dir(&root)?.as_deref(), &pack.id)?
            .is_installed
        {
            if let Some(state) = self.select_installed_pack_if_active_slot_empty(&root, &pack.id)? {
                sync_native_engine_model_id(app, &state.installed_dir)?;
            }
            emit_model_changed(app, &pack.id);
            return Ok(());
        }

        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut flags = self
                .cancel_flags
                .lock()
                .map_err(|_| anyhow::anyhow!("Android ASR download lock is poisoned"))?;
            if flags.contains_key(&pack.id) {
                return Err(anyhow::anyhow!(
                    "Android ASR model pack {} is already downloading",
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
        let auto_selected = self.auto_select_after_download_completion(&root, &pack.id)?;
        if let Some(state) = auto_selected {
            sync_native_engine_model_id(app, &state.installed_dir)?;
        }
        emit_model_changed(app, &pack.id);

        Ok(())
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        let flags = self
            .cancel_flags
            .lock()
            .map_err(|_| anyhow::anyhow!("Android ASR download lock is poisoned"))?;
        let Some(flag) = flags.get(model_id) else {
            return Err(anyhow::anyhow!(
                "No active Android ASR model download for {}",
                model_id
            ));
        };

        flag.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub fn select_pack<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        model_id: &str,
    ) -> Result<AndroidAsrModelPackState> {
        let root = models_root_for_app(app)?;
        let pack = pack_by_id(model_id)?;
        let state = self.pack_state_for_dir(&root, Some(model_id), model_id)?;
        if !state.is_selectable {
            ensure_pack_ram_gate(&pack, device_total_ram_mb())?;
            return Err(anyhow::anyhow!(
                "Android ASR model pack {} is not installed",
                model_id
            ));
        }

        write_active_model_id(&root, model_id)?;
        Ok(AndroidAsrModelPackState {
            is_active: true,
            ..state
        })
    }

    fn auto_select_after_download_completion(
        &self,
        root: &Path,
        model_id: &str,
    ) -> Result<Option<AndroidAsrModelPackState>> {
        self.cancel_flags
            .lock()
            .map_err(|_| anyhow::anyhow!("Android ASR download lock is poisoned"))?
            .remove(model_id);

        self.select_installed_pack_if_active_slot_empty(root, model_id)
    }

    fn select_installed_pack_if_active_slot_empty(
        &self,
        root: &Path,
        model_id: &str,
    ) -> Result<Option<AndroidAsrModelPackState>> {
        if active_model_id_for_dir(root)?.is_some() {
            return Ok(None);
        }

        let state = self.pack_state_for_dir(root, Some(model_id), model_id)?;
        if !state.is_selectable {
            return Err(anyhow::anyhow!(
                "Android ASR model pack {} is not installed",
                model_id
            ));
        }

        write_active_model_id(root, model_id)?;
        Ok(Some(AndroidAsrModelPackState {
            is_active: true,
            ..state
        }))
    }

    pub fn delete_pack<R: Runtime>(&self, app: &AppHandle<R>, model_id: &str) -> Result<()> {
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

async fn download_component<R: Runtime>(
    app: &AppHandle<R>,
    model_id: &str,
    file: &AndroidAsrModelFile,
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
            .ok_or_else(|| anyhow::anyhow!("Invalid model component path"))?
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
            "Failed to download Android ASR component {}: HTTP {}",
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
                "Android ASR model download cancelled for {}",
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
                "Android ASR component {} incomplete: expected {} bytes, got {} bytes",
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
        .map_err(|err| anyhow::anyhow!("Android ASR SHA-256 task failed: {}", err))??;

    fs::rename(&partial_path, &staging_path)?;
    Ok(())
}

fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
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
        "android-asr-model-progress",
        AndroidAsrDownloadProgress {
            model_id: model_id.to_string(),
            phase: phase.to_string(),
            file,
            downloaded,
            total,
            percentage,
        },
    );
}

fn emit_model_changed<R: Runtime>(app: &AppHandle<R>, model_id: &str) {
    let _ = app.emit("android-asr-model-changed", model_id);
}

#[cfg(target_os = "android")]
fn sync_native_engine_model_id<R: Runtime>(app: &AppHandle<R>, installed_dir: &str) -> Result<()> {
    use tauri_plugin_verbatim_android::VerbatimAndroidExt;

    app.verbatim_android()
        .set_engine_model_id(installed_dir.to_string())
        .map(|_| ())
        .map_err(|error| {
            anyhow::anyhow!(
                "Failed to sync Android ASR native engine model selection: {}",
                error
            )
        })
}

#[cfg(not(target_os = "android"))]
fn sync_native_engine_model_id<R: Runtime>(
    _app: &AppHandle<R>,
    _installed_dir: &str,
) -> Result<()> {
    Ok(())
}

fn pack_by_id(model_id: &str) -> Result<AndroidAsrModelPack> {
    builtin_model_packs()
        .into_iter()
        .find(|pack| pack.id == model_id)
        .ok_or_else(|| anyhow::anyhow!("Android ASR model pack not found: {}", model_id))
}

fn ensure_pack_ram_gate(pack: &AndroidAsrModelPack, total_ram_mb: Option<u64>) -> Result<()> {
    if ram_gate_satisfied(pack.min_ram_mb, total_ram_mb) {
        return Ok(());
    }

    let reported_ram = total_ram_mb.unwrap_or_default();
    Err(anyhow::anyhow!(
        "Android ASR model pack {} requires at least {} MB RAM; this device reports {} MB",
        pack.id,
        pack.min_ram_mb,
        reported_ram
    ))
}

fn ram_gate_satisfied(min_ram_mb: u64, total_ram_mb: Option<u64>) -> bool {
    min_ram_mb == 0
        || total_ram_mb
            .map(|total| total >= min_ram_mb)
            .unwrap_or(true)
}

#[cfg(target_os = "android")]
fn device_total_ram_mb() -> Option<u64> {
    parse_meminfo_total_ram_mb(&fs::read_to_string("/proc/meminfo").ok()?)
}

#[cfg(target_os = "android")]
fn parse_meminfo_total_ram_mb(contents: &str) -> Option<u64> {
    let line = contents
        .lines()
        .find(|line| line.trim_start().starts_with("MemTotal:"))?;
    let kb = line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())?;
    Some(kb / 1024)
}

#[cfg(not(target_os = "android"))]
fn device_total_ram_mb() -> Option<u64> {
    None
}

fn models_root_for_app<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    Ok(crate::portable::app_data_dir(app)
        .map_err(|err| anyhow::anyhow!("Failed to get app data dir: {}", err))?
        .join(MODELS_SUBDIR))
}

pub(crate) fn installed_pack_dir_for_app<R: Runtime>(
    app: &AppHandle<R>,
    model_id: &str,
) -> Result<PathBuf> {
    Ok(installed_pack_dir_for_root(
        &models_root_for_app(app)?,
        model_id,
    ))
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

fn missing_files(pack: &AndroidAsrModelPack, pack_dir: &Path) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for file in &pack.files {
        let target_rel = safe_relative_path(&file.target_path)?;
        if !pack_dir.join(target_rel).is_file() {
            missing.push(file.target_path.clone());
        }
    }
    Ok(missing)
}

fn ensure_pack_layout(pack: &AndroidAsrModelPack, pack_dir: &Path) -> Result<()> {
    let missing = missing_files(pack, pack_dir)?;
    if missing.is_empty() {
        return Ok(());
    }

    Err(anyhow::anyhow!(
        "Android ASR model pack {} is incomplete; missing {}",
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
            "Failed to verify Android ASR component {}: {}",
            component,
            err
        )
    })?;

    if actual.eq_ignore_ascii_case(expected_sha256) {
        return Ok(());
    }

    let _ = fs::remove_file(path);
    Err(anyhow::anyhow!(
        "Android ASR component verification failed for {}: expected {}, got {}",
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
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn manifest_contains_one_extensible_pack_with_expected_layout() {
        let packs = builtin_model_packs();

        assert_eq!(packs.len(), 4);
        assert_eq!(packs[0].id, "g3-zipformer-whisper-tiny-en");
        assert_eq!(packs[1].id, "g3-zipformer-whisper-base-en");
        assert_eq!(packs[2].id, "sensevoice-multilingual-zh-en-ja-ko-yue");
        assert_eq!(packs[3].id, "canary-180m-flash-en-es-de-fr");

        for pack in &packs {
            match pack.engine_kind {
                AndroidAsrEngineKind::ZipformerWhisper => {
                    assert_eq!(pack.files.len(), 8);
                    assert_eq!(component_targets(pack), zipformer_whisper_targets());
                }
                AndroidAsrEngineKind::SenseVoice => {
                    assert_eq!(pack.files.len(), 3);
                    assert_eq!(component_targets(pack), sense_voice_targets());
                }
                AndroidAsrEngineKind::Canary => {
                    assert_eq!(pack.files.len(), 4);
                    assert_eq!(component_targets(pack), canary_targets());
                    assert_eq!(pack.min_ram_mb, 6144);
                    assert_eq!(pack.size_mb, 207);
                }
            }
            assert!(pack.files.iter().all(|file| is_sha256_hex(&file.sha256)));
            assert!(pack
                .files
                .iter()
                .filter(|file| file.url.contains("huggingface.co"))
                .all(|file| file.url.contains("/resolve/") && !file.url.contains("/main/")));
        }
    }

    #[test]
    fn higher_accuracy_pack_reuses_streaming_and_vad_files_with_base_whisper() {
        let packs = builtin_model_packs();
        let starter = packs
            .iter()
            .find(|pack| pack.id == "g3-zipformer-whisper-tiny-en")
            .unwrap();
        let higher_accuracy = packs
            .iter()
            .find(|pack| pack.id == "g3-zipformer-whisper-base-en")
            .unwrap();

        for target_path in [
            "streaming/encoder.onnx",
            "streaming/decoder.onnx",
            "streaming/joiner.onnx",
            "streaming/tokens.txt",
            "silero_vad_v4.onnx",
        ] {
            let starter_file = starter
                .files
                .iter()
                .find(|file| file.target_path == target_path)
                .unwrap();
            let higher_accuracy_file = higher_accuracy
                .files
                .iter()
                .find(|file| file.target_path == target_path)
                .unwrap();

            assert_eq!(higher_accuracy_file.url, starter_file.url);
            assert_eq!(higher_accuracy_file.sha256, starter_file.sha256);
            assert_eq!(higher_accuracy_file.size_bytes, starter_file.size_bytes);
        }

        assert!(higher_accuracy
            .files
            .iter()
            .filter(|file| file.target_path.starts_with("whisper/"))
            .all(|file| file
                .url
                .contains("csukuangfj/sherpa-onnx-whisper-base.en/resolve/59eea950fc76df2453efb57e6c0fd334548e8ffe")));
    }

    #[test]
    fn manifest_supports_zipformer_whisper_and_sensevoice_layouts() {
        let packs = builtin_model_packs();
        let starter = packs
            .iter()
            .find(|pack| pack.id == "g3-zipformer-whisper-tiny-en")
            .unwrap();
        let sensevoice = packs
            .iter()
            .find(|pack| pack.id == "sensevoice-multilingual-zh-en-ja-ko-yue")
            .unwrap();
        let canary = packs
            .iter()
            .find(|pack| pack.id == "canary-180m-flash-en-es-de-fr")
            .unwrap();

        assert_eq!(starter.engine_kind, AndroidAsrEngineKind::ZipformerWhisper);
        assert_eq!(component_targets(starter), zipformer_whisper_targets());
        assert_eq!(sensevoice.engine_kind, AndroidAsrEngineKind::SenseVoice);
        assert_eq!(
            component_targets(sensevoice),
            vec![
                "sense_voice/model.onnx",
                "sense_voice/tokens.txt",
                "silero_vad_v4.onnx"
            ]
        );
        assert_eq!(canary.engine_kind, AndroidAsrEngineKind::Canary);
        assert_eq!(component_targets(canary), canary_targets());
    }

    #[test]
    fn canary_pack_is_not_selectable_below_ram_gate() {
        let temp = tempfile::tempdir().unwrap();
        let pack_dir = write_complete_named_pack(temp.path(), "canary-180m-flash-en-es-de-fr");
        let manager = AndroidAsrModelManager::default();

        let too_small = manager
            .pack_state_for_dir_with_ram(
                temp.path(),
                Some("canary-180m-flash-en-es-de-fr"),
                "canary-180m-flash-en-es-de-fr",
                Some(4096),
            )
            .unwrap();

        assert!(too_small.is_installed);
        assert!(!too_small.is_selectable);
        assert!(!too_small.is_active);
        assert_eq!(too_small.min_ram_mb, 6144);

        let enough_ram = manager
            .pack_state_for_dir_with_ram(
                temp.path(),
                Some("canary-180m-flash-en-es-de-fr"),
                "canary-180m-flash-en-es-de-fr",
                Some(6144),
            )
            .unwrap();

        assert!(enough_ram.is_selectable);
        assert!(enough_ram.is_active);
        assert_eq!(
            enough_ram.installed_dir,
            pack_dir.to_string_lossy().into_owned()
        );
    }

    #[test]
    fn installed_pack_state_rejects_missing_required_files() {
        let temp = tempfile::tempdir().unwrap();
        let manager = AndroidAsrModelManager::default();

        let state = manager
            .pack_state_for_dir(
                temp.path(),
                Some("g3-zipformer-whisper-tiny-en"),
                "g3-zipformer-whisper-tiny-en",
            )
            .unwrap();

        assert!(!state.is_installed);
        assert!(!state.is_selectable);
        assert!(state
            .missing_files
            .contains(&"streaming/encoder.onnx".into()));
    }

    #[test]
    fn installed_pack_state_accepts_complete_required_layout() {
        let temp = tempfile::tempdir().unwrap();
        let pack_dir = write_complete_pack(temp.path());

        let manager = AndroidAsrModelManager::default();
        let state = manager
            .pack_state_for_dir(
                temp.path(),
                Some("g3-zipformer-whisper-tiny-en"),
                "g3-zipformer-whisper-tiny-en",
            )
            .unwrap();

        assert!(state.is_installed);
        assert!(state.is_selectable);
        assert!(state.is_active);
        assert_eq!(state.installed_dir, pack_dir.to_string_lossy().into_owned());
        assert!(state.missing_files.is_empty());
    }

    #[test]
    fn download_completion_auto_selects_installed_pack_when_active_slot_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        let pack_dir = write_complete_pack(temp.path());
        let manager = AndroidAsrModelManager::default();

        let state = manager
            .select_installed_pack_if_active_slot_empty(temp.path(), "g3-zipformer-whisper-tiny-en")
            .unwrap()
            .expect("empty active slot should select the freshly installed pack");

        assert!(state.is_active);
        assert_eq!(state.installed_dir, pack_dir.to_string_lossy().into_owned());
        assert_eq!(
            active_model_id_for_dir(temp.path()).unwrap().as_deref(),
            Some("g3-zipformer-whisper-tiny-en")
        );
    }

    #[test]
    fn download_completion_clears_registered_flag_before_auto_selecting() {
        let temp = tempfile::tempdir().unwrap();
        let pack_dir = write_complete_pack(temp.path());
        let manager = AndroidAsrModelManager::default();
        manager.cancel_flags.lock().unwrap().insert(
            "g3-zipformer-whisper-tiny-en".to_string(),
            Arc::new(AtomicBool::new(false)),
        );

        let state = manager
            .auto_select_after_download_completion(temp.path(), "g3-zipformer-whisper-tiny-en")
            .unwrap()
            .expect("download completion should select the freshly installed pack");

        assert!(state.is_active);
        assert_eq!(state.installed_dir, pack_dir.to_string_lossy().into_owned());
        assert!(!manager.is_downloading("g3-zipformer-whisper-tiny-en"));
        assert_eq!(
            active_model_id_for_dir(temp.path()).unwrap().as_deref(),
            Some("g3-zipformer-whisper-tiny-en")
        );
    }

    #[test]
    fn download_completion_preserves_existing_active_pack() {
        let temp = tempfile::tempdir().unwrap();
        write_complete_pack(temp.path());
        write_active_model_id(temp.path(), "already-active-pack").unwrap();
        let manager = AndroidAsrModelManager::default();

        let state = manager
            .select_installed_pack_if_active_slot_empty(temp.path(), "g3-zipformer-whisper-tiny-en")
            .unwrap();

        assert!(state.is_none());
        assert_eq!(
            active_model_id_for_dir(temp.path()).unwrap().as_deref(),
            Some("already-active-pack")
        );
    }

    #[tokio::test]
    async fn download_pack_with_empty_active_slot_auto_selects_installed_pack() {
        let app = test_app("asr-empty-active");
        let app_data_dir = crate::portable::app_data_dir(app.handle()).unwrap();
        let _cleanup = AppDataCleanup(app_data_dir);
        let root = models_root_for_app(app.handle()).unwrap();
        write_complete_pack(&root);
        let manager = AndroidAsrModelManager::default();

        manager
            .download_pack(app.handle(), "g3-zipformer-whisper-tiny-en")
            .await
            .unwrap();

        assert_eq!(
            active_model_id_for_dir(&root).unwrap().as_deref(),
            Some("g3-zipformer-whisper-tiny-en")
        );
    }

    #[tokio::test]
    async fn download_pack_preserves_existing_active_pack() {
        let app = test_app("asr-existing-active");
        let app_data_dir = crate::portable::app_data_dir(app.handle()).unwrap();
        let _cleanup = AppDataCleanup(app_data_dir);
        let root = models_root_for_app(app.handle()).unwrap();
        write_complete_pack(&root);
        write_active_model_id(&root, "already-active-pack").unwrap();
        let manager = AndroidAsrModelManager::default();

        manager
            .download_pack(app.handle(), "g3-zipformer-whisper-tiny-en")
            .await
            .unwrap();

        assert_eq!(
            active_model_id_for_dir(&root).unwrap().as_deref(),
            Some("already-active-pack")
        );
    }

    #[test]
    fn model_component_paths_must_stay_inside_pack_dir() {
        assert_eq!(
            safe_relative_path("streaming/encoder.onnx").unwrap(),
            PathBuf::from("streaming/encoder.onnx")
        );

        assert!(safe_relative_path("../encoder.onnx").is_err());
        assert!(safe_relative_path("/tmp/encoder.onnx").is_err());
        assert!(safe_relative_path("streaming/../encoder.onnx").is_err());
    }

    #[test]
    fn sha256_mismatch_rejects_downloaded_component() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("component.onnx");
        fs::write(&file, b"not the expected model").unwrap();

        let error = verify_sha256(&file, "0".repeat(64).as_str(), "component.onnx")
            .expect_err("mismatch should reject the file");

        assert!(error.to_string().contains("verification failed"));
    }

    fn write_complete_pack(root: &Path) -> PathBuf {
        write_complete_named_pack(root, "g3-zipformer-whisper-tiny-en")
    }

    fn write_complete_named_pack(root: &Path, model_id: &str) -> PathBuf {
        let pack = builtin_model_packs()
            .into_iter()
            .find(|pack| pack.id == model_id)
            .unwrap();
        let pack_dir = root.join(model_id);
        for file in &pack.files {
            let target = pack_dir.join(&file.target_path);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(target, b"fixture").unwrap();
        }
        pack_dir
    }

    fn test_app(name: &str) -> tauri::App<tauri::test::MockRuntime> {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        context.config_mut().identifier = format!(
            "com.galaxyruler.verbatim.test.{}.{}.{}",
            name,
            std::process::id(),
            unique
        );
        tauri::test::mock_builder().build(context).unwrap()
    }

    struct AppDataCleanup(PathBuf);

    impl Drop for AppDataCleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn component_targets(pack: &AndroidAsrModelPack) -> Vec<String> {
        pack.files
            .iter()
            .map(|file| file.target_path.clone())
            .collect()
    }

    fn zipformer_whisper_targets() -> Vec<String> {
        [
            "streaming/encoder.onnx",
            "streaming/decoder.onnx",
            "streaming/joiner.onnx",
            "streaming/tokens.txt",
            "whisper/encoder.onnx",
            "whisper/decoder.onnx",
            "whisper/tokens.txt",
            "silero_vad_v4.onnx",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    fn sense_voice_targets() -> Vec<String> {
        [
            "sense_voice/model.onnx",
            "sense_voice/tokens.txt",
            "silero_vad_v4.onnx",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    fn canary_targets() -> Vec<String> {
        [
            "canary/encoder.onnx",
            "canary/decoder.onnx",
            "canary/tokens.txt",
            "silero_vad_v4.onnx",
        ]
        .into_iter()
        .map(String::from)
        .collect()
    }

    fn is_sha256_hex(value: &str) -> bool {
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}
