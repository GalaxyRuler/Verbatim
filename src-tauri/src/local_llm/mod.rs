pub mod catalog;
pub mod download;
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

    #[test]
    fn list_models_for_dir_reports_downloaded_and_partial_state() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let model_path = temp_dir.path().join("Qwen3-1.7B-Q4_K_M.gguf");
        let partial_path = temp_dir
            .path()
            .join("SmolLM2-1.7B-Instruct-Q4_K_M.gguf.partial");

        std::fs::write(&model_path, b"complete").expect("complete model");
        std::fs::write(&partial_path, b"partial").expect("partial model");

        let models = crate::local_llm::download::list_models_for_dir(temp_dir.path())
            .expect("models with status");
        let qwen = models
            .iter()
            .find(|model| model.id == "qwen3-1_7b-q4_k_m")
            .expect("qwen model");
        let smol = models
            .iter()
            .find(|model| model.id == "smollm2-1_7b-instruct-q4_k_m")
            .expect("smol model");

        assert!(qwen.is_downloaded);
        assert_eq!(qwen.partial_size, 0);
        assert!(!smol.is_downloaded);
        assert_eq!(smol.partial_size, 7);
    }

    #[test]
    fn checksum_mismatch_deletes_partial_local_llm_file() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let partial_path = temp_dir.path().join("model.gguf.partial");
        std::fs::write(&partial_path, b"bad model").expect("partial");

        let result = crate::local_llm::download::verify_sha256(
            &partial_path,
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
            "bad-model",
        );

        assert!(result.is_err());
        assert!(!partial_path.exists());
    }

    #[test]
    fn delete_model_from_dir_removes_complete_and_partial_files() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let model = crate::local_llm::catalog::load_builtin_local_llm_models()
            .expect("catalog")
            .remove("qwen3-1_7b-q4_k_m")
            .expect("qwen");
        let model_path = crate::local_llm::download::model_path(temp_dir.path(), &model);
        let partial_path = crate::local_llm::download::partial_path(temp_dir.path(), &model);

        std::fs::write(&model_path, b"complete").expect("complete");
        std::fs::write(&partial_path, b"partial").expect("partial");

        crate::local_llm::download::delete_model_from_dir(temp_dir.path(), &model).expect("delete");

        assert!(!model_path.exists());
        assert!(!partial_path.exists());
    }

    #[test]
    fn selected_runtime_model_requires_downloaded_artifact() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let mut settings = LocalLlmSettings::default();
        settings.enabled = true;
        settings.selected_model_id = "qwen3-1_7b-q4_k_m".to_string();

        let missing = crate::local_llm::download::selected_downloaded_model_for_runtime(
            temp_dir.path(),
            &settings,
        )
        .expect("selection check");

        assert!(missing.is_none());

        let model = crate::local_llm::catalog::load_builtin_local_llm_models()
            .expect("catalog")
            .remove("qwen3-1_7b-q4_k_m")
            .expect("qwen");
        let model_path = crate::local_llm::download::model_path(temp_dir.path(), &model);
        std::fs::write(&model_path, b"complete").expect("complete model");

        let selected = crate::local_llm::download::selected_downloaded_model_for_runtime(
            temp_dir.path(),
            &settings,
        )
        .expect("selection check")
        .expect("downloaded model");

        assert_eq!(selected.0.id, "qwen3-1_7b-q4_k_m");
        assert_eq!(selected.1, model_path);
    }
}
