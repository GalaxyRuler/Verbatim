use crate::settings::PostProcessProvider;
use log::debug;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct JsonSchema {
    name: String,
    strict: bool,
    schema: Value,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    json_schema: JsonSchema,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningConfig>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

/// Build headers for API requests based on provider type
fn build_headers(provider: &PostProcessProvider, api_key: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();

    // Common headers
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        REFERER,
        HeaderValue::from_static("https://github.com/GalaxyRuler/Verbatim"),
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static("Verbatim/1.0 (+https://github.com/GalaxyRuler/Verbatim)"),
    );
    headers.insert("X-Title", HeaderValue::from_static("Verbatim"));

    // Provider-specific auth headers
    if !api_key.is_empty() {
        if provider.id == "anthropic" {
            headers.insert(
                "x-api-key",
                HeaderValue::from_str(api_key)
                    .map_err(|e| format!("Invalid API key header value: {}", e))?,
            );
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        } else {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {}", api_key))
                    .map_err(|e| format!("Invalid authorization header value: {}", e))?,
            );
        }
    }

    Ok(headers)
}

/// Create an HTTP client with provider-specific headers
fn create_client(provider: &PostProcessProvider, api_key: &str) -> Result<reqwest::Client, String> {
    let headers = build_headers(provider, api_key)?;
    reqwest::Client::builder()
        .default_headers(headers)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

fn endpoint_urls(base_url: &str, endpoint: &str) -> Vec<String> {
    let base_url = base_url.trim_end_matches('/');
    let mut urls = vec![format!("{}{}", base_url, endpoint)];

    if should_try_openai_v1_fallback(base_url) {
        urls.push(format!("{}/v1{}", base_url, endpoint));
    }

    urls
}

fn should_try_openai_v1_fallback(base_url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(base_url) else {
        return false;
    };

    let path = parsed.path().trim_end_matches('/');
    path.is_empty() || path == "/"
}

fn response_error_message(parsed: &Value) -> Option<String> {
    let error = parsed.get("error")?;

    if let Some(message) = error.as_str() {
        return Some(message.to_string());
    }

    if let Some(message) = error.get("message").and_then(|message| message.as_str()) {
        return Some(message.to_string());
    }

    Some(error.to_string())
}

fn is_unexpected_endpoint_error(message: &str) -> bool {
    message.to_lowercase().contains("unexpected endpoint")
}

async fn read_json_response(
    response: reqwest::Response,
    failure_context: &str,
) -> Result<Value, String> {
    let status = response.status();
    let text = response
        .text()
        .await
        .unwrap_or_else(|_| "Failed to read response".to_string());

    if !status.is_success() {
        return Err(format!("{} failed with status {}", failure_context, status));
    }

    serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse {} response: {}", failure_context, e))
}

/// Send a chat completion request to an OpenAI-compatible API
/// Returns Ok(Some(content)) on success, Ok(None) if response has no content,
/// or Err on actual errors (HTTP, parsing, etc.)
pub async fn send_chat_completion(
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    prompt: String,
    reasoning_effort: Option<String>,
    reasoning: Option<ReasoningConfig>,
) -> Result<Option<String>, String> {
    send_chat_completion_with_schema(
        provider,
        api_key,
        model,
        prompt,
        None,
        None,
        reasoning_effort,
        reasoning,
    )
    .await
}

/// Send a chat completion request with structured output support
/// When json_schema is provided, uses structured outputs mode
/// system_prompt is used as the system message when provided
/// reasoning_effort sets the OpenAI-style top-level field (e.g., "none", "low", "medium", "high")
/// reasoning sets the OpenRouter-style nested object (effort + exclude)
pub async fn send_chat_completion_with_schema(
    provider: &PostProcessProvider,
    api_key: String,
    model: &str,
    user_content: String,
    system_prompt: Option<String>,
    json_schema: Option<Value>,
    reasoning_effort: Option<String>,
    reasoning: Option<ReasoningConfig>,
) -> Result<Option<String>, String> {
    let client = create_client(provider, &api_key)?;

    // Build messages vector
    let mut messages = Vec::new();

    // Add system prompt if provided
    if let Some(system) = system_prompt {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: system,
        });
    }

    // Add user message
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: user_content,
    });

    // Build response_format if schema is provided
    let response_format = json_schema.map(|schema| ResponseFormat {
        format_type: "json_schema".to_string(),
        json_schema: JsonSchema {
            name: "transcription_output".to_string(),
            strict: true,
            schema,
        },
    });

    let request_body = ChatCompletionRequest {
        model: model.to_string(),
        messages,
        response_format,
        reasoning_effort,
        reasoning,
    };

    let urls = endpoint_urls(&provider.base_url, "/chat/completions");
    let mut last_error: Option<String> = None;

    for (index, url) in urls.iter().enumerate() {
        debug!("Sending chat completion request to: {}", url);

        let response = client
            .post(url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let parsed = read_json_response(response, "API request").await?;
        if let Some(message) = response_error_message(&parsed) {
            let can_retry = is_unexpected_endpoint_error(&message) && index + 1 < urls.len();
            if can_retry {
                debug!(
                    "Chat completion endpoint '{}' was rejected by provider; retrying OpenAI /v1 endpoint",
                    url
                );
                last_error = Some(message);
                continue;
            }
            return Err(format!("API request failed: {}", message));
        }

        let completion: ChatCompletionResponse = serde_json::from_value(parsed)
            .map_err(|e| format!("Failed to parse API response: {}", e))?;

        return Ok(completion
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone()));
    }

    Err(last_error.unwrap_or_else(|| "API request failed".to_string()))
}

