use crate::managers::model::EngineType;
use crate::providers::{
    EngineProvider, LifecycleCost, ModelAsset, ModelLocator, ProviderCapabilities, SpeechRequest,
    SpeechResponse, SpeechTaskKind, StreamingSupport, TranslationPairSupport,
};
use anyhow::Result;
use ::transcribe_rs::{
    onnx::{
        canary::CanaryModel,
        cohere::CohereModel,
        gigaam::GigaAMModel,
        moonshine::{MoonshineModel, MoonshineVariant, StreamingModel},
        parakeet::ParakeetModel,
        sense_voice::SenseVoiceModel,
        Quantization,
    },
    whisper_cpp::WhisperEngine,
};

pub enum LoadedEngine {
    Whisper(WhisperEngine),
    Parakeet(ParakeetModel),
    Moonshine(MoonshineModel),
    MoonshineStreaming(StreamingModel),
    SenseVoice(SenseVoiceModel),
    GigaAM(GigaAMModel),
    Canary(CanaryModel),
    Cohere(CohereModel),
}

pub struct TranscribeRsProvider {
    engine: Option<LoadedEngine>,
    model_id: Option<String>,
}

impl TranscribeRsProvider {
    pub fn new() -> Self {
        Self {
            engine: None,
            model_id: None,
        }
    }
}

impl Default for TranscribeRsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineProvider for TranscribeRsProvider {
    fn provider_id(&self) -> &'static str {
        "transcribe_rs"
    }

    fn capabilities(&self, asset: &ModelAsset) -> ProviderCapabilities {
        ProviderCapabilities {
            tasks: vec![SpeechTaskKind::Transcribe],
            translation_pairs: if asset.metadata.supports_translation {
                TranslationPairSupport::EnglishOnly
            } else {
                TranslationPairSupport::None
            },
            streaming: match asset.metadata.engine_type {
                EngineType::MoonshineStreaming => StreamingSupport::PartialText,
                _ => StreamingSupport::None,
            },
            lifecycle: LifecycleCost::Expensive,
        }
    }

    fn load(&mut self, asset: &ModelAsset) -> Result<()> {
        let model_path = match &asset.locator {
            ModelLocator::File(path) | ModelLocator::Directory(path) => path,
            ModelLocator::ManagedServer { .. } | ModelLocator::ExternalHttp { .. } => {
                return Err(anyhow::anyhow!(
                    "transcribe_rs provider requires a local file or directory model asset"
                ));
            }
        };

        let loaded = match asset.metadata.engine_type {
            EngineType::Whisper => LoadedEngine::Whisper(WhisperEngine::load(model_path)?),
            EngineType::Parakeet => {
                LoadedEngine::Parakeet(ParakeetModel::load(model_path, &Quantization::Int8)?)
            }
            EngineType::Moonshine => LoadedEngine::Moonshine(MoonshineModel::load(
                model_path,
                MoonshineVariant::Base,
                &Quantization::default(),
            )?),
            EngineType::MoonshineStreaming => LoadedEngine::MoonshineStreaming(
                StreamingModel::load(model_path, 0, &Quantization::default())?,
            ),
            EngineType::SenseVoice => {
                LoadedEngine::SenseVoice(SenseVoiceModel::load(model_path, &Quantization::Int8)?)
            }
            EngineType::GigaAM => {
                LoadedEngine::GigaAM(GigaAMModel::load(model_path, &Quantization::Int8)?)
            }
            EngineType::Canary => {
                LoadedEngine::Canary(CanaryModel::load(model_path, &Quantization::Int8)?)
            }
            EngineType::Cohere => {
                LoadedEngine::Cohere(CohereModel::load(model_path, &Quantization::Int8)?)
            }
        };

        self.engine = Some(loaded);
        self.model_id = Some(asset.id.clone());
        Ok(())
    }

    fn unload(&mut self) {
        self.engine = None;
        self.model_id = None;
    }

    fn run(&mut self, _request: SpeechRequest) -> Result<SpeechResponse> {
        Err(anyhow::anyhow!(
            "transcribe_rs provider run is not wired yet"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::model::{EngineType, ModelInfo};
    use crate::providers::{ModelAsset, SpeechTaskKind, TranslationPairSupport};
    use std::path::PathBuf;

    fn asset(engine_type: EngineType, supports_translation: bool) -> ModelAsset {
        ModelAsset::from_model_info(
            ModelInfo {
                id: "test".to_string(),
                name: "Test".to_string(),
                description: "Test".to_string(),
                filename: "test.bin".to_string(),
                url: None,
                sha256: None,
                size_mb: 1,
                is_downloaded: true,
                is_downloading: false,
                partial_size: 0,
                is_directory: false,
                engine_type,
                accuracy_score: 0.0,
                speed_score: 0.0,
                supports_translation,
                is_recommended: false,
                supported_languages: vec!["en".to_string(), "ar".to_string()],
                supports_language_selection: true,
                is_custom: false,
            },
            PathBuf::from("test.bin"),
        )
    }

    #[test]
    fn transcribe_rs_provider_supports_transcription_for_current_engines() {
        let provider = TranscribeRsProvider::new();
        let capabilities = provider.capabilities(&asset(EngineType::Whisper, false));
        assert!(capabilities.supports_task(&SpeechTaskKind::Transcribe));
    }

    #[test]
    fn transcribe_rs_provider_reports_english_only_translation_when_model_supports_translation() {
        let provider = TranscribeRsProvider::new();
        let capabilities = provider.capabilities(&asset(EngineType::Whisper, true));
        assert_eq!(
            capabilities.translation_pairs,
            TranslationPairSupport::EnglishOnly
        );
    }
}
