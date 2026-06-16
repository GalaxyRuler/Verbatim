use std::path::Path;

const LOCALHOST_BIND_ADDRESS: &str = "127.0.0.1";
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
