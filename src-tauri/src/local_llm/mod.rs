pub mod catalog;

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
            selected_model_id: "qwen3-1_7b-q4_k_m".to_string(),
            runtime_mode: "managed".to_string(),
            runtime_host: "127.0.0.1".to_string(),
            runtime_port: 0,
            unload_timeout_secs: 300,
            max_output_tokens: 512,
        }
    }
}
