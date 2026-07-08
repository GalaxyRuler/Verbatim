use super::model::{EngineType, ModelInfo};
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

const DEFAULT_VERBATIM_ASSET_BASE_URL: &str = "https://verbatim-assets.galaxyruler.space";

fn verbatim_asset_url(filename: &str) -> String {
    let base_url = option_env!("VERBATIM_ASSET_BASE_URL")
        .unwrap_or(DEFAULT_VERBATIM_ASSET_BASE_URL)
        .trim_end_matches('/');
    format!("{}/{}", base_url, filename.trim_start_matches('/'))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuiltinModelDefinition {
    id: String,
    name: String,
    description: String,
    filename: String,
    url: Option<String>,
    sha256: Option<String>,
    size_mb: u64,
    is_directory: bool,
    engine_type: EngineType,
    license_label: String,
    accelerator_support: Vec<String>,
    accuracy_score: f32,
    speed_score: f32,
    supports_translation: bool,
    is_recommended: bool,
    supported_languages: SupportedLanguages,
    supports_language_selection: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SupportedLanguages {
    Explicit(Vec<String>),
    Set { set: LanguageSet },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum LanguageSet {
    Whisper,
    ParakeetV3,
    SenseVoice,
    Cohere,
}

pub fn load_builtin_models() -> Result<HashMap<String, ModelInfo>> {
    let definitions: Vec<BuiltinModelDefinition> =
        serde_json::from_str(include_str!("../../resources/model_catalog.json"))?;

    Ok(definitions
        .into_iter()
        .map(|definition| {
            let model = definition.into_model_info();
            (model.id.clone(), model)
        })
        .collect())
}

impl BuiltinModelDefinition {
    fn into_model_info(self) -> ModelInfo {
        ModelInfo {
            id: self.id,
            name: self.name,
            description: self.description,
            filename: self.filename,
            url: self.url.map(|url| {
                if let Some(filename) = url.strip_prefix("asset:") {
                    verbatim_asset_url(filename)
                } else {
                    url
                }
            }),
            sha256: self.sha256,
            size_mb: self.size_mb,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
            is_directory: self.is_directory,
            engine_type: self.engine_type,
            license_label: self.license_label,
            accelerator_support: self.accelerator_support,
            accuracy_score: self.accuracy_score,
            speed_score: self.speed_score,
            supports_translation: self.supports_translation,
            is_recommended: self.is_recommended,
            supported_languages: self.supported_languages.into_languages(),
            supports_language_selection: self.supports_language_selection,
            is_custom: false,
        }
    }
}

impl SupportedLanguages {
    fn into_languages(self) -> Vec<String> {
        match self {
            Self::Explicit(languages) => languages,
            Self::Set { set } => set.languages(),
        }
    }
}

impl LanguageSet {
    fn languages(self) -> Vec<String> {
        match self {
            Self::Whisper => [
                "en", "zh", "zh-Hans", "zh-Hant", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr",
                "pl", "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms",
                "cs", "ro", "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml",
                "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu",
                "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn",
                "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
                "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt",
                "haw", "ln", "ha", "ba", "jw", "su", "yue",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            Self::ParakeetV3 => [
                "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
                "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            Self::SenseVoice => ["zh", "zh-Hans", "zh-Hant", "en", "yue", "ja", "ko"]
                .into_iter()
                .map(String::from)
                .collect(),
            Self::Cohere => [
                "en", "fr", "de", "it", "es", "pt", "el", "nl", "pl", "zh", "zh-Hans", "zh-Hant",
                "ja", "ko", "vi", "ar",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_loads_builtin_models() {
        let models = load_builtin_models().expect("catalog parses");

        assert!(models.contains_key("small"));
        assert!(models.contains_key("parakeet-tdt-0.6b-v3"));
        assert!(models.contains_key("cohere-int8"));
        assert_eq!(
            models
                .get("parakeet-tdt-0.6b-v3")
                .expect("parakeet v3 exists")
                .supported_languages
                .len(),
            25
        );
    }

    #[test]
    fn cohere_arabic_entry_is_arabic_and_english_only() {
        let models = load_builtin_models().expect("catalog parses");
        let model = models
            .get("cohere-arabic-int8")
            .expect("cohere-arabic-int8 exists");

        assert!(matches!(model.engine_type, EngineType::Cohere));
        assert_eq!(model.supported_languages, ["ar", "en"]);
        assert!(model.supports_language_selection);
        assert!(!model.is_recommended);
        assert!(!model.supports_translation);
        assert!(model
            .url
            .as_deref()
            .expect("download url")
            .ends_with("/cohere-arabic-int8.tar.gz"));
    }

    #[test]
    fn asset_urls_resolve_to_configured_asset_host() {
        let models = load_builtin_models().expect("catalog parses");
        let medium = models.get("medium").expect("medium exists");

        assert!(medium
            .url
            .as_deref()
            .expect("medium has download url")
            .ends_with("/whisper-medium-q4_1.bin"));
    }

    #[test]
    fn downloadable_catalog_models_have_https_urls_and_sha256() {
        let models = load_builtin_models().expect("catalog parses");

        for model in models.values() {
            let url = model.url.as_deref().expect("downloadable model URL");
            assert!(
                url.starts_with("https://"),
                "{} must use HTTPS for downloads",
                model.id
            );

            let sha256 = model.sha256.as_deref().expect("download checksum");
            assert_eq!(sha256.len(), 64, "{} SHA-256 length", model.id);
            assert!(
                sha256.chars().all(|ch| ch.is_ascii_hexdigit()),
                "{} SHA-256 must be hex",
                model.id
            );
            assert!(
                model.size_mb > 0,
                "{} must declare a positive size",
                model.id
            );
        }
    }

    #[test]
    fn catalog_models_have_complete_language_and_score_metadata() {
        let models = load_builtin_models().expect("catalog parses");

        for model in models.values() {
            assert!(!model.id.trim().is_empty(), "model id is required");
            assert!(
                !model.name.trim().is_empty(),
                "{} name is required",
                model.id
            );
            assert!(
                !model.filename.trim().is_empty(),
                "{} filename is required",
                model.id
            );
            assert!(
                !model.supported_languages.is_empty(),
                "{} must declare supported languages",
                model.id
            );
            assert!(
                (0.0..=1.0).contains(&model.accuracy_score),
                "{} accuracy score must be normalized",
                model.id
            );
            assert!(
                (0.0..=1.0).contains(&model.speed_score),
                "{} speed score must be normalized",
                model.id
            );

            let mut seen_languages = HashSet::new();
            for language in &model.supported_languages {
                assert_eq!(
                    language.trim(),
                    language,
                    "{} language code must be trimmed",
                    model.id
                );
                assert!(
                    !language.is_empty(),
                    "{} language code is required",
                    model.id
                );
                assert!(
                    seen_languages.insert(language),
                    "{} has duplicate language code '{}'",
                    model.id,
                    language
                );
            }
        }
    }

    #[test]
    fn catalog_models_have_license_and_accelerator_metadata() {
        let models = load_builtin_models().expect("catalog parses");
        let allowed_accelerators = HashSet::from(["whisper-cpp", "onnx-runtime"]);
        let allowed_license_labels =
            HashSet::from(["Apache-2.0", "CC-BY-4.0", "MIT", "Requires upstream review"]);

        for model in models.values() {
            assert!(
                !model.license_label.trim().is_empty(),
                "{} license label is required",
                model.id
            );
            assert_eq!(
                model.license_label.trim(),
                model.license_label,
                "{} license label must be trimmed",
                model.id
            );
            assert!(
                allowed_license_labels.contains(model.license_label.as_str()),
                "{} unsupported license label '{}'",
                model.id,
                model.license_label
            );
            assert!(
                !model.accelerator_support.is_empty(),
                "{} accelerator support is required",
                model.id
            );

            let mut seen_accelerators = HashSet::new();
            for accelerator in &model.accelerator_support {
                assert_eq!(
                    accelerator.trim(),
                    accelerator,
                    "{} accelerator label must be trimmed",
                    model.id
                );
                assert!(
                    allowed_accelerators.contains(accelerator.as_str()),
                    "{} unsupported accelerator family '{}'",
                    model.id,
                    accelerator
                );
                assert!(
                    seen_accelerators.insert(accelerator),
                    "{} has duplicate accelerator family '{}'",
                    model.id,
                    accelerator
                );
            }

            let expected_accelerator = match model.engine_type {
                EngineType::Whisper => "whisper-cpp",
                EngineType::Parakeet
                | EngineType::Moonshine
                | EngineType::MoonshineStreaming
                | EngineType::SenseVoice
                | EngineType::GigaAM
                | EngineType::Canary
                | EngineType::Cohere => "onnx-runtime",
            };
            assert!(
                model
                    .accelerator_support
                    .iter()
                    .any(|accelerator| accelerator == expected_accelerator),
                "{} must declare {} accelerator compatibility",
                model.id,
                expected_accelerator
            );
        }
    }

    #[test]
    fn catalog_recommendation_and_translation_flags_are_consistent() {
        let models = load_builtin_models().expect("catalog parses");
        let recommended_count = models.values().filter(|model| model.is_recommended).count();

        assert_eq!(
            recommended_count, 1,
            "exactly one built-in transcription model should be recommended"
        );

        for model in models.values() {
            if model.supports_translation {
                assert!(
                    matches!(model.engine_type, EngineType::Whisper | EngineType::Canary),
                    "{} translation support must be backed by an engine with translation capability",
                    model.id
                );
            }
        }
    }
}
