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
    fn asset_urls_resolve_to_configured_asset_host() {
        let models = load_builtin_models().expect("catalog parses");
        let medium = models.get("medium").expect("medium exists");

        assert!(medium
            .url
            .as_deref()
            .expect("medium has download url")
            .ends_with("/whisper-medium-q4_1.bin"));
    }
}
