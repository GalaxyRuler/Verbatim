use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, State};

use crate::managers::history::HistoryManager;
use crate::selection::SelectionReplacementOutcome;
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
    let captured = capture_selection_on_main_thread(&app).await?;
    crate::selection::validate_selected_text_anchor(&captured).map_err(|err| format!("{err:?}"))?;
    let task = transform_mode::build_transform_task(
        action.clone(),
        &captured.selected_text,
        target_language,
    )
    .map_err(|err| err.to_string())?;
    let settings = crate::settings::get_settings(&app);
    let transformed = transform_mode::execute_transform_task(&app, &settings, &task)
        .await
        .map_err(|err| err.to_string())?;

    let mut should_emit_recovery_copied = false;

    let status = match replace_selection_on_main_thread(&app, captured, transformed.text.clone())
        .await
    {
        Ok(SelectionReplacementOutcome::Replaced(_)) => TransformCommandStatus::Replaced,
        Ok(SelectionReplacementOutcome::Recoverable(recovery)) => {
            crate::clipboard::copy_text_for_recovery(&app, &recovery.copy_text, &recovery.reason)
                .map_err(|err| err.to_string())?;
            should_emit_recovery_copied = true;
            TransformCommandStatus::CopiedForRecovery
        }
        Err(err) => {
            crate::clipboard::copy_text_for_recovery(
                &app,
                &transformed.text,
                "transform replacement failure",
            )
            .map_err(|copy_err| copy_err.to_string())?;
            log::warn!("Transform replacement failed after processing: {}", err);
            should_emit_recovery_copied = true;
            TransformCommandStatus::CopiedForRecovery
        }
    };

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
) -> Result<crate::selection::SelectionSnapshot, String> {
    let (sender, receiver) = std::sync::mpsc::channel();

    app.run_on_main_thread(move || {
        let _ = sender.send(crate::selection::capture_current_selection_snapshot());
    })
    .map_err(|err| err.to_string())?;

    receiver
        .recv()
        .map_err(|err| err.to_string())?
        .map_err(|err| format!("{err:?}"))
}

async fn replace_selection_on_main_thread(
    app: &AppHandle,
    captured: crate::selection::SelectionSnapshot,
    replacement_text: String,
) -> Result<SelectionReplacementOutcome, String> {
    let app_for_replace = app.clone();
    let (sender, receiver) = std::sync::mpsc::channel();

    app.run_on_main_thread(move || {
        let result = crate::selection::replace_captured_selection(
            &app_for_replace,
            &captured,
            &replacement_text,
        );
        let _ = sender.send(result);
    })
    .map_err(|err| err.to_string())?;

    receiver
        .recv()
        .map_err(|err| err.to_string())?
        .map_err(|err| format!("{err:?}"))
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
}
