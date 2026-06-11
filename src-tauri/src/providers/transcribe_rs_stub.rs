use crate::managers::model::EngineType;
use crate::providers::{
    EngineProvider, LifecycleCost, ModelAsset, ProviderCapabilities, SpeechRequest, SpeechResponse,
    SpeechTaskKind, StreamingSupport, TranslationPairSupport,
};
use anyhow::Result;

pub struct TranscribeRsProvider;

impl TranscribeRsProvider {
    pub fn new() -> Self {
        Self
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

    fn load(&mut self, _asset: &ModelAsset) -> Result<()> {
        Err(anyhow::anyhow!(
            "transcribe-rs engine is disabled in this build"
        ))
    }

    fn unload(&mut self) {}

    fn run(&mut self, _request: SpeechRequest) -> Result<SpeechResponse> {
        Err(anyhow::anyhow!(
            "transcribe-rs engine is disabled in this build"
        ))
    }
}
