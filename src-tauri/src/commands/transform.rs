use std::future::Future;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::adaptive::types::{InsertionMethod, InsertionReceipt};
use crate::managers::history::HistoryManager;
use crate::selection::{
    SelectionCaptureError, SelectionReplaceError, SelectionReplacementOutcome, SelectionSnapshot,
};
use crate::transform_mode::{self, TransformAction};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TransformCommandStatus {
    Replaced,
    CopiedForRecovery,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct TransformCommandResult {
    pub status: TransformCommandStatus,
    pub history_entry_id: i64,
    pub provider_id: String,
    pub model: String,
}

#[tauri::command]
#[specta::specta]
pub async fn transform_selected_text(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    action: TransformAction,
    target_language: Option<String>,
) -> Result<TransformCommandResult, String> {
    run_transform_selected_text(
        app,
        Arc::clone(history_manager.inner()),
        action,
        target_language,
    )
    .await
}

pub async fn run_transform_selected_text(
    app: AppHandle,
    history_manager: Arc<HistoryManager>,
    action: TransformAction,
    target_language: Option<String>,
) -> Result<TransformCommandResult, String> {
    if crate::private_session::is_enabled(&app) {
        return Err("Text transforms are disabled while Private Session is on".to_string());
    }

    let capture_result = capture_selection_on_main_thread(&app).await;
    run_transform_selected_text_with_executor(capture_result, move |captured| {
        execute_captured_transform(app, history_manager, action, target_language, captured)
    })
    .await
}

async fn run_transform_selected_text_with_executor<E, F>(
    capture_result: Result<SelectionSnapshot, SelectionCaptureError>,
    executor: E,
) -> Result<TransformCommandResult, String>
where
    E: FnOnce(SelectionSnapshot) -> F,
    F: Future<Output = Result<TransformCommandResult, String>>,
{
    let captured = capture_result.map_err(transform_capture_error_for_command)?;
    crate::selection::validate_selected_text_anchor(&captured).map_err(|err| format!("{err:?}"))?;

    executor(captured).await
}

async fn execute_captured_transform(
    app: AppHandle,
    history_manager: Arc<HistoryManager>,
    action: TransformAction,
    target_language: Option<String>,
    captured: SelectionSnapshot,
) -> Result<TransformCommandResult, String> {
    let task = transform_mode::build_transform_task(
        action.clone(),
        &captured.selected_text,
        target_language,
    )
    .map_err(|err| err.to_string())?;
    let operation_token = app
        .try_state::<crate::operation_cancellation::OperationCancellationState>()
        .map(|state| state.begin_operation());
    let mut settings = crate::settings::get_settings(&app);
    crate::credentials::hydrate_runtime_post_process_api_keys(&app, &mut settings);
    let transformed =
        transform_mode::execute_transform_task(&app, &settings, &task, operation_token.as_ref())
            .await
            .map_err(|err| err.to_string())?;

    ensure_transform_not_cancelled(operation_token.as_ref(), "replacement")?;

    let mut should_emit_recovery_copied = false;

    let outcome = replace_selection_with_transaction_on_main_thread(
        &app,
        captured,
        transformed.text.clone(),
        operation_token.clone(),
    )
    .await?;

    let status = if outcome.receipt.succeeded {
        TransformCommandStatus::Replaced
    } else {
        if let Some(recovery) = &outcome.recovery_copy {
            ensure_transform_not_cancelled(operation_token.as_ref(), "recovery copy")?;
            crate::clipboard::copy_text_for_recovery(&app, &recovery.text, recovery.reason)
                .map_err(|err| err.to_string())?;
        }
        if let Some(error) = outcome.receipt.error.as_deref() {
            log::warn!("Transform replacement failed after processing: {error}");
        }
        should_emit_recovery_copied = true;
        TransformCommandStatus::CopiedForRecovery
    };

    ensure_transform_not_cancelled(operation_token.as_ref(), "history save")?;

    let history_entry = history_manager
        .save_transform_entry(
            task.selected_text,
            transformed.text,
            action_id(&action).to_string(),
            task.target_language,
            Some(transformed.provider_id.clone()),
            Some(transformed.model.clone()),
            recovery_status(&status).to_string(),
        )
        .map_err(|err| err.to_string())?;

    if should_emit_recovery_copied {
        emit_transform_recovery_copied(&app);
    }

    Ok(TransformCommandResult {
        status,
        history_entry_id: history_entry.id,
        provider_id: transformed.provider_id,
        model: transformed.model,
    })
}

fn transform_capture_error_for_command(error: SelectionCaptureError) -> String {
    if let SelectionCaptureError::Unavailable(detail) = &error {
        log::warn!("Selected-text capture unavailable: {detail}");
    }

    error.reason_code().to_string()
}

fn ensure_transform_not_cancelled(
    operation_token: Option<&crate::operation_cancellation::OperationToken>,
    stage: &str,
) -> Result<(), String> {
    if operation_token.is_some_and(|token| token.is_cancelled()) {
        return Err(format!("Transform cancelled before {stage}"));
    }

    Ok(())
}

pub fn shortcut_target_language(settings: &crate::settings::AppSettings) -> String {
    settings
        .translation_request
        .as_ref()
        .map(|request| request.target_language.trim())
        .filter(|language| !language.is_empty())
        .unwrap_or("en")
        .to_string()
}

fn emit_transform_recovery_copied(app: &AppHandle) {
    let _ = app.emit("transform-recovery-copied", ());
}

async fn capture_selection_on_main_thread(
    app: &AppHandle,
) -> Result<SelectionSnapshot, SelectionCaptureError> {
    let (sender, receiver) = std::sync::mpsc::channel();

    app.run_on_main_thread(move || {
        let _ = sender.send(crate::selection::capture_current_selection_snapshot());
    })
    .map_err(|err| SelectionCaptureError::Unavailable(err.to_string()))?;

    receiver
        .recv()
        .map_err(|err| SelectionCaptureError::Unavailable(err.to_string()))?
}

async fn replace_selection_with_transaction_on_main_thread(
    app: &AppHandle,
    captured: crate::selection::SelectionSnapshot,
    replacement_text: String,
    operation_token: Option<crate::operation_cancellation::OperationToken>,
) -> Result<crate::insertion::InsertionOutcome, String> {
    let app_for_replace = app.clone();
    let (sender, receiver) = std::sync::mpsc::channel();

    app.run_on_main_thread(move || {
        let mut insertion_transaction = crate::insertion::InsertionTransaction::new(|request| {
            replace_captured_selection_receipt(
                &app_for_replace,
                &captured,
                &request.text,
                operation_token.as_ref(),
            )
        });
        let outcome = insertion_transaction.run(
            crate::insertion::InsertionAttempt::transform_replacement(replacement_text),
        );
        let _ = sender.send(outcome);
    })
    .map_err(|err| err.to_string())?;

    receiver.recv().map_err(|err| err.to_string())
}

fn replace_captured_selection_receipt(
    app: &AppHandle,
    captured: &crate::selection::SelectionSnapshot,
    replacement_text: &str,
    operation_token: Option<&crate::operation_cancellation::OperationToken>,
) -> InsertionReceipt {
    let cancellation_check = || {
        operation_token
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
    };

    match crate::selection::replace_captured_selection_with_cancellation(
        app,
        captured,
        replacement_text,
        Some(&cancellation_check),
    ) {
        Ok(SelectionReplacementOutcome::Replaced(_)) => {
            crate::clipboard::receipt_from_current_paste_method(app, true, Ok(()))
        }
        Ok(SelectionReplacementOutcome::Recoverable(recovery)) => {
            crate::clipboard::receipt_from_current_paste_method(app, true, Err(recovery.reason))
        }
        Err(error) => selection_preflight_failure_receipt(error),
    }
}

fn selection_preflight_failure_receipt(error: SelectionReplaceError) -> InsertionReceipt {
    let target_verified = !matches!(&error, SelectionReplaceError::TargetChanged { .. });

    InsertionReceipt {
        attempted: false,
        succeeded: false,
        method: InsertionMethod::None,
        target_verified,
        error: Some(format!("{error:?}")),
    }
}

fn action_id(action: &TransformAction) -> &'static str {
    match action {
        TransformAction::Polish => "polish",
        TransformAction::MakeConcise => "make_concise",
        TransformAction::TurnIntoList => "turn_into_list",
        TransformAction::TranslateToSelectedLanguage => "translate_to_selected_language",
        TransformAction::PromptEngineer => "prompt_engineer",
    }
}

