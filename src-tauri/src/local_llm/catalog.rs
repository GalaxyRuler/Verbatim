use anyhow::Result;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

const DEFAULT_VERBATIM_ASSET_BASE_URL: &str = "https://verbatim-assets.galaxyruler.space";

fn verbatim_asset_url(filename: &str) -> String {
    let base_url = option_env!("VERBATIM_ASSET_BASE_URL")
        .unwrap_or(DEFAULT_VERBATIM_ASSET_BASE_URL)
        .trim_end_matches('/');
    format!("{}/{}", base_url, filename.trim_start_matches('/'))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
pub struct LocalLlmModelInfo {
    pub id: String,
    pub label: String,
    pub filename: String,
    pub url: Option<String>,
    pub sha256: Option<String>,
    pub size_mb: u64,
    pub quantization: String,
    pub context_window: u32,
    pub recommended_role: String,
    pub supported_language_notes: String,
    pub license_label: String,
    pub runtime: String,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub partial_size: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuiltinLocalLlmDefinition {
    id: String,
    label: String,
    filename: String,
    url: Option<String>,
    sha256: Option<String>,
    size_mb: u64,
    quantization: String,
    context_window: u32,
    recommended_role: String,
    supported_language_notes: String,
    license_label: String,
    runtime: String,
}

pub fn load_builtin_local_llm_models() -> Result<HashMap<String, LocalLlmModelInfo>> {
    let definitions: Vec<BuiltinLocalLlmDefinition> =
        serde_json::from_str(include_str!("../../resources/local_llm_catalog.json"))?;

    Ok(definitions
        .into_iter()
        .map(|definition| {
            let model = definition.into_model_info();
            (model.id.clone(), model)
        })
        .collect())
}

impl BuiltinLocalLlmDefinition {
    fn into_model_info(self) -> LocalLlmModelInfo {
        LocalLlmModelInfo {
            id: self.id,
            label: self.label,
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
            quantization: self.quantization,
            context_window: self.context_window,
            recommended_role: self.recommended_role,
            supported_language_notes: self.supported_language_notes,
            license_label: self.license_label,
            runtime: self.runtime,
            is_downloaded: false,
            is_downloading: false,
            partial_size: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_builtin_local_llm_models() {
        let models = load_builtin_local_llm_models().expect("catalog parses");

        assert!(models.contains_key("qwen3-1_7b-q4_k_m"));
        assert!(models.contains_key("smollm2-1_7b-instruct-q4_k_m"));

        let qwen = models
            .get("qwen3-1_7b-q4_k_m")
            .expect("qwen candidate exists");
        assert_eq!(qwen.quantization, "Q4_K_M");
        assert_eq!(qwen.size_mb, 1280);
        assert_eq!(qwen.runtime, "llama.cpp");
        assert_eq!(qwen.recommended_role, "experimental");
        assert!(qwen.supported_language_notes.contains("multilingual"));
        assert!(qwen.license_label.contains("Apache"));
    }

    #[test]
    fn catalog_includes_tiny_experimental_candidate() {
        let models = load_builtin_local_llm_models().expect("catalog parses");
        let tiny = models
            .get("qwen2_5-0_5b-instruct-q4_k_m")
            .expect("tiny qwen fallback exists");

        let url = tiny.url.as_deref().expect("model has URL");
        assert!(url.starts_with("https://huggingface.co/"));
        assert_eq!(tiny.size_mb, 469);
        assert_eq!(tiny.recommended_role, "experimental");
    }

    #[test]
    fn catalog_does_not_recommend_unverified_models_as_default() {
        let models = load_builtin_local_llm_models().expect("catalog parses");

        for model in models.values() {
            assert_ne!(
                model.recommended_role, "default",
                "{} must pass local evaluation before becoming default",
                model.id
            );
        }
    }

    #[test]
    fn downloadable_catalog_models_have_https_urls_and_sha256() {
        let models = load_builtin_local_llm_models().expect("catalog parses");

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
                "{} SHA-256 must be hexadecimal",
                model.id
            );
        }
    }
}
