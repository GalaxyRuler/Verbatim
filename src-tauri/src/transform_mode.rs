use log::debug;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fmt;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransformAction {
    Polish,
    MakeConcise,
    TurnIntoList,
    TranslateToSelectedLanguage,
    PromptEngineer,
}

impl TransformAction {
    fn allows_translation(&self) -> bool {
        matches!(self, Self::TranslateToSelectedLanguage)
    }

    fn instruction(&self, target_language: Option<&str>) -> String {
        match self {
            Self::Polish => "Polish the selected text for grammar, punctuation, capitalization, and readability without changing meaning.".to_string(),
            Self::MakeConcise => {
                "Make the selected text more concise while preserving every important detail and the original language.".to_string()
            }
            Self::TurnIntoList => {
                "Turn the selected text into a clear numbered or bulleted list when the content naturally contains multiple items. Preserve the original language.".to_string()
            }
            Self::TranslateToSelectedLanguage => format!(
                "Translate the selected text to {}. Preserve names, numbers, URLs, emails, and formatting intent.",
                target_language.unwrap_or("the selected target language")
            ),
            Self::PromptEngineer => {
                "Rewrite the selected text as a clearer, more precise prompt. Preserve the user's intent, constraints, and language unless translation was explicitly requested.".to_string()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransformModeError {
    MissingSelection,
    MissingTargetLanguage,
    MissingProvider,
    MissingModel(String),
    MissingApiKey(String),
    ProviderUnavailable(String),
    OutputRejected(String),
}

impl fmt::Display for TransformModeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSelection => write!(formatter, "No selected text to transform"),
            Self::MissingTargetLanguage => {
                write!(formatter, "Translation transform requires a target language")
            }
            Self::MissingProvider => write!(formatter, "No post-processing provider is selected"),
            Self::MissingModel(provider_id) => write!(
                formatter,
                "Post-processing provider '{}' has no model configured",
                provider_id
            ),
            Self::MissingApiKey(provider_id) => write!(
                formatter,
                "Remote provider '{}' requires an API key before selected text can leave this device",
                provider_id
            ),
            Self::ProviderUnavailable(message) => write!(formatter, "{message}"),
            Self::OutputRejected(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for TransformModeError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformTask {
    pub action: TransformAction,
    pub selected_text: String,
    pub target_language: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformPrompt {
    pub system_prompt: String,
    pub user_content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransformExecutionResult {
    pub text: String,
    pub provider_id: String,
    pub model: String,
}

pub fn build_transform_task(
    action: TransformAction,
    selected_text: &str,
    target_language: Option<String>,
) -> Result<TransformTask, TransformModeError> {
    if selected_text.trim().is_empty() {
        return Err(TransformModeError::MissingSelection);
    }

    let target_language = target_language
        .map(|language| language.trim().to_string())
        .filter(|language| !language.is_empty());

    if action.allows_translation() && target_language.is_none() {
        return Err(TransformModeError::MissingTargetLanguage);
    }

    Ok(TransformTask {
        action,
        selected_text: selected_text.to_string(),
        target_language,
    })
}

pub fn build_transform_prompt(task: &TransformTask) -> TransformPrompt {
    let mut system_lines = vec![
        "You transform selected text for Verbatim.".to_string(),
        task.action.instruction(task.target_language.as_deref()),
        "Return only the transformed text.".to_string(),
        "Do not add facts, explanations, greetings, signoffs, or new content.".to_string(),
        "Preserve names, code, numbers, URLs, emails, and mixed-language text.".to_string(),
    ];

    if !task.action.allows_translation() {
        system_lines.push("Do not translate any text.".to_string());
        system_lines
            .push("Preserve every language and script already present in the input.".to_string());
    }

    TransformPrompt {
        system_prompt: system_lines.join("\n"),
        user_content: task.selected_text.clone(),
    }
}

pub fn validate_transform_output(
    task: &TransformTask,
    output: &str,
) -> Result<(), TransformModeError> {
    if task.action.allows_translation() {
        return Ok(());
    }

    crate::text_processing::validate_preserved_text(&task.selected_text, output)
        .map_err(|err| TransformModeError::OutputRejected(format!("transform output failed {err}")))
}

pub async fn execute_transform_task(
    app: &AppHandle,
    settings: &crate::settings::AppSettings,
    task: &TransformTask,
    operation_token: Option<&crate::operation_cancellation::OperationToken>,
) -> Result<TransformExecutionResult, TransformModeError> {
    let prompt = build_transform_prompt(task);
    let runtime = crate::runtime_settings::text_processing_provider_runtime(
        settings,
        crate::runtime_settings::TextProcessingIntent::ExplicitTransform,
    );

    if runtime.uses_managed_local() {
        return execute_with_managed_local_llm(app, settings, task, &prompt, operation_token).await;
    }

    let api_runtime = runtime
        .api_provider()
        .cloned()
        .ok_or_else(|| transform_runtime_error(settings, runtime.skip_reason()))?;

    execute_with_configured_provider(api_runtime, task, &prompt, operation_token).await
}

fn ensure_transform_not_cancelled(
    operation_token: Option<&crate::operation_cancellation::OperationToken>,
    stage: &str,
) -> Result<(), TransformModeError> {
    if operation_token.is_some_and(|token| token.is_cancelled()) {
        return Err(TransformModeError::ProviderUnavailable(format!(
            "Transform cancelled before {stage}"
        )));
    }

    Ok(())
}

fn transform_runtime_error(
    settings: &crate::settings::AppSettings,
    reason: Option<crate::runtime_settings::TextProcessingSkipReason>,
) -> TransformModeError {
    let provider_id = settings.post_process_provider_id.clone();

    match reason {
        Some(crate::runtime_settings::TextProcessingSkipReason::MissingProvider) => {
            TransformModeError::MissingProvider
        }
        Some(crate::runtime_settings::TextProcessingSkipReason::MissingModel) => {
            TransformModeError::MissingModel(provider_id)
        }
        Some(crate::runtime_settings::TextProcessingSkipReason::RemoteMissingApiKey) => {
            TransformModeError::MissingApiKey(provider_id)
        }
        Some(crate::runtime_settings::TextProcessingSkipReason::NotRequested) => {
            TransformModeError::ProviderUnavailable(
                "Transform was not requested by runtime settings".to_string(),
            )
        }
        Some(crate::runtime_settings::TextProcessingSkipReason::DisabledBySettings) => {
            TransformModeError::ProviderUnavailable(
                "Transform provider is disabled by runtime settings".to_string(),
            )
        }
        None => TransformModeError::MissingProvider,
    }
}

async fn execute_with_managed_local_llm(
    app: &AppHandle,
    settings: &crate::settings::AppSettings,
    task: &TransformTask,
    prompt: &TransformPrompt,
    operation_token: Option<&crate::operation_cancellation::OperationToken>,
) -> Result<TransformExecutionResult, TransformModeError> {
    let Some(manager) = app.try_state::<Arc<crate::local_llm::download::LocalLlmManager>>() else {
        return Err(TransformModeError::ProviderUnavailable(
            "Managed local transform skipped because local LLM manager is unavailable".to_string(),
        ));
    };

    let endpoint = manager
        .ensure_runtime(&settings.local_llm)
        .await
        .map_err(|err| {
            TransformModeError::ProviderUnavailable(format!(
                "Managed local transform runtime is unavailable: {err}"
            ))
        })?;

    debug!(
        "Starting managed local transform with action '{:?}' and model '{}'",
        task.action, endpoint.model_id
    );

    ensure_transform_not_cancelled(operation_token, "managed local request")?;

    let content = crate::llm_client::send_chat_completion_with_schema(
        &endpoint.provider,
        String::new(),
        &endpoint.model,
        prompt.user_content.clone(),
        Some(prompt.system_prompt.clone()),
        None,
        None,
        None,
    )
    .await
    .map_err(|err| {
        TransformModeError::ProviderUnavailable(format!("Managed local transform failed: {err}"))
    })?
    .ok_or_else(|| {
        TransformModeError::ProviderUnavailable(
            "Managed local transform returned no content".to_string(),
        )
    })?;

    let text = crate::text_processing::strip_invisible_chars(&content);
    validate_transform_output(task, &text)?;

    Ok(TransformExecutionResult {
        text,
        provider_id: crate::local_llm::runtime::VERBATIM_LOCAL_PROVIDER_ID.to_string(),
        model: endpoint.model,
    })
}

async fn execute_with_configured_provider(
    runtime: crate::runtime_settings::TextProcessingApiRuntime,
    task: &TransformTask,
    prompt: &TransformPrompt,
    operation_token: Option<&crate::operation_cancellation::OperationToken>,
) -> Result<TransformExecutionResult, TransformModeError> {
    let provider = runtime.provider;
    let model = runtime.model;
    let api_key = runtime.api_key;

    let raw_text = if provider.supports_structured_output {
        let field = "transform";
        let request = crate::text_processing::TextProviderRequest::transform_structured(
            prompt.user_content.clone(),
            Some(prompt.system_prompt.clone()),
        );

        ensure_transform_not_cancelled(operation_token, "structured provider request")?;

        let content =
            crate::text_processing::send_text_provider_request(&provider, api_key, &model, request)
                .await
                .map_err(|err| {
                    TransformModeError::ProviderUnavailable(format!(
                        "Transform failed for provider '{}': {err}",
                        provider.id
                    ))
                })?
                .ok_or_else(|| {
                    TransformModeError::ProviderUnavailable(format!(
                        "Transform provider '{}' returned no content",
                        provider.id
                    ))
                })?;

        structured_transform_text(&content, field)?
    } else {
        let request = crate::text_processing::TextProviderRequest::new(
            prompt.user_content.clone(),
            Some(prompt.system_prompt.clone()),
        );

        ensure_transform_not_cancelled(operation_token, "provider request")?;

        crate::text_processing::send_text_provider_request(&provider, api_key, &model, request)
            .await
            .map_err(|err| {
                TransformModeError::ProviderUnavailable(format!(
                    "Transform failed for provider '{}': {err}",
                    provider.id
                ))
            })?
            .ok_or_else(|| {
                TransformModeError::ProviderUnavailable(format!(
                    "Transform provider '{}' returned no content",
                    provider.id
                ))
            })?
    };

    let text = crate::text_processing::strip_invisible_chars(&raw_text);
    validate_transform_output(task, &text)?;

    Ok(TransformExecutionResult {
        text,
        provider_id: provider.id,
        model,
    })
}

fn structured_transform_text(content: &str, field: &str) -> Result<String, TransformModeError> {
    crate::text_processing::extract_structured_text(content, field)
        .map_err(|err| TransformModeError::OutputRejected(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn transform_settings_for_provider(
        base_url: &str,
        api_key: &str,
    ) -> crate::settings::AppSettings {
        let mut settings = crate::settings::get_default_settings();
        let provider_id = "transform-test-provider".to_string();

        settings.local_llm.enabled = false;
        settings.post_process_provider_id = provider_id.clone();
        settings
            .post_process_models
            .insert(provider_id.clone(), "test-model".to_string());
        settings
            .post_process_providers
            .push(crate::settings::PostProcessProvider {
                id: provider_id.clone(),
                label: "Transform Test Provider".to_string(),
                base_url: base_url.to_string(),
                allow_base_url_edit: true,
                models_endpoint: None,
                supports_structured_output: true,
            });

        if !api_key.is_empty() {
            settings
                .post_process_api_keys
                .insert(provider_id, api_key.to_string());
        }

        settings
    }

    #[test]
    fn cancelled_transform_token_blocks_provider_request_stage() {
        let state = crate::operation_cancellation::OperationCancellationState::default();
        let token = state.begin_operation();
        state.cancel_current_operation();

        let err = ensure_transform_not_cancelled(Some(&token), "provider request")
            .expect_err("cancelled transform must not start provider request");

        assert!(err.to_string().contains("provider request"));
    }

    fn transform_provider_is_available(base_url: &str, api_key: &str) -> bool {
        let settings = transform_settings_for_provider(base_url, api_key);
        crate::runtime_settings::text_processing_provider_runtime(
            &settings,
            crate::runtime_settings::TextProcessingIntent::ExplicitTransform,
        )
        .api_provider()
        .is_some()
    }

    fn transform_provider_skip_reason(
        base_url: &str,
        api_key: &str,
    ) -> Option<crate::runtime_settings::TextProcessingSkipReason> {
        let settings = transform_settings_for_provider(base_url, api_key);
        crate::runtime_settings::text_processing_provider_runtime(
            &settings,
            crate::runtime_settings::TextProcessingIntent::ExplicitTransform,
        )
        .skip_reason()
    }

    #[test]
    fn non_translate_actions_require_selected_text() {
        let err = build_transform_task(TransformAction::Polish, "   ", None)
            .expect_err("empty selection should be rejected");

        assert_eq!(err, TransformModeError::MissingSelection);
    }

    #[test]
    fn translate_action_requires_target_language() {
        let err = build_transform_task(
            TransformAction::TranslateToSelectedLanguage,
            "Bonjour",
            None,
        )
        .expect_err("translation needs an explicit target language");

        assert_eq!(err, TransformModeError::MissingTargetLanguage);
    }

    #[test]
    fn transform_task_preserves_selected_text_exactly_for_history_and_provider_input() {
        let selected = "\n  Hello Khalid, please review this.  \n";
        let task = build_transform_task(TransformAction::Polish, selected, None)
            .expect("valid selection with boundary whitespace");
        let prompt = build_transform_prompt(&task);

        assert_eq!(task.selected_text, selected);
        assert_eq!(prompt.user_content, selected);
    }

    #[test]
    fn non_translate_prompts_forbid_translation_and_new_content() {
        let task = build_transform_task(
            TransformAction::MakeConcise,
            "Hello Khalid, I wanted to ask about the spreadsheet.",
            None,
        )
        .expect("valid task");
        let prompt = build_transform_prompt(&task);

        assert!(prompt.system_prompt.contains("Do not translate"));
        assert!(prompt.system_prompt.contains("Do not add facts"));
        assert!(prompt.system_prompt.contains("Return only"));
        assert_eq!(
            prompt.user_content,
            "Hello Khalid, I wanted to ask about the spreadsheet."
        );
    }

    #[test]
    fn translate_prompt_targets_selected_language_explicitly() {
        let task = build_transform_task(
            TransformAction::TranslateToSelectedLanguage,
            "Can you send the file?",
            Some("Arabic".to_string()),
        )
        .expect("valid translate task");
        let prompt = build_transform_prompt(&task);

        assert!(prompt.system_prompt.contains("Translate"));
        assert!(prompt.system_prompt.contains("Arabic"));
        assert!(prompt.system_prompt.contains("Return only"));
    }

    #[test]
    fn non_translate_output_rejects_unrequested_translation_or_script_loss() {
        let task = build_transform_task(
            TransformAction::Polish,
            "Please send this to خالد today.",
            None,
        )
        .expect("valid task");

        let TransformModeError::OutputRejected(err) =
            validate_transform_output(&task, "Please send this to Khalid today.")
                .expect_err("script loss should be rejected")
        else {
            panic!("expected output rejection");
        };

        assert!(err.contains("preservation checks"));
        assert!(err.contains("LostArabicScript"));
    }

    #[test]
    fn remote_transform_provider_requires_api_key_before_selected_text_egress() {
        assert_eq!(
            transform_provider_skip_reason("https://api.openai.com/v1", ""),
            Some(crate::runtime_settings::TextProcessingSkipReason::RemoteMissingApiKey)
        );
        assert!(transform_provider_is_available(
            "https://api.openai.com/v1",
            "sk-test"
        ));
        assert!(transform_provider_is_available(
            "http://127.0.0.1:1234/v1",
            ""
        ));
        assert_eq!(
            transform_provider_skip_reason("https://localhost.example.com/v1", "",),
            Some(crate::runtime_settings::TextProcessingSkipReason::RemoteMissingApiKey)
        );
    }

    #[test]
    fn structured_transform_response_extracts_transform_field() {
        let result =
            structured_transform_text(r#"{"transform":"Polished selected text"}"#, "transform")
                .expect("valid structured transform");

        assert_eq!(result, "Polished selected text");
    }

    #[test]
    fn shared_structured_text_parser_strips_invisible_characters() {
        let result = crate::text_processing::extract_structured_text(
            "{\"transform\":\"\u{200B}Polished\u{200D} text\u{FEFF}\"}",
            "transform",
        )
        .expect("valid structured transform");

        assert_eq!(result, "Polished text");
    }

    #[test]
    fn structured_transform_response_rejects_invalid_json() {
        let err = structured_transform_text("Polished selected text", "transform")
            .expect_err("structured response must be valid JSON");

        assert!(matches!(err, TransformModeError::OutputRejected(_)));
        assert!(err.to_string().contains("not valid JSON"));
    }

    #[test]
    fn structured_transform_response_rejects_missing_transform_field() {
        let err = structured_transform_text(r#"{"result":"Polished"}"#, "transform")
            .expect_err("structured response must include transform field");

        assert!(matches!(err, TransformModeError::OutputRejected(_)));
        assert!(err.to_string().contains("string 'transform' field"));
    }
}
