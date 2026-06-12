// No-engine TranscriptionManager - avoids whisper/Vulkan dependencies in
// fast test builds while failing loudly for engine-dependent operations.

use crate::managers::model::ModelManager;
use anyhow::Result;
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::AppHandle;

const ENGINE_DISABLED_ERROR: &str = "transcribe-rs engine is disabled in this build";

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
}

/// RAII guard that is a no-op in the mock — mirrors the real `LoadingGuard`.
pub struct LoadingGuard;

#[derive(Clone)]
pub struct TranscriptionManager {
    #[allow(dead_code)]
    app_handle: AppHandle,
}

impl TranscriptionManager {
    pub fn new(app_handle: &AppHandle, _model_manager: Arc<ModelManager>) -> Result<Self> {
        Ok(Self {
            app_handle: app_handle.clone(),
        })
    }

    pub fn is_model_loaded(&self) -> bool {
        false
    }

    pub fn try_start_loading(&self) -> Option<LoadingGuard> {
        Some(LoadingGuard)
    }

    pub fn unload_model(&self) -> Result<()> {
        Ok(())
    }

    pub fn maybe_unload_immediately(&self, _context: &str) {}

    pub fn load_model(&self, _model_id: &str) -> Result<()> {
        Err(anyhow::anyhow!(ENGINE_DISABLED_ERROR))
    }

    pub fn initiate_model_load(&self) {}

    pub fn get_current_model(&self) -> Option<String> {
        None
    }

    pub fn transcribe(&self, _audio: Vec<f32>) -> Result<String> {
        Err(anyhow::anyhow!(ENGINE_DISABLED_ERROR))
    }
}

/// No-op in CI mock.
pub fn apply_accelerator_settings(_app: &tauri::AppHandle) {}

#[derive(Serialize, Clone, Debug, Type)]
pub struct GpuDeviceOption {
    pub id: i32,
    pub name: String,
    pub total_vram_mb: usize,
}

#[derive(Serialize, Clone, Debug, Type)]
pub struct AvailableAccelerators {
    pub whisper: Vec<String>,
    pub ort: Vec<String>,
    pub gpu_devices: Vec<GpuDeviceOption>,
}

/// Returns empty lists in CI mock.
pub fn get_available_accelerators() -> AvailableAccelerators {
    AvailableAccelerators {
        whisper: vec![],
        ort: vec![],
        gpu_devices: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::ENGINE_DISABLED_ERROR;
    use crate::providers::{
        EngineProvider, ModelAsset, ModelLocator, SpeechInput, SpeechRequest, SpeechTaskKind,
        TranscribeRsProvider,
    };
    use crate::{
        managers::model::{EngineType, ModelInfo},
        providers::{CancellationToken, LanguageSelection},
    };
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_asset() -> ModelAsset {
        ModelAsset {
            id: "test-model".to_string(),
            locator: ModelLocator::File(PathBuf::from("test.bin")),
            metadata: ModelInfo {
                id: "test-model".to_string(),
                name: "Test Model".to_string(),
                description: "Test".to_string(),
                filename: "test.bin".to_string(),
                url: None,
                sha256: None,
                size_mb: 1,
                is_downloaded: true,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type: EngineType::Whisper,
                accuracy_score: 0.0,
                speed_score: 0.0,
                supports_translation: false,
                is_recommended: false,
                supported_languages: vec![],
                supports_language_selection: true,
                is_custom: false,
            },
        }
    }

    fn test_request() -> SpeechRequest {
        SpeechRequest {
            task: SpeechTaskKind::Transcribe,
            input: SpeechInput::Audio(Arc::from([0.0_f32, 1.0_f32])),
            source_language: LanguageSelection::Auto,
            translation: None,
            language_shortlist: vec![],
            custom_words: vec![],
            cancellation: CancellationToken::default(),
        }
    }

    #[test]
    fn no_engine_provider_fails_loudly_when_loading() {
        let mut provider = TranscribeRsProvider::new();

        let error = provider
            .load(&test_asset())
            .expect_err("no-engine provider must not load models");

        assert!(error.to_string().contains(ENGINE_DISABLED_ERROR));
    }

    #[test]
    fn no_engine_provider_fails_loudly_when_transcribing() {
        let mut provider = TranscribeRsProvider::new();

        let error = provider
            .run(test_request())
            .expect_err("no-engine provider must not transcribe");

        assert!(error.to_string().contains(ENGINE_DISABLED_ERROR));
    }
}
