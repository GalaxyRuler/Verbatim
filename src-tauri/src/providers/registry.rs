use crate::providers::{ModelAsset, ProviderCapabilities, SpeechTaskKind};

pub trait ProviderDescriptor: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn capabilities(&self, asset: &ModelAsset) -> ProviderCapabilities;
}

pub struct ProviderRegistry {
    providers: Vec<Box<dyn ProviderDescriptor>>,
}

impl ProviderRegistry {
    pub fn new(providers: Vec<Box<dyn ProviderDescriptor>>) -> Self {
        Self { providers }
    }

    pub fn select(
        &self,
        asset: &ModelAsset,
        task: &SpeechTaskKind,
    ) -> Option<&dyn ProviderDescriptor> {
        self.providers
            .iter()
            .map(|provider| provider.as_ref())
            .find(|provider| provider.capabilities(asset).supports_task(task))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::model::{EngineType, ModelInfo};
    use crate::providers::{
        LifecycleCost, ModelAsset, ProviderCapabilities, SpeechTaskKind, StreamingSupport,
        TranslationPairSupport,
    };
    use std::path::PathBuf;

    fn test_asset() -> ModelAsset {
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
            PathBuf::from("test.bin"),
        )
    }

    struct FakeProvider {
        id: &'static str,
        capabilities: ProviderCapabilities,
    }

    impl ProviderDescriptor for FakeProvider {
        fn provider_id(&self) -> &'static str {
            self.id
        }

        fn capabilities(&self, _asset: &ModelAsset) -> ProviderCapabilities {
            self.capabilities.clone()
        }
    }

    #[test]
    fn registry_selects_provider_supporting_task() {
        let asset = test_asset();
        let registry = ProviderRegistry::new(vec![
            Box::new(FakeProvider {
                id: "nope",
                capabilities: ProviderCapabilities {
                    tasks: vec![SpeechTaskKind::TranslateText],
                    translation_pairs: TranslationPairSupport::None,
                    streaming: StreamingSupport::None,
                    lifecycle: LifecycleCost::NoLoad,
                },
            }),
            Box::new(FakeProvider {
                id: "asr",
                capabilities: ProviderCapabilities {
                    tasks: vec![SpeechTaskKind::Transcribe],
                    translation_pairs: TranslationPairSupport::None,
                    streaming: StreamingSupport::None,
                    lifecycle: LifecycleCost::Cheap,
                },
            }),
        ]);

        let selected = registry
            .select(&asset, &SpeechTaskKind::Transcribe)
            .expect("provider should be selected");
        assert_eq!(selected.provider_id(), "asr");
    }
}
