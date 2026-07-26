use crate::settings::{get_settings, mutate_settings_locked};
use anyhow::Result;
use flate2::read::GzDecoder;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tar::Archive;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub enum EngineType {
    Whisper,
    Parakeet,
    Moonshine,
    MoonshineStreaming,
    SenseVoice,
    GigaAM,
    Canary,
    Cohere,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub filename: String,
    pub url: Option<String>,
    pub sha256: Option<String>,
    pub size_mb: u64,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub partial_size: u64,
    pub is_directory: bool,
    pub engine_type: EngineType,
    pub license_label: String,
    pub accelerator_support: Vec<String>,
    pub accuracy_score: f32,        // 0.0 to 1.0, higher is more accurate
    pub speed_score: f32,           // 0.0 to 1.0, higher is faster
    pub supports_translation: bool, // Whether the model supports translating to English
    pub is_recommended: bool,       // Whether this is the recommended model for new users
    pub supported_languages: Vec<String>, // Languages this model can transcribe
    pub supports_language_selection: bool, // Whether the user can explicitly pick a language
    pub is_custom: bool,            // Whether this is a user-provided custom model
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

/// RAII guard that cleans up download state (`is_downloading` flag and cancel flag)
/// when dropped, unless explicitly disarmed. This ensures consistent cleanup on
/// every error path without requiring manual cleanup at each `?` or `return Err`.
struct DownloadCleanup<'a> {
    available_models: &'a Mutex<HashMap<String, ModelInfo>>,
    cancel_flags: &'a Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    model_id: String,
    disarmed: bool,
}

impl<'a> Drop for DownloadCleanup<'a> {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(self.model_id.as_str()) {
                model.is_downloading = false;
            }
        }
        self.cancel_flags.lock().unwrap().remove(&self.model_id);
    }
}

pub struct ModelManager {
    app_handle: AppHandle,
    models_dir: PathBuf,
    available_models: Mutex<HashMap<String, ModelInfo>>,
    cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    extracting_models: Arc<Mutex<HashSet<String>>>,
    // Built-in model files are verified once immediately before first use.
    // Keeping only identifiers here avoids re-hashing large models on every
    // dictation while never persisting a trust decision across app launches.
    verified_model_ids: Mutex<HashSet<String>>,
}

const DIRECTORY_INTEGRITY_MANIFEST: &str = ".verbatim-integrity.json";
const DIRECTORY_INTEGRITY_MANIFEST_PARTIAL: &str = ".verbatim-integrity.json.partial";

/// Integrity data recorded after extracting a built-in directory model from a
/// checksum-verified archive. It detects later partial writes or disk changes
/// before the model reaches an inference engine.
///
/// This is intentionally an integrity check, not a trust boundary: a local
/// user who can alter a model directory can also alter this manifest. The
/// archive checksum remains the authenticity check at download time.
#[derive(Debug, Serialize, Deserialize)]
struct DirectoryIntegrityManifest {
    schema_version: u8,
    model_id: String,
    archive_sha256: String,
    files: BTreeMap<String, String>,
}

