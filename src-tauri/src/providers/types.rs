use crate::managers::model::ModelInfo;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum SpeechInput {
    Audio(Arc<[f32]>),
    Text(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpeechTaskKind {
    Transcribe,
    TranslateSpeech,
    TranslateText,
    PostProcessText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguageSelection {
    Auto,
    Language(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranslationTarget {
    pub source_language: LanguageSelection,
    pub target_language: String,
}

#[derive(Clone, Debug)]
pub struct SpeechRequest {
    pub task: SpeechTaskKind,
    pub input: SpeechInput,
    pub translation: Option<TranslationTarget>,
    pub language_shortlist: Vec<String>,
    pub custom_words: Vec<String>,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechResponse {
    pub text: String,
    pub detected_language: Option<String>,
    pub translated: bool,
    pub provider_id: &'static str,
    pub model_id: String,
}

#[derive(Clone, Debug)]
pub struct ModelAsset {
    pub id: String,
    pub locator: ModelLocator,
    pub metadata: ModelInfo,
}

impl ModelAsset {
    pub fn from_model_info(metadata: ModelInfo, resolved_path: PathBuf) -> Self {
        let locator = if metadata.is_directory {
            ModelLocator::Directory(resolved_path)
        } else {
            ModelLocator::File(resolved_path)
        };

        Self {
            id: metadata.id.clone(),
            locator,
            metadata,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelLocator {
    File(PathBuf),
    Directory(PathBuf),
    ManagedServer {
        endpoint: String,
        health_url: Option<String>,
    },
    ExternalHttp {
        endpoint: String,
        credential_ref: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub tasks: Vec<SpeechTaskKind>,
    pub translation_pairs: TranslationPairSupport,
    pub streaming: StreamingSupport,
    pub lifecycle: LifecycleCost,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranslationPairSupport {
    None,
    EnglishOnly,
    Explicit(Vec<(String, String)>),
    AnyToAny { languages: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamingSupport {
    None,
    PartialText,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LifecycleCost {
    NoLoad,
    Cheap,
    Expensive,
    SidecarProcess,
}

impl TranslationPairSupport {
    pub fn supports_pair(&self, source_language: &str, target_language: &str) -> bool {
        match self {
            TranslationPairSupport::None => false,
            TranslationPairSupport::EnglishOnly => target_language == "en",
            TranslationPairSupport::Explicit(pairs) => pairs
                .iter()
                .any(|(source, target)| source == source_language && target == target_language),
            TranslationPairSupport::AnyToAny { languages } => {
                languages.iter().any(|language| language == source_language)
                    && languages.iter().any(|language| language == target_language)
            }
        }
    }
}

impl ProviderCapabilities {
    pub fn supports_task(&self, task: &SpeechTaskKind) -> bool {
        self.tasks.iter().any(|candidate| candidate == task)
    }
}

#[derive(Clone, Debug, Default)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

pub trait EngineProvider: Send {
    fn provider_id(&self) -> &'static str;
    fn capabilities(&self, asset: &ModelAsset) -> ProviderCapabilities;
    fn load(&mut self, asset: &ModelAsset) -> Result<()>;
    fn unload(&mut self);
    fn run(&mut self, request: SpeechRequest) -> Result<SpeechResponse>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::model::{EngineType, ModelInfo};
    use std::path::PathBuf;
    use std::sync::Arc;

    fn minimal_model_info(id: &str, is_directory: bool) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            name: "Test Model".to_string(),
            description: "Test".to_string(),
            filename: "test-model".to_string(),
            url: None,
            sha256: None,
            size_mb: 1,
            is_downloaded: true,
            is_downloading: false,
            partial_size: 0,
            is_directory,
            engine_type: EngineType::Whisper,
            accuracy_score: 0.0,
            speed_score: 0.0,
            supports_translation: false,
            is_recommended: false,
            supported_languages: vec![],
            supports_language_selection: true,
            is_custom: false,
        }
    }

    #[test]
    fn speech_input_requires_exactly_one_input_kind() {
        let audio = SpeechInput::Audio(Arc::from([0.0_f32, 1.0_f32]));
        assert!(matches!(audio, SpeechInput::Audio(_)));

        let text = SpeechInput::Text("hello".to_string());
        assert!(matches!(text, SpeechInput::Text(_)));
    }

    #[test]
    fn cancellation_token_defaults_to_not_cancelled() {
        let token = CancellationToken::default();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn english_only_translation_supports_any_source_to_english_only() {
        let support = TranslationPairSupport::EnglishOnly;
        assert!(support.supports_pair("ar", "en"));
        assert!(support.supports_pair("fr", "en"));
        assert!(!support.supports_pair("en", "ar"));
        assert!(!support.supports_pair("fr", "es"));
    }

    #[test]
    fn explicit_translation_pairs_require_exact_match() {
        let support = TranslationPairSupport::Explicit(vec![
            ("es".to_string(), "fr".to_string()),
            ("fr".to_string(), "yue".to_string()),
        ]);

        assert!(support.supports_pair("es", "fr"));
        assert!(support.supports_pair("fr", "yue"));
        assert!(!support.supports_pair("fr", "es"));
    }

    #[test]
    fn any_to_any_translation_requires_both_languages_in_set() {
        let support = TranslationPairSupport::AnyToAny {
            languages: vec!["ar".to_string(), "en".to_string(), "fr".to_string()],
        };

        assert!(support.supports_pair("ar", "fr"));
        assert!(support.supports_pair("fr", "en"));
        assert!(!support.supports_pair("ar", "es"));
        assert!(!support.supports_pair("es", "en"));
    }

    #[test]
    fn capability_task_lookup_is_exact() {
        let capabilities = ProviderCapabilities {
            tasks: vec![SpeechTaskKind::Transcribe, SpeechTaskKind::TranslateText],
            translation_pairs: TranslationPairSupport::None,
            streaming: StreamingSupport::None,
            lifecycle: LifecycleCost::Cheap,
        };

        assert!(capabilities.supports_task(&SpeechTaskKind::Transcribe));
        assert!(capabilities.supports_task(&SpeechTaskKind::TranslateText));
        assert!(!capabilities.supports_task(&SpeechTaskKind::TranslateSpeech));
    }

    #[test]
    fn model_asset_uses_directory_locator_for_directory_models() {
        let info = minimal_model_info("dir-model", true);
        let asset = ModelAsset::from_model_info(info, PathBuf::from("C:/models/dir-model"));
        assert!(matches!(asset.locator, ModelLocator::Directory(_)));
    }

    #[test]
    fn model_asset_uses_file_locator_for_file_models() {
        let info = minimal_model_info("file-model", false);
        let asset = ModelAsset::from_model_info(info, PathBuf::from("C:/models/model.bin"));
        assert!(matches!(asset.locator, ModelLocator::File(_)));
    }
}
