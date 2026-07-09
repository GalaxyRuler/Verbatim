use serde_json::Value;
use std::fmt;

const TRANSFORMED_TEXT_DESCRIPTION: &str = "The transformed selected text";

#[derive(Clone, Debug, Default)]
pub struct ProviderReasoningConfig {
    pub reasoning_effort: Option<String>,
    pub reasoning: Option<crate::llm_client::ReasoningConfig>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructuredTextRequest {
    pub field: String,
    pub description: String,
}

impl StructuredTextRequest {
    pub fn new(field: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            description: description.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextProviderRequest {
    pub user_content: String,
    pub system_prompt: Option<String>,
    pub structured_output: Option<StructuredTextRequest>,
}

impl TextProviderRequest {
    pub fn new(user_content: impl Into<String>, system_prompt: Option<String>) -> Self {
        Self {
            user_content: user_content.into(),
            system_prompt,
            structured_output: None,
        }
    }

    pub fn structured(
        user_content: impl Into<String>,
        system_prompt: Option<impl Into<String>>,
        field: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            user_content: user_content.into(),
            system_prompt: system_prompt.map(Into::into),
            structured_output: Some(StructuredTextRequest::new(field, description)),
        }
    }

    pub fn transform_structured(
        user_content: impl Into<String>,
        system_prompt: Option<impl Into<String>>,
    ) -> Self {
        Self::structured(
            user_content,
            system_prompt,
            "transform",
            TRANSFORMED_TEXT_DESCRIPTION,
        )
    }

    pub fn json_schema(&self) -> Option<Value> {
        self.structured_output
            .as_ref()
            .map(|structured| structured_text_schema(&structured.field, &structured.description))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructuredTextError {
    InvalidJson(String),
    MissingStringField(String),
}

impl fmt::Display for StructuredTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(err) => {
                write!(formatter, "Structured response was not valid JSON: {err}")
            }
            Self::MissingStringField(field) => write!(
                formatter,
                "Structured response did not include a string '{field}' field"
            ),
        }
    }
}

impl std::error::Error for StructuredTextError {}

pub fn strip_invisible_chars(text: &str) -> String {
    text.replace(['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'], "")
}

pub fn extract_structured_text(content: &str, field: &str) -> Result<String, StructuredTextError> {
    let json = serde_json::from_str::<Value>(content)
        .map_err(|err| StructuredTextError::InvalidJson(err.to_string()))?;

    json.get(field)
        .and_then(|value| value.as_str())
        .map(strip_invisible_chars)
        .ok_or_else(|| StructuredTextError::MissingStringField(field.to_string()))
}

pub fn provider_reasoning_config(provider_id: &str) -> ProviderReasoningConfig {
    match provider_id {
        "custom" => ProviderReasoningConfig {
            reasoning_effort: Some("none".to_string()),
            reasoning: None,
        },
        "openrouter" => ProviderReasoningConfig {
            reasoning_effort: None,
            reasoning: Some(crate::llm_client::ReasoningConfig {
                effort: Some("none".to_string()),
                exclude: Some(true),
            }),
        },
        _ => ProviderReasoningConfig::default(),
    }
}

pub fn structured_text_schema(field: &str, description: &str) -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            field: {
                "type": "string",
                "description": description
            }
        },
        "required": [field],
        "additionalProperties": false
    })
}

pub async fn send_text_provider_request(
    provider: &crate::settings::PostProcessProvider,
    api_key: String,
    model: &str,
    request: TextProviderRequest,
) -> Result<Option<String>, String> {
    send_text_provider_request_with_cancellation(provider, api_key, model, request, None).await
}

pub async fn send_text_provider_request_with_cancellation(
    provider: &crate::settings::PostProcessProvider,
    api_key: String,
    model: &str,
    request: TextProviderRequest,
    cancellation: Option<&crate::providers::CancellationToken>,
) -> Result<Option<String>, String> {
    let request_config = provider_reasoning_config(&provider.id);
    let json_schema = request.json_schema();
    crate::llm_client::send_chat_completion_with_schema_and_cancellation(
        provider,
        api_key,
        model,
        request.user_content,
        request.system_prompt,
        json_schema,
        request_config.reasoning_effort,
        request_config.reasoning,
        cancellation,
    )
    .await
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextValidationError {
    PreservationChecks(String),
    UnrequestedTranslation(String),
}

impl fmt::Display for TextValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PreservationChecks(issues) => {
                write!(formatter, "preservation checks: {issues}")
            }
            Self::UnrequestedTranslation(err) => write!(formatter, "{err}"),
        }
    }
}

impl std::error::Error for TextValidationError {}

pub fn validate_preserved_text(input: &str, output: &str) -> Result<(), TextValidationError> {
    let evaluation = crate::local_llm::evaluation::evaluate_post_processing_output(input, output);
    if !evaluation.passed {
        return Err(TextValidationError::PreservationChecks(format!(
            "{:?}",
            evaluation.issues
        )));
    }

    validate_no_unrequested_translation(input, output)
}

pub fn validate_no_unrequested_translation(
    input: &str,
    output: &str,
) -> Result<(), TextValidationError> {
    crate::adaptive::processor::validate_unrequested_translation(input, output)
        .map_err(TextValidationError::UnrequestedTranslation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_reasoning_config_disables_reasoning_for_custom_and_openrouter() {
        let custom = provider_reasoning_config("custom");
        assert_eq!(custom.reasoning_effort.as_deref(), Some("none"));
        assert!(custom.reasoning.is_none());

        let openrouter = provider_reasoning_config("openrouter");
        assert_eq!(openrouter.reasoning_effort, None);
        let reasoning = openrouter.reasoning.expect("OpenRouter reasoning config");
        assert_eq!(reasoning.effort.as_deref(), Some("none"));
        assert_eq!(reasoning.exclude, Some(true));

        let openai = provider_reasoning_config("openai");
        assert!(openai.reasoning_effort.is_none());
        assert!(openai.reasoning.is_none());
    }

    #[test]
    fn structured_text_schema_requires_named_string_field() {
        let schema = structured_text_schema("transform", "The transformed selected text");

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["required"][0], "transform");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["transform"]["type"], "string");
        assert_eq!(
            schema["properties"]["transform"]["description"],
            "The transformed selected text"
        );
    }

    #[test]
    fn preserved_text_validation_rejects_script_loss() {
        let err = validate_preserved_text(
            "Please send this to خالد today.",
            "Please send this to Khalid today.",
        )
        .expect_err("script loss should be rejected");

        assert!(err.to_string().contains("preservation checks"));
        assert!(err.to_string().contains("LostArabicScript"));
    }

    #[test]
    fn structured_provider_request_preserves_prompt_boundary_and_schema() {
        let request = TextProviderRequest::structured(
            "Selected text",
            Some("System rules"),
            "transform",
            "The transformed selected text",
        );
        let schema = request.json_schema().expect("structured schema");

        assert_eq!(request.user_content, "Selected text");
        assert_eq!(request.system_prompt.as_deref(), Some("System rules"));
        assert_eq!(schema["required"][0], "transform");
        assert_eq!(
            schema["properties"]["transform"]["description"],
            "The transformed selected text"
        );
    }
}
