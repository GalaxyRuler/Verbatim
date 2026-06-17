use crate::settings::PostProcessProvider;
use anyhow::Result;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

pub const VERBATIM_LOCAL_PROVIDER_ID: &str = "verbatim_local";
pub const VERBATIM_LOCAL_PROVIDER_LABEL: &str = "Verbatim Local";
pub const LOCALHOST_BIND_ADDRESS: &str = "127.0.0.1";
const DEFAULT_CONTEXT_SIZE: &str = "2048";

pub fn build_llama_server_args(model_path: &Path, port: u16) -> Vec<String> {
    vec![
        "-m".to_string(),
        model_path.to_string_lossy().into_owned(),
        "--host".to_string(),
        LOCALHOST_BIND_ADDRESS.to_string(),
        "--port".to_string(),
        port.to_string(),
        "--ctx-size".to_string(),
        DEFAULT_CONTEXT_SIZE.to_string(),
        "--threads".to_string(),
        "-1".to_string(),
        "--threads-batch".to_string(),
        "-1".to_string(),
        "--reasoning".to_string(),
        "off".to_string(),
        "--no-webui".to_string(),
    ]
}

pub fn build_managed_local_provider(port: u16) -> PostProcessProvider {
    PostProcessProvider {
        id: VERBATIM_LOCAL_PROVIDER_ID.to_string(),
        label: VERBATIM_LOCAL_PROVIDER_LABEL.to_string(),
        base_url: format!("http://{}:{}/v1", LOCALHOST_BIND_ADDRESS, port),
        allow_base_url_edit: false,
        models_endpoint: Some("/models".to_string()),
        supports_structured_output: false,
    }
}

#[derive(Debug, Clone)]
pub struct ManagedLocalLlmEndpoint {
    pub provider: PostProcessProvider,
    pub model_id: String,
    pub model: String,
}

pub fn build_managed_local_endpoint(
    port: u16,
    model_id: &str,
    model_name: &str,
) -> ManagedLocalLlmEndpoint {
    ManagedLocalLlmEndpoint {
        provider: build_managed_local_provider(port),
        model_id: model_id.to_string(),
        model: model_name.to_string(),
    }
}

pub fn select_runtime_port(configured_port: u16) -> Result<u16> {
    if configured_port > 0 {
        return Ok(configured_port);
    }

    let listener = TcpListener::bind((LOCALHOST_BIND_ADDRESS, 0))?;
    Ok(listener.local_addr()?.port())
}

pub fn local_post_processing_system_prompt() -> String {
    [
        "You clean dictated transcripts for Verbatim.",
        "Fix only punctuation, capitalization, spacing, and obvious dictation artifacts.",
        "Do not translate any text.",
        "Do not add facts, greetings, signoffs, explanations, or new content.",
        "Preserve every language and script already present in the input.",
        "Preserve names, code, numbers, URLs, emails, and mixed-language text.",
        "Return only the cleaned transcript.",
    ]
    .join("\n")
}

pub fn resolve_llama_server_executable_from_sources(
    explicit_path: Option<&Path>,
    path_env: &str,
    local_app_data: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = explicit_path {
        return Some(path.to_path_buf());
    }

    for dir in std::env::split_paths(path_env) {
        for candidate_name in llama_server_candidate_names() {
            let candidate = dir.join(candidate_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let local_app_data = local_app_data?;
    let root = local_app_data.join("Programs").join("llama.cpp");
    for candidate_name in llama_server_candidate_names() {
        let direct = root.join(candidate_name);
        if direct.is_file() {
            return Some(direct);
        }

        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let candidate = entry.path().join(candidate_name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

pub fn resolve_llama_server_executable() -> Option<PathBuf> {
    let explicit_path = std::env::var_os("LLAMA_SERVER_PATH").map(PathBuf::from);
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);

    resolve_llama_server_executable_from_sources(
        explicit_path.as_deref(),
        &path_env.to_string_lossy(),
        local_app_data.as_deref(),
    )
}

fn llama_server_candidate_names() -> &'static [&'static str] {
    #[cfg(target_os = "windows")]
    {
        &["llama-server.exe"]
    }
    #[cfg(not(target_os = "windows"))]
    {
        &["llama-server"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_provider_uses_loopback_openai_endpoint() {
        let provider = build_managed_local_provider(18080);

        assert_eq!(provider.id, VERBATIM_LOCAL_PROVIDER_ID);
        assert_eq!(provider.base_url, "http://127.0.0.1:18080/v1");
        assert!(!provider.allow_base_url_edit);
        assert_eq!(provider.models_endpoint.as_deref(), Some("/models"));
    }

    #[test]
    fn local_post_processing_prompt_preserves_language_and_meaning() {
        let prompt = local_post_processing_system_prompt();

        assert!(prompt.contains("Do not translate"));
        assert!(prompt.contains("Do not add facts"));
        assert!(prompt.contains("Return only"));
    }

    #[test]
    fn runtime_path_resolution_prefers_explicit_executable() {
        let explicit = Path::new("C:/tools/llama-server.exe");
        let resolved = resolve_llama_server_executable_from_sources(Some(explicit), "", None);

        assert_eq!(resolved.as_deref(), Some(explicit));
    }

    #[test]
    fn runtime_port_selection_uses_configured_port_or_loopback_ephemeral() {
        assert_eq!(select_runtime_port(18081).expect("configured port"), 18081);

        let port = select_runtime_port(0).expect("ephemeral port");
        assert!(port > 0);
    }

    #[test]
    fn managed_endpoint_uses_downloaded_filename_as_model_name() {
        let endpoint = build_managed_local_endpoint(18080, "tiny", "tiny.gguf");

        assert_eq!(endpoint.model_id, "tiny");
        assert_eq!(endpoint.model, "tiny.gguf");
        assert_eq!(endpoint.provider.base_url, "http://127.0.0.1:18080/v1");
    }
}
