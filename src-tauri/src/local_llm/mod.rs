pub mod catalog;
pub mod evaluation;
pub mod runtime;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Type)]
pub struct LocalLlmSettings {
    pub enabled: bool,
    pub selected_model_id: String,
    pub runtime_mode: String,
    pub runtime_host: String,
    pub runtime_port: u16,
    pub unload_timeout_secs: u64,
    pub max_output_tokens: u16,
}

impl Default for LocalLlmSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            selected_model_id: String::new(),
            runtime_mode: "managed".to_string(),
            runtime_host: "127.0.0.1".to_string(),
            runtime_port: 0,
            unload_timeout_secs: 300,
            max_output_tokens: 512,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_do_not_preselect_unproven_managed_model() {
        let settings = LocalLlmSettings::default();

        assert!(!settings.enabled);
        assert!(settings.selected_model_id.is_empty());
    }

    #[test]
    fn evaluation_rejects_mixed_script_output_that_drops_arabic() {
        let evaluation = crate::local_llm::evaluation::evaluate_post_processing_output(
            "meeting at two pm بخصوص التقرير النهائي",
            "meeting at two pm regarding the final report",
        );

        assert!(!evaluation.passed);
        assert!(evaluation
            .has_issue(crate::local_llm::evaluation::LocalLlmEvaluationIssue::LostArabicScript));
    }

    #[test]
    fn runtime_args_always_bind_to_loopback_and_disable_reasoning() {
        let args = crate::local_llm::runtime::build_llama_server_args(
            std::path::Path::new("C:/models/qwen.gguf"),
            18080,
        );

        assert!(args.windows(2).any(|pair| pair == ["--host", "127.0.0.1"]));
        assert!(args.windows(2).any(|pair| pair == ["--port", "18080"]));
        assert!(args.windows(2).any(|pair| pair == ["--reasoning", "off"]));
        assert!(!args.iter().any(|arg| arg == "0.0.0.0"));
    }
}