/// Fetch available models from an OpenAI-compatible API
/// Returns a list of model IDs
pub async fn fetch_models(
    provider: &PostProcessProvider,
    api_key: String,
) -> Result<Vec<String>, String> {
    let client = create_client(provider, &api_key)?;
    let urls = endpoint_urls(&provider.base_url, "/models");
    let mut last_error: Option<String> = None;

    for (index, url) in urls.iter().enumerate() {
        debug!("Fetching models from: {}", url);

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch models: {}", e))?;

        let parsed = read_json_response(response, "Model list request").await?;
        if let Some(message) = response_error_message(&parsed) {
            let can_retry = is_unexpected_endpoint_error(&message) && index + 1 < urls.len();
            if can_retry {
                debug!(
                    "Model endpoint '{}' was rejected by provider; retrying OpenAI /v1 endpoint",
                    url
                );
                last_error = Some(message);
                continue;
            }
            return Err(format!("Model list request failed: {}", message));
        }

        let mut models = Vec::new();

        // Handle OpenAI format: { data: [ { id: "..." }, ... ] }
        if let Some(data) = parsed.get("data").and_then(|d| d.as_array()) {
            for entry in data {
                if let Some(id) = entry.get("id").and_then(|i| i.as_str()) {
                    models.push(id.to_string());
                } else if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
                    models.push(name.to_string());
                }
            }
        }
        // Handle array format: [ "model1", "model2", ... ]
        else if let Some(array) = parsed.as_array() {
            for entry in array {
                if let Some(model) = entry.as_str() {
                    models.push(model.to_string());
                }
            }
        }

        return Ok(models);
    }

    Err(last_error.unwrap_or_else(|| "Model list request failed".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn provider(base_url: String) -> PostProcessProvider {
        PostProcessProvider {
            id: "custom".to_string(),
            label: "Custom".to_string(),
            base_url,
            allow_base_url_edit: true,
            models_endpoint: Some("/models".to_string()),
            supports_structured_output: false,
        }
    }

    fn read_request_path(stream: &mut std::net::TcpStream) -> String {
        let mut buffer = [0u8; 2048];
        let read = stream.read(&mut buffer).expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]);
        request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or_default()
            .to_string()
    }

    fn respond_json(stream: &mut std::net::TcpStream, body: &str) {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn respond_json_status(stream: &mut std::net::TcpStream, status: &str, body: &str) {
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    #[tokio::test]
    async fn chat_completion_http_failure_does_not_return_response_body() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("local addr");

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("request");
            assert_eq!(read_request_path(&mut stream), "/v1/chat/completions");
            respond_json_status(
                &mut stream,
                "400 Bad Request",
                r#"{"error":{"message":"Pineapple Lighthouse 17.B3 transcript echoed"}}"#,
            );
        });

        let error = send_chat_completion(
            &provider(format!("http://{}/v1", addr)),
            String::new(),
            "test-model",
            "Pineapple Lighthouse 17.B3".to_string(),
            None,
            None,
        )
        .await
        .expect_err("http failure should be returned");

        assert!(error.contains("status 400 Bad Request"));
        assert!(!error.contains("Pineapple"));
        assert!(!error.contains("transcript echoed"));
        server.join().expect("server thread");
    }

    #[tokio::test]
    async fn fetch_models_retries_v1_when_lm_studio_reports_unexpected_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("local addr");

        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().expect("first request");
            assert_eq!(read_request_path(&mut first), "/models");
            respond_json(
                &mut first,
                r#"{"error":"Unexpected endpoint or method. (GET /models)"}"#,
            );

            let (mut second, _) = listener.accept().expect("retry request");
            assert_eq!(read_request_path(&mut second), "/v1/models");
            respond_json(
                &mut second,
                r#"{"data":[{"id":"google/gemma-4-e4b","object":"model"}]}"#,
            );
        });

        let models = fetch_models(&provider(format!("http://{}", addr)), String::new())
            .await
            .expect("models after v1 retry");

        assert_eq!(models, vec!["google/gemma-4-e4b"]);
        server.join().expect("server thread");
    }

    #[tokio::test]
    async fn chat_completion_retries_v1_when_lm_studio_reports_unexpected_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let addr = listener.local_addr().expect("local addr");

        let server = thread::spawn(move || {
            let (mut first, _) = listener.accept().expect("first request");
            assert_eq!(read_request_path(&mut first), "/chat/completions");
            respond_json(
                &mut first,
                r#"{"error":"Unexpected endpoint or method. (POST /chat/completions)"}"#,
            );

            let (mut second, _) = listener.accept().expect("retry request");
            assert_eq!(read_request_path(&mut second), "/v1/chat/completions");
            respond_json(
                &mut second,
                r#"{"choices":[{"message":{"content":"Cleaned text."}}]}"#,
            );
        });

        let completion = send_chat_completion(
            &provider(format!("http://{}", addr)),
            String::new(),
            "google/gemma-4-e4b",
            "Clean this transcript: cleaned text".to_string(),
            None,
            None,
        )
        .await
        .expect("completion after v1 retry");

        assert_eq!(completion.as_deref(), Some("Cleaned text."));
        server.join().expect("server thread");
    }
}