impl ModelManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // A native smoke can point at a disposable model directory so it never
        // reads or mutates a user's downloaded models. The override is active
        // only with the native-smoke status contract.
        let models_dir = crate::native_smoke::model_directory_override().unwrap_or(
            crate::portable::app_data_dir(app_handle)
                .map_err(|e| anyhow::anyhow!("Failed to get app data dir: {}", e))?
                .join("models"),
        );

        if !models_dir.exists() {
            fs::create_dir_all(&models_dir)?;
        }

        Self::prepare_native_smoke_model_fixture(&models_dir)?;

        let mut available_models = super::model_catalog::load_builtin_models()?;

        // Auto-discover custom Whisper models (.bin files) in the models directory
        if let Err(e) = Self::discover_custom_whisper_models(&models_dir, &mut available_models) {
            warn!("Failed to discover custom models: {}", e);
        }

        let manager = Self {
            app_handle: app_handle.clone(),
            models_dir,
            available_models: Mutex::new(available_models),
            cancel_flags: Arc::new(Mutex::new(HashMap::new())),
            extracting_models: Arc::new(Mutex::new(HashSet::new())),
            verified_model_ids: Mutex::new(HashSet::new()),
        };

        // Migrate any bundled models to user directory
        manager.migrate_bundled_models()?;

        // Migrate GigaAM from single-file to directory format
        manager.migrate_gigaam_to_directory()?;

        // Check which models are already downloaded
        manager.update_download_status()?;

        // Auto-select a model if none is currently selected
        manager.auto_select_model_if_needed()?;

        Ok(manager)
    }

    pub fn get_available_models(&self) -> Vec<ModelInfo> {
        let models = self.available_models.lock().unwrap();
        models.values().cloned().collect()
    }

    pub fn get_model_info(&self, model_id: &str) -> Option<ModelInfo> {
        let models = self.available_models.lock().unwrap();
        models.get(model_id).cloned()
    }

    fn migrate_bundled_models(&self) -> Result<()> {
        // Check for bundled models and copy them to user directory
        let bundled_models = ["ggml-small.bin"]; // Add other bundled models here if any

        for filename in &bundled_models {
            let bundled_path = crate::utils::resolve_resource_path(
                &self.app_handle,
                &format!("resources/models/{}", filename),
            );

            if let Ok(bundled_path) = bundled_path {
                if bundled_path.exists() {
                    let user_path = self.models_dir.join(filename);

                    // Only copy if user doesn't already have the model
                    if !user_path.exists() {
                        info!("Migrating bundled model {} to user directory", filename);
                        fs::copy(&bundled_path, &user_path)?;
                        info!("Successfully migrated {}", filename);
                    }
                }
            }
        }

        Ok(())
    }

    /// Migrate GigaAM from the old single-file format (giga-am-v3.int8.onnx)
    /// to the new directory format (giga-am-v3-int8/model.int8.onnx + vocab.txt).
    /// This was required by the transcribe-rs 0.3.x upgrade.
    fn migrate_gigaam_to_directory(&self) -> Result<()> {
        let old_file = self.models_dir.join("giga-am-v3.int8.onnx");
        let new_dir = self.models_dir.join("giga-am-v3-int8");

        if !old_file.exists() || new_dir.exists() {
            return Ok(());
        }

        info!("Migrating GigaAM from single-file to directory format");

        let vocab_path = crate::utils::resolve_resource_path(
            &self.app_handle,
            "resources/models/gigaam_vocab.txt",
        )
        .map_err(|e| anyhow::anyhow!("Failed to resolve GigaAM vocab path: {}", e))?;

        info!(
            "Resolved vocab path: {:?} (exists: {})",
            vocab_path,
            vocab_path.exists()
        );
        info!("Old file: {:?} (exists: {})", old_file, old_file.exists());
        info!("New dir: {:?} (exists: {})", new_dir, new_dir.exists());

        fs::create_dir_all(&new_dir)?;
        fs::rename(&old_file, new_dir.join("model.int8.onnx"))?;
        fs::copy(&vocab_path, new_dir.join("vocab.txt"))?;

        // Clean up old partial file if it exists
        let old_partial = self.models_dir.join("giga-am-v3.int8.onnx.partial");
        if old_partial.exists() {
            let _ = fs::remove_file(&old_partial);
        }

        info!("GigaAM migration complete");
        Ok(())
    }

    fn update_download_status(&self) -> Result<()> {
        let mut models = self.available_models.lock().unwrap();

        for model in models.values_mut() {
            if model.is_directory {
                // For directory-based models, check if the directory exists
                let model_path = self.models_dir.join(&model.filename);
                let partial_path = self.models_dir.join(format!("{}.partial", &model.filename));
                let extracting_path = self
                    .models_dir
                    .join(format!("{}.extracting", &model.filename));

                // Clean up any leftover .extracting directories from interrupted extractions
                // But only if this model is NOT currently being extracted
                let is_currently_extracting = {
                    let extracting = self.extracting_models.lock().unwrap();
                    extracting.contains(&model.id)
                };
                if extracting_path.exists() && !is_currently_extracting {
                    warn!("Cleaning up interrupted extraction for model: {}", model.id);
                    let _ = fs::remove_dir_all(&extracting_path);
                }

                model.is_downloaded = model_path.exists() && model_path.is_dir();
                model.is_downloading = false;

                // Get partial file size if it exists (for the .tar.gz being downloaded)
                if partial_path.exists() {
                    model.partial_size = partial_path.metadata().map(|m| m.len()).unwrap_or(0);
                } else {
                    model.partial_size = 0;
                }
            } else {
                // For file-based models (existing logic)
                let model_path = self.models_dir.join(&model.filename);
                let partial_path = self.models_dir.join(format!("{}.partial", &model.filename));

                model.is_downloaded = model_path.exists();
                model.is_downloading = false;

                // Get partial file size if it exists
                if partial_path.exists() {
                    model.partial_size = partial_path.metadata().map(|m| m.len()).unwrap_or(0);
                } else {
                    model.partial_size = 0;
                }
            }
        }

        Ok(())
    }

    fn auto_select_model_if_needed(&self) -> Result<()> {
        // Snapshot the model list up front so the settings-lock closure below never
        // nests the available_models mutex inside the settings write lock.
        let (known_model_ids, first_downloaded) = {
            let models = self.available_models.lock().unwrap();
            let known: HashSet<String> = models.keys().cloned().collect();
            let first = models
                .values()
                .find(|model| model.is_downloaded)
                .map(|model| (model.id.clone(), model.name.clone()));
            (known, first)
        };

        // Fast path: current selection exists in available_models — nothing to mutate,
        // so skip taking the settings write lock entirely.
        let current = get_settings(&self.app_handle).selected_model;
        if !current.is_empty() && known_model_ids.contains(&current) {
            return Ok(());
        }

        mutate_settings_locked(&self.app_handle, |settings| {
            // Clear stale selection: selected model is set but doesn't exist
            // in available_models (e.g. deleted custom model file)
            if !settings.selected_model.is_empty()
                && !known_model_ids.contains(&settings.selected_model)
            {
                info!(
                    "Selected model '{}' not found in available models, clearing selection",
                    settings.selected_model
                );
                settings.selected_model = String::new();
            }

            // If no model is selected, pick the first downloaded one
            if settings.selected_model.is_empty() {
                if let Some((id, name)) = &first_downloaded {
                    info!("Auto-selecting model: {} ({})", id, name);
                    settings.selected_model = id.clone();
                    info!("Successfully auto-selected model: {}", id);
                }
            }
        });

        Ok(())
    }

    /// Prepare a tiny local model placeholder for packaged native smoke tests.
    fn prepare_native_smoke_model_fixture(models_dir: &Path) -> Result<()> {
        if std::env::var("VERBATIM_SMOKE_MODEL_FIXTURE").as_deref() != Ok("1") {
            return Ok(());
        }

        let fixture_path = Self::write_native_smoke_model_fixture(models_dir)?;
        info!(
            "Prepared native smoke model fixture at {}",
            fixture_path.display()
        );

        Ok(())
    }

    fn write_native_smoke_model_fixture(models_dir: &Path) -> Result<PathBuf> {
        let fixture_path = models_dir.join("verbatim-smoke-model.bin");
        fs::write(
            &fixture_path,
            [
                "verbatim native smoke fixture",
                "This is not a real speech model and must never be used for inference.",
                "",
            ]
            .join("\n"),
        )?;

        Ok(fixture_path)
    }

    /// Discover custom Whisper models (.bin files) in the models directory.
    /// Skips files that match predefined model filenames.
    fn discover_custom_whisper_models(
        models_dir: &Path,
        available_models: &mut HashMap<String, ModelInfo>,
    ) -> Result<()> {
        if !models_dir.exists() {
            return Ok(());
        }

        // Collect filenames of predefined Whisper file-based models to skip
        let predefined_filenames: HashSet<String> = available_models
            .values()
            .filter(|m| matches!(m.engine_type, EngineType::Whisper) && !m.is_directory)
            .map(|m| m.filename.clone())
            .collect();

        // Scan models directory for .bin files
        for entry in fs::read_dir(models_dir)? {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to read directory entry: {}", e);
                    continue;
                }
            };

            let path = entry.path();

            // Only process .bin files (not directories)
            if !path.is_file() {
                continue;
            }

            let filename = match path.file_name().and_then(|s| s.to_str()) {
                Some(name) => name.to_string(),
                None => continue,
            };

            // Skip hidden files
            if filename.starts_with('.') {
                continue;
            }

            // Only process .bin files (Whisper GGML format).
            // This also excludes .partial downloads (e.g., "model.bin.partial").
            // If we add discovery for other formats, add a .partial check before this filter.
            if !filename.ends_with(".bin") {
                continue;
            }

            // Skip predefined model files
            if predefined_filenames.contains(&filename) {
                continue;
            }

            // Generate model ID from filename (remove .bin extension)
            let model_id = filename.trim_end_matches(".bin").to_string();

            // Skip if model ID already exists (shouldn't happen, but be safe)
            if available_models.contains_key(&model_id) {
                continue;
            }

            // Generate display name: replace - and _ with space, capitalize words
            let display_name = model_id
                .replace(['-', '_'], " ")
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            // Get file size in MB
            let size_mb = match path.metadata() {
                Ok(meta) => meta.len() / (1024 * 1024),
                Err(e) => {
                    warn!("Failed to get metadata for {}: {}", filename, e);
                    0
                }
            };

            info!(
                "Discovered custom Whisper model: {} ({}, {} MB)",
                model_id, filename, size_mb
            );

            available_models.insert(
                model_id.clone(),
                ModelInfo {
                    id: model_id,
                    name: display_name,
                    description: "Not officially supported".to_string(),
                    filename,
                    url: None,    // Custom models have no download URL
                    sha256: None, // Custom models skip verification
                    size_mb,
                    is_downloaded: true, // Already present on disk
                    is_downloading: false,
                    partial_size: 0,
                    is_directory: false,
                    engine_type: EngineType::Whisper,
                    license_label: "Unverified user-provided model".to_string(),
                    accelerator_support: vec!["whisper-cpp".to_string()],
                    accuracy_score: 0.0, // Sentinel: UI hides score bars when both are 0
                    speed_score: 0.0,
                    supports_translation: false,
                    is_recommended: false,
                    supported_languages: vec![],
                    supports_language_selection: true,
                    is_custom: true,
                },
            );
        }

        Ok(())
    }

    /// Verifies the SHA256 of `path` against `expected_sha256` (if provided).
    /// On mismatch or read error the partial file is deleted and an error is returned,
    /// so the next download attempt always starts from a clean state.
    /// When `expected_sha256` is `None` (custom user models) verification is skipped.
    fn verify_sha256(path: &Path, expected_sha256: Option<&str>, model_id: &str) -> Result<()> {
        let Some(expected) = expected_sha256 else {
            return Ok(());
        };
        match Self::compute_sha256(path) {
            Ok(actual) if actual == expected => {
                info!("SHA256 verified for model {}", model_id);
                Ok(())
            }
            Ok(actual) => {
                warn!(
                    "SHA256 mismatch for model {}: expected {}, got {}",
                    model_id, expected, actual
                );
                let _ = fs::remove_file(path);
                Err(anyhow::anyhow!(
                    "Download verification failed for model {}: file is corrupt. Please retry.",
                    model_id
                ))
            }
            Err(e) => {
                let _ = fs::remove_file(path);
                Err(anyhow::anyhow!(
                    "Failed to verify download for model {}: {}. Please retry.",
                    model_id,
                    e
                ))
            }
        }
    }

    /// Verifies a completed built-in model before loading it. Unlike a partial
    /// download failure, a mismatch here removes the completed file itself so
    /// a corrupt asset cannot remain selectable or reach the inference engine.
    fn verify_existing_model_file(
        path: &Path,
        expected_sha256: Option<&str>,
        model_id: &str,
    ) -> Result<()> {
        let Some(expected) = expected_sha256 else {
            return Ok(());
        };

        match Self::compute_sha256(path) {
            Ok(actual) if actual == expected => {
                info!("SHA256 verified existing model {}", model_id);
                Ok(())
            }
            Ok(actual) => {
                warn!(
                    "SHA256 mismatch for existing model {}: expected {}, got {}",
                    model_id, expected, actual
                );
                let _ = fs::remove_file(path);
                Err(anyhow::anyhow!(
                    "Model verification failed for {}: the local file is corrupt. Please download it again.",
                    model_id
                ))
            }
            Err(error) => {
                let _ = fs::remove_file(path);
                Err(anyhow::anyhow!(
                    "Failed to verify local model {}: {}. Please download it again.",
                    model_id,
                    error
                ))
            }
        }
    }

    fn directory_file_hashes(root: &Path) -> Result<BTreeMap<String, String>> {
        fn collect(
            root: &Path,
            current: &Path,
            files: &mut BTreeMap<String, String>,
        ) -> Result<()> {
            let mut entries = fs::read_dir(current)?.collect::<std::io::Result<Vec<_>>>()?;
            entries.sort_by_key(|entry| entry.file_name());

            for entry in entries {
                let path = entry.path();
                let file_type = entry.file_type()?;
                if file_type.is_symlink() {
                    anyhow::bail!(
                        "Model directory contains an unsupported symbolic link: {}",
                        path.display()
                    );
                }

                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| anyhow::anyhow!("Resolve model file path: {error}"))?;
                if relative == Path::new(DIRECTORY_INTEGRITY_MANIFEST)
                    || relative == Path::new(DIRECTORY_INTEGRITY_MANIFEST_PARTIAL)
                {
                    continue;
                }

                if file_type.is_dir() {
                    collect(root, &path, files)?;
                    continue;
                }
                if !file_type.is_file() {
                    anyhow::bail!(
                        "Model directory contains an unsupported entry: {}",
                        path.display()
                    );
                }

                let key = relative.to_string_lossy().replace('\\', "/");
                files.insert(key, ModelManager::compute_sha256(&path)?);
            }
            Ok(())
        }

        let mut files = BTreeMap::new();
        collect(root, root, &mut files)?;
        if files.is_empty() {
            anyhow::bail!(
                "Model directory contains no model files: {}",
                root.display()
            );
        }
        Ok(files)
    }

    fn write_directory_integrity_manifest(
        path: &Path,
        model_id: &str,
        expected_sha256: Option<&str>,
    ) -> Result<()> {
        let Some(expected_sha256) = expected_sha256 else {
            return Ok(());
        };

        let manifest = DirectoryIntegrityManifest {
            schema_version: 1,
            model_id: model_id.to_string(),
            archive_sha256: expected_sha256.to_ascii_lowercase(),
            files: Self::directory_file_hashes(path)?,
        };
        let manifest_path = path.join(DIRECTORY_INTEGRITY_MANIFEST);
        let partial_path = path.join(DIRECTORY_INTEGRITY_MANIFEST_PARTIAL);
        fs::write(&partial_path, serde_json::to_vec_pretty(&manifest)?)?;
        fs::rename(&partial_path, &manifest_path)?;
        Ok(())
    }

    /// Validate an extracted built-in model against the file manifest produced
    /// after its checksum-verified archive was unpacked. Older installations
    /// did not have a manifest; establish a one-time baseline for those files
    /// instead of deleting a working model during upgrade.
    fn verify_existing_model_directory(
        path: &Path,
        expected_sha256: Option<&str>,
        model_id: &str,
    ) -> Result<()> {
        let Some(expected_sha256) = expected_sha256 else {
            return Ok(());
        };
        let manifest_path = path.join(DIRECTORY_INTEGRITY_MANIFEST);

        if !manifest_path.exists() {
            warn!(
                "Built-in directory model {} predates integrity manifests; establishing a local baseline",
                model_id
            );
            return Self::write_directory_integrity_manifest(path, model_id, Some(expected_sha256));
        }

        let manifest = fs::read(&manifest_path)
            .map_err(|error| anyhow::anyhow!("Read model integrity manifest: {error}"))
            .and_then(|contents| {
                serde_json::from_slice::<DirectoryIntegrityManifest>(&contents)
                    .map_err(|error| anyhow::anyhow!("Parse model integrity manifest: {error}"))
            })?;

        let manifest_matches_model = manifest.schema_version == 1
            && manifest.model_id == model_id
            && manifest
                .archive_sha256
                .eq_ignore_ascii_case(expected_sha256);
        if !manifest_matches_model {
            let _ = fs::remove_dir_all(path);
            return Err(anyhow::anyhow!(
                "Model verification failed for {}: the integrity manifest does not match this model. Please download it again.",
                model_id
            ));
        }

        let files = Self::directory_file_hashes(path)?;
        if files == manifest.files {
            return Ok(());
        }

        let _ = fs::remove_dir_all(path);
        Err(anyhow::anyhow!(
            "Model verification failed for {}: the extracted model files are corrupt. Please download it again.",
            model_id
        ))
    }

    fn verify_model_before_use(&self, model_info: &ModelInfo, model_path: &Path) -> Result<()> {
        if model_info.is_custom || model_info.sha256.is_none() {
            return Ok(());
        }

        {
            let verified = self.verified_model_ids.lock().unwrap();
            if verified.contains(&model_info.id) {
                return Ok(());
            }
        }

        let verification = if model_info.is_directory {
            Self::verify_existing_model_directory(
                model_path,
                model_info.sha256.as_deref(),
                &model_info.id,
            )
        } else {
            Self::verify_existing_model_file(
                model_path,
                model_info.sha256.as_deref(),
                &model_info.id,
            )
        };

        match verification {
            Ok(()) => {
                self.verified_model_ids
                    .lock()
                    .unwrap()
                    .insert(model_info.id.clone());
                Ok(())
            }
            Err(error) => {
                if let Some(model) = self
                    .available_models
                    .lock()
                    .unwrap()
                    .get_mut(&model_info.id)
                {
                    model.is_downloaded = false;
                }
                Err(error)
            }
        }
    }

    /// Computes the SHA256 hex digest of a file, reading in 64KB chunks to handle large models.
    fn compute_sha256(path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }

    pub async fn download_model(&self, model_id: &str) -> Result<()> {
        let model_info = {
            let models = self.available_models.lock().unwrap();
            models.get(model_id).cloned()
        };

        let model_info =
            model_info.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        let url = model_info
            .url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No download URL for model"))?;
        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));

        // Don't download if complete version already exists
        if model_path.exists() {
            // Clean up any partial file that might exist
            if partial_path.exists() {
                let _ = fs::remove_file(&partial_path);
            }
            if model_info.is_directory {
                match self.verify_model_before_use(&model_info, &model_path) {
                    Ok(()) => {
                        self.update_download_status()?;
                        return Ok(());
                    }
                    Err(error) => {
                        // A manifest mismatch removes the corrupted extracted
                        // directory. Continue into a fresh download instead of
                        // reporting a misleading success or requiring a second
                        // click from the user.
                        warn!(
                            "Existing directory model {} failed verification; downloading a clean replacement: {}",
                            model_id, error
                        );
                    }
                }
            } else {
                match Self::verify_existing_model_file(
                    &model_path,
                    model_info.sha256.as_deref(),
                    model_id,
                ) {
                    Ok(()) => {
                        if !model_info.is_custom && model_info.sha256.is_some() {
                            self.verified_model_ids
                                .lock()
                                .unwrap()
                                .insert(model_id.to_string());
                        }
                        self.update_download_status()?;
                        return Ok(());
                    }
                    Err(error) => {
                        // The verifier removes the corrupt completed file. Continue
                        // into a fresh download instead of reporting a misleading
                        // success or requiring the user to click Download twice.
                        warn!(
                            "Existing model {} failed verification; downloading a clean replacement: {}",
                            model_id, error
                        );
                    }
                }
            }
        }

        // Check if we have a partial download to resume
        let mut resume_from = if partial_path.exists() {
            let size = partial_path.metadata()?.len();
            info!("Resuming download of model {} from byte {}", model_id, size);
            size
        } else {
            info!("Starting fresh download of model {} from {}", model_id, url);
            0
        };

        // Mark as downloading
        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(model_id) {
                model.is_downloading = true;
            }
        }

        // Create cancellation flag for this download
        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut flags = self.cancel_flags.lock().unwrap();
            flags.insert(model_id.to_string(), cancel_flag.clone());
        }

        // Guard ensures is_downloading and cancel_flags are cleaned up on every
        // error path. Disarmed only on success (which sets is_downloaded = true).
        let mut cleanup = DownloadCleanup {
            available_models: &self.available_models,
            cancel_flags: &self.cancel_flags,
            model_id: model_id.to_string(),
            disarmed: false,
        };

        // Create HTTP client with range request for resuming
        let downloader = crate::download::DownloadClient::default();
        let deadline = Instant::now() + downloader.total_timeout();
        let mut request = downloader.get(&url);

        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }

        let mut response = downloader.send(request, &cancel_flag).await?;

        // If we tried to resume but server returned 200 (not 206 Partial Content),
        // the server doesn't support range requests. Delete partial file and restart
        // fresh to avoid file corruption (appending full file to partial).
        if resume_from > 0 && response.status() == reqwest::StatusCode::OK {
            warn!(
                "Server doesn't support range requests for model {}, restarting download",
                model_id
            );
            drop(response);
            let _ = fs::remove_file(&partial_path);

            // Reset resume_from since we're starting fresh
            resume_from = 0;

            // Restart download without range header
            response = downloader.send(downloader.get(&url), &cancel_flag).await?;
        }

        // Check for success or partial content status
        if !response.status().is_success()
            && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
        {
            return Err(anyhow::anyhow!(
                "Failed to download model: HTTP {}",
                response.status()
            ));
        }

        let total_size = if resume_from > 0 {
            // For resumed downloads, add the resume point to content length
            resume_from + response.content_length().unwrap_or(0)
        } else {
            response.content_length().unwrap_or(0)
        };

        let mut downloaded = resume_from;

        // Open file for appending if resuming, or create new if starting fresh
        let mut file = if resume_from > 0 {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&partial_path)?
        } else {
            std::fs::File::create(&partial_path)?
        };

        // Emit initial progress
        let initial_progress = DownloadProgress {
            model_id: model_id.to_string(),
            downloaded,
            total: total_size,
            percentage: if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                0.0
            },
        };
        let _ = self
            .app_handle
            .emit("model-download-progress", &initial_progress);

        // Throttle progress events to max 10/sec (100ms intervals)
        let mut last_emit = Instant::now();
        let throttle_duration = Duration::from_millis(100);

        // Download with progress
        while let Some(chunk) = downloader
            .next_chunk(&mut response, &cancel_flag, deadline)
            .await?
        {
            file.write_all(&chunk)?;
            downloaded += chunk.len() as u64;

            let percentage = if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                0.0
            };

            // Emit progress event (throttled to avoid UI freeze)
            if last_emit.elapsed() >= throttle_duration {
                let progress = DownloadProgress {
                    model_id: model_id.to_string(),
                    downloaded,
                    total: total_size,
                    percentage,
                };
                let _ = self.app_handle.emit("model-download-progress", &progress);
                last_emit = Instant::now();
            }
        }

        // Emit final progress to ensure 100% is shown
        let final_progress = DownloadProgress {
            model_id: model_id.to_string(),
            downloaded,
            total: total_size,
            percentage: if total_size > 0 {
                (downloaded as f64 / total_size as f64) * 100.0
            } else {
                100.0
            },
        };
        let _ = self
            .app_handle
            .emit("model-download-progress", &final_progress);

        file.flush()?;
        drop(file); // Ensure file is closed before moving

        // Verify downloaded file size matches expected size
        if total_size > 0 {
            let actual_size = partial_path.metadata()?.len();
            if actual_size != total_size {
                // Download is incomplete/corrupted - delete partial and return error
                let _ = fs::remove_file(&partial_path);
                return Err(anyhow::anyhow!(
                    "Download incomplete: expected {} bytes, got {} bytes",
                    total_size,
                    actual_size
                ));
            }
        }

        // Verify SHA256 checksum. Runs in a blocking thread so the async executor is not
        // stalled while hashing large model files (up to 1.6 GB). On failure the partial
        // is deleted inside verify_sha256 so the next attempt always starts fresh.
        let _ = self.app_handle.emit("model-verification-started", model_id);
        info!("Verifying SHA256 for model {}...", model_id);
        let verify_path = partial_path.clone();
        let verify_expected = model_info.sha256.clone();
        let verify_model_id = model_id.to_string();
        let verify_result = tokio::task::spawn_blocking(move || {
            Self::verify_sha256(&verify_path, verify_expected.as_deref(), &verify_model_id)
        })
        .await
        .map_err(|e| anyhow::anyhow!("SHA256 task panicked: {}", e))?;
        verify_result?;
        let _ = self
            .app_handle
            .emit("model-verification-completed", model_id);

        // Handle directory-based models (extract tar.gz) vs file-based models
        if model_info.is_directory {
            // Track that this model is being extracted
            {
                let mut extracting = self.extracting_models.lock().unwrap();
                extracting.insert(model_id.to_string());
            }

            // Emit extraction started event
            let _ = self.app_handle.emit("model-extraction-started", model_id);
            info!("Extracting archive for directory-based model: {}", model_id);

            // Use a temporary extraction directory to ensure atomic operations
            let temp_extract_dir = self
                .models_dir
                .join(format!("{}.extracting", &model_info.filename));
            let final_model_dir = self.models_dir.join(&model_info.filename);

            let extraction_result = (|| -> Result<()> {
                // Clean up any previous incomplete extraction.
                if temp_extract_dir.exists() {
                    fs::remove_dir_all(&temp_extract_dir)?;
                }
                fs::create_dir_all(&temp_extract_dir)?;

                // Extract only into the temporary directory. The archive has
                // already passed its SHA256 check, so any later failure can
                // safely discard it and require a clean replacement.
                let tar_gz = File::open(&partial_path)?;
                let tar = GzDecoder::new(tar_gz);
                let mut archive = Archive::new(tar);
                archive.unpack(&temp_extract_dir)?;

                // Archives usually contain one top-level model directory, but
                // retain support for archives with files at their root.
                let extracted_dirs: Vec<_> = fs::read_dir(&temp_extract_dir)?
                    .filter_map(|entry| entry.ok())
                    .filter(|entry| entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                    .collect();
                let source_dir = if extracted_dirs.len() == 1 {
                    extracted_dirs[0].path()
                } else {
                    temp_extract_dir.clone()
                };

                // Write the manifest before replacing an existing working
                // model. A directory only becomes selectable after both its
                // verified archive and extracted-file inventory are complete.
                Self::write_directory_integrity_manifest(
                    &source_dir,
                    model_id,
                    model_info.sha256.as_deref(),
                )?;

                if final_model_dir.exists() {
                    fs::remove_dir_all(&final_model_dir)?;
                }
                fs::rename(&source_dir, &final_model_dir)?;
                if source_dir != temp_extract_dir {
                    fs::remove_dir_all(&temp_extract_dir)?;
                }
                Ok(())
            })();

            if let Err(error) = extraction_result {
                let error_msg = format!("Failed to extract archive: {error}");
                let _ = fs::remove_dir_all(&temp_extract_dir);
                // Delete the archive so a retry never resumes from an asset
                // that failed to extract or inventory successfully.
                let _ = fs::remove_file(&partial_path);
                {
                    let mut extracting = self.extracting_models.lock().unwrap();
                    extracting.remove(model_id);
                }
                let _ = self.app_handle.emit(
                    "model-extraction-failed",
                    &serde_json::json!({
                        "model_id": model_id,
                        "error": error_msg
                    }),
                );
                return Err(anyhow::anyhow!(error_msg));
            }

            info!("Successfully extracted archive for model: {}", model_id);
            // Remove from extracting set
            {
                let mut extracting = self.extracting_models.lock().unwrap();
                extracting.remove(model_id);
            }
            // Emit extraction completed event
            let _ = self.app_handle.emit("model-extraction-completed", model_id);

            // Remove the downloaded tar.gz file
            let _ = fs::remove_file(&partial_path);
        } else {
            // Move partial file to final location for file-based models
            fs::rename(&partial_path, &model_path)?;
        }

        // Disarm the guard — success path does its own cleanup because it
        // additionally sets is_downloaded = true.
        cleanup.disarmed = true;
        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(model_id) {
                model.is_downloading = false;
                model.is_downloaded = true;
                model.partial_size = 0;
            }
        }
        if !model_info.is_directory && !model_info.is_custom && model_info.sha256.is_some() {
            self.verified_model_ids
                .lock()
                .unwrap()
                .insert(model_id.to_string());
        }
        self.cancel_flags.lock().unwrap().remove(model_id);

        // Emit completion event
        let _ = self.app_handle.emit("model-download-complete", model_id);

        info!(
            "Successfully downloaded model {} to {:?}",
            model_id, model_path
        );

        Ok(())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<()> {
        debug!("ModelManager: delete_model called for: {}", model_id);

        let model_info = {
            let models = self.available_models.lock().unwrap();
            models.get(model_id).cloned()
        };

        let model_info =
            model_info.ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        debug!("ModelManager: Found model info: {:?}", model_info);

        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));
        debug!("ModelManager: Model path: {:?}", model_path);
        debug!("ModelManager: Partial path: {:?}", partial_path);

        let mut deleted_something = false;

        if model_info.is_directory {
            // Delete complete model directory if it exists
            if model_path.exists() && model_path.is_dir() {
                info!("Deleting model directory at: {:?}", model_path);
                fs::remove_dir_all(&model_path)?;
                info!("Model directory deleted successfully");
                deleted_something = true;
            }
        } else {
            // Delete complete model file if it exists
            if model_path.exists() {
                info!("Deleting model file at: {:?}", model_path);
                fs::remove_file(&model_path)?;
                info!("Model file deleted successfully");
                deleted_something = true;
            }
        }

        // Delete partial file if it exists (same for both types)
        if partial_path.exists() {
            info!("Deleting partial file at: {:?}", partial_path);
            fs::remove_file(&partial_path)?;
            info!("Partial file deleted successfully");
            deleted_something = true;
        }

        if !deleted_something {
            return Err(anyhow::anyhow!("No model files found to delete"));
        }

        // Custom models should be removed from the list entirely since they
        // have no download URL and can't be re-downloaded
        if model_info.is_custom {
            let mut models = self.available_models.lock().unwrap();
            models.remove(model_id);
            debug!("ModelManager: removed custom model from available models");
        } else {
            // Update download status (marks predefined models as not downloaded)
            self.update_download_status()?;
            debug!("ModelManager: download status updated");
        }
        self.verified_model_ids.lock().unwrap().remove(model_id);

        // Emit event to notify UI
        let _ = self.app_handle.emit("model-deleted", model_id);

        Ok(())
    }

    #[cfg_attr(not(feature = "transcribe-rs-engine"), allow(dead_code))]
    pub fn get_model_path(&self, model_id: &str) -> Result<PathBuf> {
        let model_info = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;

        if !model_info.is_downloaded {
            return Err(anyhow::anyhow!("Model not available: {}", model_id));
        }

        // Ensure we don't return partial files/directories
        if model_info.is_downloading {
            return Err(anyhow::anyhow!(
                "Model is currently downloading: {}",
                model_id
            ));
        }

        let model_path = self.models_dir.join(&model_info.filename);
        let partial_path = self
            .models_dir
            .join(format!("{}.partial", &model_info.filename));

        if model_info.is_directory {
            // For directory-based models, ensure the directory exists and is complete
            if model_path.exists() && model_path.is_dir() && !partial_path.exists() {
                debug!("Verifying directory model {} before inference", model_id);
                self.verify_model_before_use(&model_info, &model_path)?;
                debug!("Verified directory model {} before inference", model_id);
                Ok(model_path)
            } else {
                Err(anyhow::anyhow!(
                    "Complete model directory not found: {}",
                    model_id
                ))
            }
        } else {
            // For file-based models (existing logic)
            if model_path.exists() && !partial_path.exists() {
                self.verify_model_before_use(&model_info, &model_path)?;
                Ok(model_path)
            } else {
                Err(anyhow::anyhow!(
                    "Complete model file not found: {}",
                    model_id
                ))
            }
        }
    }

    #[cfg_attr(not(feature = "transcribe-rs-engine"), allow(dead_code))]
    pub fn get_model_asset(&self, model_id: &str) -> Result<crate::providers::ModelAsset> {
        let model_info = self
            .get_model_info(model_id)
            .ok_or_else(|| anyhow::anyhow!("Model not found: {}", model_id))?;
        let model_path = self.get_model_path(model_id)?;

        Ok(crate::providers::ModelAsset::from_model_info(
            model_info, model_path,
        ))
    }

    pub fn cancel_download(&self, model_id: &str) -> Result<()> {
        debug!("ModelManager: cancel_download called for: {}", model_id);

        // Set the cancellation flag to stop the download loop
        {
            let flags = self.cancel_flags.lock().unwrap();
            if let Some(flag) = flags.get(model_id) {
                flag.store(true, Ordering::Relaxed);
                info!("Cancellation flag set for: {}", model_id);
            } else {
                warn!("No active download found for: {}", model_id);
            }
        }

        // Update state immediately for UI responsiveness
        {
            let mut models = self.available_models.lock().unwrap();
            if let Some(model) = models.get_mut(model_id) {
                model.is_downloading = false;
            }
        }

        // Update download status to reflect current state
        self.update_download_status()?;

        // Emit cancellation event so all UI components can clear their state
        let _ = self.app_handle.emit("model-download-cancelled", model_id);

        info!("Download cancellation initiated for: {}", model_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_discover_custom_whisper_models() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().to_path_buf();

        // Create test .bin files
        let mut custom_file = File::create(models_dir.join("my-custom-model.bin")).unwrap();
        custom_file.write_all(b"fake model data").unwrap();

        let mut another_file = File::create(models_dir.join("whisper_medical_v2.bin")).unwrap();
        another_file.write_all(b"another fake model").unwrap();

        // Create files that should be ignored
        File::create(models_dir.join(".hidden-model.bin")).unwrap(); // Hidden file
        File::create(models_dir.join("readme.txt")).unwrap(); // Non-.bin file
        File::create(models_dir.join("ggml-small.bin")).unwrap(); // Predefined filename
        fs::create_dir(models_dir.join("some-directory.bin")).unwrap(); // Directory

        // Set up available_models with a predefined Whisper model
        let mut models = HashMap::new();
        models.insert(
            "small".to_string(),
            ModelInfo {
                id: "small".to_string(),
                name: "Whisper Small".to_string(),
                description: "Test".to_string(),
                filename: "ggml-small.bin".to_string(),
                url: Some("https://example.com".to_string()),
                sha256: None,
                size_mb: 100,
                is_downloaded: false,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                license_label: "MIT".to_string(),
                accelerator_support: vec!["whisper-cpp".to_string()],
                accuracy_score: 0.5,
                speed_score: 0.5,
                supports_translation: true,
                is_recommended: false,
                supported_languages: vec!["en".to_string()],
                supports_language_selection: true,
                is_custom: false,
            },
        );

        // Discover custom models
        ModelManager::discover_custom_whisper_models(&models_dir, &mut models).unwrap();

        // Should have discovered 2 custom models (my-custom-model and whisper_medical_v2)
        assert!(models.contains_key("my-custom-model"));
        assert!(models.contains_key("whisper_medical_v2"));

        // Verify custom model properties
        let custom = models.get("my-custom-model").unwrap();
        assert_eq!(custom.name, "My Custom Model");
        assert_eq!(custom.filename, "my-custom-model.bin");
        assert!(custom.url.is_none()); // Custom models have no URL
        assert!(custom.is_downloaded);
        assert!(custom.is_custom);
        assert_eq!(custom.accuracy_score, 0.0);
        assert_eq!(custom.speed_score, 0.0);
        assert!(custom.supported_languages.is_empty());

        // Verify underscore handling
        let medical = models.get("whisper_medical_v2").unwrap();
        assert_eq!(medical.name, "Whisper Medical V2");

        // Should NOT have discovered hidden, non-.bin, predefined, or directories
        assert!(!models.contains_key(".hidden-model"));
        assert!(!models.contains_key("readme"));
        assert!(!models.contains_key("some-directory"));
    }

    #[test]
    fn test_discover_custom_models_empty_dir() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().to_path_buf();

        let mut models = HashMap::new();
        let count_before = models.len();

        ModelManager::discover_custom_whisper_models(&models_dir, &mut models).unwrap();

        // No new models should be added
        assert_eq!(models.len(), count_before);
    }

    #[test]
    fn test_discover_custom_models_nonexistent_dir() {
        let models_dir = PathBuf::from("/nonexistent/path/that/does/not/exist");

        let mut models = HashMap::new();
        let count_before = models.len();

        // Should not error, just return Ok
        let result = ModelManager::discover_custom_whisper_models(&models_dir, &mut models);
        assert!(result.is_ok());
        assert_eq!(models.len(), count_before);
    }

    #[test]
    fn test_native_smoke_model_fixture_is_discovered_as_local_custom_model() {
        let temp_dir = TempDir::new().unwrap();
        let models_dir = temp_dir.path().to_path_buf();

        let fixture_path = ModelManager::write_native_smoke_model_fixture(&models_dir).unwrap();
        assert_eq!(
            fixture_path.file_name().unwrap(),
            "verbatim-smoke-model.bin"
        );

        let fixture_contents = fs::read_to_string(&fixture_path).unwrap();
        assert!(fixture_contents.contains("must never be used for inference"));

        let mut models = HashMap::new();
        ModelManager::discover_custom_whisper_models(&models_dir, &mut models).unwrap();

        let smoke_model = models.get("verbatim-smoke-model").unwrap();
        assert_eq!(smoke_model.name, "Verbatim Smoke Model");
        assert_eq!(smoke_model.filename, "verbatim-smoke-model.bin");
        assert!(smoke_model.is_downloaded);
        assert!(smoke_model.is_custom);
        assert!(smoke_model.url.is_none());
    }

    // ── SHA256 verification tests ─────────────────────────────────────────────

    /// Helper: write `data` to a temp file and return (TempDir, path).
    /// TempDir must be kept alive for the duration of the test.
    fn write_temp_file(data: &[u8]) -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("model.partial");
        let mut f = File::create(&path).unwrap();
        f.write_all(data).unwrap();
        (dir, path)
    }

    #[test]
    fn test_verify_sha256_skipped_when_none() {
        // Custom models have no expected hash — verification must be a no-op.
        let (_dir, path) = write_temp_file(b"anything");
        assert!(ModelManager::verify_sha256(&path, None, "custom").is_ok());
        assert!(
            path.exists(),
            "file must be untouched when verification is skipped"
        );
    }

    #[test]
    fn test_verify_sha256_passes_on_correct_hash() {
        // Compute the real hash so the test is self-consistent.
        let (_dir, path) = write_temp_file(b"hello world");
        let actual = ModelManager::compute_sha256(&path).unwrap();
        assert!(
            ModelManager::verify_sha256(&path, Some(&actual), "test_model").is_ok(),
            "should pass when hash matches"
        );
        assert!(
            path.exists(),
            "file must be kept on successful verification"
        );
    }

    #[test]
    fn test_verify_sha256_fails_and_deletes_partial_on_mismatch() {
        let (_dir, path) = write_temp_file(b"this is not the real model");
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = ModelManager::verify_sha256(&path, Some(wrong_hash), "bad_model");

        assert!(result.is_err(), "mismatch must return an error");
        assert!(
            result.unwrap_err().to_string().contains("corrupt"),
            "error message should mention corruption"
        );
        assert!(
            !path.exists(),
            "partial file must be deleted after hash mismatch"
        );
    }

    #[test]
    fn test_verify_sha256_fails_and_deletes_partial_when_file_missing() {
        // Simulate a partial file that was already removed (e.g. disk full mid-download).
        let dir = TempDir::new().unwrap();
        let missing_path = dir.path().join("gone.partial");
        // Don't create the file — it should not exist.

        let result =
            ModelManager::verify_sha256(&missing_path, Some("anyexpectedhash"), "missing_model");

        assert!(result.is_err(), "missing file must return an error");
    }

    #[test]
    fn complete_builtin_model_with_wrong_hash_is_removed_before_inference() {
        let (dir, path) = write_temp_file(b"corrupt model data");
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";

        let result = ModelManager::verify_existing_model_file(&path, Some(wrong_hash), "tiny");

        assert!(result.is_err());
        assert!(
            !path.exists(),
            "a corrupt completed model must not remain selectable"
        );
        drop(dir);
    }

    #[test]
    fn complete_builtin_model_with_matching_hash_is_preserved() {
        let (dir, path) = write_temp_file(b"known good model data");
        let expected = ModelManager::compute_sha256(&path).unwrap();

        ModelManager::verify_existing_model_file(&path, Some(&expected), "tiny").unwrap();

        assert!(path.exists());
        drop(dir);
    }

    #[test]
    fn legacy_directory_model_gets_an_integrity_baseline() {
        let dir = TempDir::new().unwrap();
        let model_dir = dir.path().join("moonshine");
        fs::create_dir_all(model_dir.join("nested")).unwrap();
        fs::write(model_dir.join("encoder.ort"), b"known model data").unwrap();
        fs::write(model_dir.join("nested").join("tokens.txt"), b"vocabulary").unwrap();

        ModelManager::verify_existing_model_directory(
            &model_dir,
            Some("verified-archive-sha"),
            "moonshine",
        )
        .unwrap();

        assert!(
            model_dir.join(DIRECTORY_INTEGRITY_MANIFEST).exists(),
            "a legacy directory must be baselined rather than deleted during upgrade"
        );
        ModelManager::verify_existing_model_directory(
            &model_dir,
            Some("verified-archive-sha"),
            "moonshine",
        )
        .unwrap();
    }

    #[test]
    fn directory_model_mutation_is_removed_before_inference() {
        let dir = TempDir::new().unwrap();
        let model_dir = dir.path().join("moonshine");
        fs::create_dir_all(&model_dir).unwrap();
        let model_file = model_dir.join("encoder.ort");
        fs::write(&model_file, b"known model data").unwrap();
        ModelManager::write_directory_integrity_manifest(
            &model_dir,
            "moonshine",
            Some("verified-archive-sha"),
        )
        .unwrap();

        fs::write(&model_file, b"corrupted after extraction").unwrap();
        let error = ModelManager::verify_existing_model_directory(
            &model_dir,
            Some("verified-archive-sha"),
            "moonshine",
        )
        .unwrap_err();

        assert!(error.to_string().contains("corrupt"));
        assert!(
            !model_dir.exists(),
            "a changed built-in directory model must not remain selectable"
        );
    }
}
