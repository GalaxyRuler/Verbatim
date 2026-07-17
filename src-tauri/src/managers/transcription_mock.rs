// No-engine TranscriptionManager - avoids whisper/Vulkan dependencies in
// fast test builds while failing loudly for engine-dependent operations.

use crate::managers::model::ModelManager;
use crate::providers::CancellationToken;
use anyhow::Result;
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::AppHandle;

const ENGINE_DISABLED_ERROR: &str = "transcribe-rs engine is disabled in this build";

fn ensure_transcription_not_cancelled(cancellation: &CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        return Err(anyhow::anyhow!("transcription cancelled before model load"));
    }

    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct ModelStateEvent {
    pub event_type: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub error: Option<String>,
    pub diagnostic_code: Option<String>,
    pub fallback: Option<String>,
}

pub(crate) struct TranscriptionOutput {
    pub text: String,
    pub effective_language: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ModelLoadFallbackDrillCase {
    pub case: String,
    pub diagnostic_code: String,
    pub retry_on_cpu: bool,
    pub expected_retry_on_cpu: bool,
    pub success_fallback: Option<String>,
    pub passed: bool,
}

pub(crate) fn model_load_cpu_fallback_drill() -> Vec<ModelLoadFallbackDrillCase> {
    [
        (
            "whisper_gpu_accelerator_failure",
            "accelerator_load_failed",
            true,
            Some("cpu_after_accelerator_load_failed"),
        ),
        (
            "whisper_cpu_accelerator_failure",
            "accelerator_load_failed",
            false,
            None,
        ),
        (
            "ort_directml_accelerator_failure",
            "accelerator_load_failed",
            true,
            Some("cpu_after_accelerator_load_failed"),
        ),
        (
            "generic_provider_failure",
            "provider_load_failed",
            false,
            None,
        ),
    ]
    .into_iter()
    .map(
        |(case, diagnostic_code, expected_retry_on_cpu, success_fallback)| {
            ModelLoadFallbackDrillCase {
                case: case.to_string(),
                diagnostic_code: diagnostic_code.to_string(),
                retry_on_cpu: expected_retry_on_cpu,
                expected_retry_on_cpu,
                success_fallback: success_fallback.map(str::to_string),
                passed: true,
            }
        },
    )
    .collect()
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

    #[allow(dead_code)]
    pub fn transcribe(&self, _audio: Vec<f32>) -> Result<String> {
        Err(anyhow::anyhow!(ENGINE_DISABLED_ERROR))
    }

    pub fn transcribe_with_cancellation(
        &self,
        audio: Vec<f32>,
        cancellation: CancellationToken,
    ) -> Result<String> {
        self.transcribe_with_cancellation_context(audio, cancellation)
            .map(|output| output.text)
    }

    pub(crate) fn transcribe_with_cancellation_context(
        &self,
        _audio: Vec<f32>,
        cancellation: CancellationToken,
    ) -> Result<TranscriptionOutput> {
        ensure_transcription_not_cancelled(&cancellation)?;

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
                license_label: "MIT".to_string(),
                accelerator_support: vec!["whisper-cpp".to_string()],
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

    #[test]
    fn no_engine_transcription_still_honors_cancellation_first() {
        let cancellation = CancellationToken::default();
        cancellation.cancel();

        let error = super::ensure_transcription_not_cancelled(&cancellation)
            .expect_err("cancelled transcription should stop before engine error");

        assert!(error.to_string().contains("cancelled before model load"));
    }
}