fn recovery_status(status: &TransformCommandStatus) -> &'static str {
    match status {
        TransformCommandStatus::Replaced => "replaced",
        TransformCommandStatus::CopiedForRecovery => "copied_for_recovery",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn shortcut_target_language_uses_configured_translation_target() {
        let mut settings = crate::settings::get_default_settings();
        settings.translation_request = Some(crate::settings::TranslationRequestSettings {
            source_language: "auto".to_string(),
            target_language: "fr".to_string(),
            route: crate::settings::TranslationRoute::Auto,
        });

        assert_eq!(shortcut_target_language(&settings), "fr");
    }

    #[test]
    fn shortcut_target_language_falls_back_to_english() {
        let settings = crate::settings::get_default_settings();

        assert_eq!(shortcut_target_language(&settings), "en");
    }

    #[test]
    fn cancelled_transform_token_blocks_side_effect_stage() {
        let state = crate::operation_cancellation::OperationCancellationState::default();
        let token = state.begin_operation();
        state.cancel_current_operation();

        let err = ensure_transform_not_cancelled(Some(&token), "history save")
            .expect_err("cancelled transform must block side effect");

        assert!(err.contains("history save"));
    }

    #[test]
    fn secure_capture_errors_stop_executor_before_provider_history_or_mutation() {
        for capture_error in [
            SelectionCaptureError::SecureField,
            SelectionCaptureError::SecureCheckError,
        ] {
            let provider_calls = Cell::new(0);
            let history_writes = Cell::new(0);
            let clipboard_mutations = Cell::new(0);
            let selection_mutations = Cell::new(0);

            let result = tauri::async_runtime::block_on(run_transform_selected_text_with_executor(
                Err(capture_error.clone()),
                |_| {
                    provider_calls.set(provider_calls.get() + 1);
                    history_writes.set(history_writes.get() + 1);
                    clipboard_mutations.set(clipboard_mutations.get() + 1);
                    selection_mutations.set(selection_mutations.get() + 1);
                    std::future::ready(Err::<TransformCommandResult, String>(
                        "injected executor should not run".to_string(),
                    ))
                },
            ));

            assert_eq!(
                result.expect_err("secure capture errors must stop the command"),
                capture_error.reason_code()
            );
            assert_eq!(provider_calls.get(), 0);
            assert_eq!(history_writes.get(), 0);
            assert_eq!(clipboard_mutations.get(), 0);
            assert_eq!(selection_mutations.get(), 0);
        }
    }
}
