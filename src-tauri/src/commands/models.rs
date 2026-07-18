use crate::managers::model::{ModelInfo, ModelManager};
use crate::managers::transcription::{ModelStateEvent, TranscriptionManager};
use crate::settings::{
    get_settings, write_settings_domain, AppSettings, DictationLanguageMode, ModelUnloadTimeout,
    SettingsWriteDomain,
};
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelSwitchReason {
    LanguageLockClearedForModel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct ModelSwitchOutcome {
    pub reason: Option<ModelSwitchReason>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelSwitchSnapshot {
    selected_model: String,
    selected_language: String,
    dictation_language_mode: DictationLanguageMode,
    adaptive_language_shortlist: Vec<String>,
}

fn begin_model_switch(
    settings: &mut AppSettings,
    model_id: &str,
    supported_languages: &[String],
) -> (ModelSwitchSnapshot, ModelSwitchOutcome) {
    let snapshot = ModelSwitchSnapshot {
        selected_model: settings.selected_model.clone(),
        selected_language: settings.selected_language.clone(),
        dictation_language_mode: settings.dictation_language_mode,
        adaptive_language_shortlist: settings.adaptive_language_shortlist.clone(),
    };
    settings.selected_model = model_id.to_string();

    let reason = if settings.selected_language != "auto"
        && !supported_languages.is_empty()
        && !supported_languages.contains(&settings.selected_language)
    {
        log::info!(
            "Resetting language from '{}' to 'auto' (not supported by {})",
            settings.selected_language,
            model_id
        );
        settings.clear_dictation_language_lock();
        Some(ModelSwitchReason::LanguageLockClearedForModel)
    } else {
        None
    };

    (snapshot, ModelSwitchOutcome { reason })
}

fn restore_model_switch(settings: &mut AppSettings, snapshot: ModelSwitchSnapshot) {
    settings.selected_model = snapshot.selected_model;
    settings.selected_language = snapshot.selected_language;
    settings.dictation_language_mode = snapshot.dictation_language_mode;
    settings.adaptive_language_shortlist = snapshot.adaptive_language_shortlist;
}

#[tauri::command]
#[specta::specta]
pub async fn get_available_models(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<Vec<ModelInfo>, String> {
    Ok(model_manager.get_available_models())
}

#[tauri::command]
#[specta::specta]
pub async fn get_model_info(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<Option<ModelInfo>, String> {
    Ok(model_manager.get_model_info(&model_id))
}

#[tauri::command]
#[specta::specta]
pub async fn download_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    let result = model_manager
        .download_model(&model_id)
        .await
        .map_err(|e| e.to_string());

    if let Err(ref error) = result {
        let _ = app_handle.emit(
            "model-download-failed",
            serde_json::json!({ "model_id": &model_id, "error": error }),
        );
    }

    result
}

#[tauri::command]
#[specta::specta]
pub async fn delete_model(
    app_handle: AppHandle,
    model_manager: State<'_, Arc<ModelManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<(), String> {
    // If deleting the active model, unload it and clear the setting
    let settings = get_settings(&app_handle);
    if settings.selected_model == model_id {
        transcription_manager
            .unload_model()
            .map_err(|e| format!("Failed to unload model: {}", e))?;

        write_settings_domain(&app_handle, SettingsWriteDomain::Models, |settings| {
            if settings.selected_model == model_id {
                settings.selected_model = String::new();
            }
        })?;
    }

    model_manager
        .delete_model(&model_id)
        .map_err(|e| e.to_string())
}

/// Shared logic for switching the active model, used by both the Tauri command
/// and the tray menu handler.
///
/// Validates the model, updates the persisted setting, and loads the model
/// unless the unload timeout is set to "Immediately" (in which case the model
/// will be loaded on-demand during the next transcription).
pub fn switch_active_model(app: &AppHandle, model_id: &str) -> Result<ModelSwitchOutcome, String> {
    let model_manager = app.state::<Arc<ModelManager>>();
    let transcription_manager = app.state::<Arc<TranscriptionManager>>();

    // Atomically claim the loading slot — prevents concurrent model loads
    // from tray double-clicks or overlapping commands. The guard resets the
    // flag on drop (including early returns, errors, and panics).
    let _loading_guard = transcription_manager
        .try_start_loading()
        .ok_or_else(|| "Model load already in progress".to_string())?;

    // Check if model exists and is available
    let model_info = model_manager
        .get_model_info(model_id)
        .ok_or_else(|| format!("Model not found: {}", model_id))?;

    if !model_info.is_downloaded {
        return Err(format!("Model not downloaded: {}", model_id));
    }

    // Persist the new selection early so the frontend sees the correct model
    // when it reacts to events emitted by load_model.
    let mut transition = None;
    write_settings_domain(app, SettingsWriteDomain::Models, |settings| {
        let unload_timeout = settings.model_unload_timeout;
        let (snapshot, outcome) = begin_model_switch(
            settings,
            model_id,
            model_info.supported_languages.as_slice(),
        );
        transition = Some((unload_timeout, snapshot, outcome));
    })?;
    let (unload_timeout, snapshot, outcome) =
        transition.ok_or_else(|| "Model switch settings transition did not run".to_string())?;

    // Skip eager loading if unload is set to "Immediately" — the model
    // will be loaded on-demand during the next transcription.
    if unload_timeout == ModelUnloadTimeout::Immediately {
        // Notify frontend — load_model won't be called so no events
        // would otherwise be emitted.
        let _ = app.emit(
            "model-state-changed",
            ModelStateEvent {
                event_type: "selection_changed".to_string(),
                model_id: Some(model_id.to_string()),
                model_name: Some(model_info.name.clone()),
                error: None,
                diagnostic_code: None,
                fallback: None,
            },
        );
        log::info!(
            "Model selection changed to {} (not loading — unload set to Immediately).",
            model_id
        );
        return Ok(outcome);
    }

    // Load the model. On failure, revert the persisted selection.
    if let Err(e) = transcription_manager.load_model(model_id) {
        write_settings_domain(app, SettingsWriteDomain::Models, |settings| {
            restore_model_switch(settings, snapshot);
        })
        .map_err(|rollback_error| {
            format!("{e}; failed to restore model and language settings: {rollback_error}")
        })?;
        return Err(e.to_string());
    }

    Ok(outcome)
}

#[tauri::command]
#[specta::specta]
pub async fn set_active_model(
    app_handle: AppHandle,
    _model_manager: State<'_, Arc<ModelManager>>,
    _transcription_manager: State<'_, Arc<TranscriptionManager>>,
    model_id: String,
) -> Result<ModelSwitchOutcome, String> {
    switch_active_model(&app_handle, &model_id)
}

#[tauri::command]
#[specta::specta]
pub async fn get_current_model(app_handle: AppHandle) -> Result<String, String> {
    let settings = get_settings(&app_handle);
    Ok(settings.selected_model)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcription_model_status(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<Option<String>, String> {
    Ok(transcription_manager.get_current_model())
}

#[tauri::command]
#[specta::specta]
pub async fn is_model_loading(
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
) -> Result<bool, String> {
    // Check if transcription manager has a loaded model
    let current_model = transcription_manager.get_current_model();
    Ok(current_model.is_none())
}

#[tauri::command]
#[specta::specta]
pub async fn has_any_models_available(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    let models = model_manager.get_available_models();
    Ok(models.iter().any(|m| m.is_downloaded))
}

#[tauri::command]
#[specta::specta]
pub async fn has_any_models_or_downloads(
    model_manager: State<'_, Arc<ModelManager>>,
) -> Result<bool, String> {
    let models = model_manager.get_available_models();
    // Return true if any models are downloaded OR if any downloads are in progress
    Ok(models.iter().any(|m| m.is_downloaded))
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_download(
    model_manager: State<'_, Arc<ModelManager>>,
    model_id: String,
) -> Result<(), String> {
    model_manager
        .cancel_download(&model_id)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{get_default_settings, DictationLanguageMode};

    fn language_tuple(
        settings: &crate::settings::AppSettings,
    ) -> (String, DictationLanguageMode, Vec<String>) {
        (
            settings.selected_language.clone(),
            settings.dictation_language_mode,
            settings.adaptive_language_shortlist.clone(),
        )
    }

    #[test]
    fn failed_model_load_restores_model_and_full_language_tuple() {
        let mut settings = get_default_settings();
        settings.selected_model = "old-model".to_string();
        settings.selected_language = "zh-Hans".to_string();
        settings.dictation_language_mode = DictationLanguageMode::Single;
        settings.adaptive_language_shortlist = vec!["zh-Hans".to_string(), "en-US".to_string()];
        let before_language = language_tuple(&settings);

        let (snapshot, _) = begin_model_switch(&mut settings, "english-model", &["en".to_string()]);
        restore_model_switch(&mut settings, snapshot);

        assert_eq!(settings.selected_model, "old-model");
        assert_eq!(language_tuple(&settings), before_language);
    }

    #[test]
    fn incompatible_model_switch_clears_language_tuple_and_reports_reason() {
        let mut settings = get_default_settings();
        settings.selected_model = "old-model".to_string();
        settings.selected_language = "ar".to_string();
        settings.dictation_language_mode = DictationLanguageMode::Single;
        settings.adaptive_language_shortlist = vec!["ar".to_string(), "en".to_string()];

        let (_, outcome) = begin_model_switch(&mut settings, "english-model", &["en".to_string()]);

        assert_eq!(settings.selected_model, "english-model");
        assert_eq!(settings.selected_language, "auto");
        assert_eq!(
            settings.dictation_language_mode,
            DictationLanguageMode::Auto
        );
        assert_eq!(
            settings.adaptive_language_shortlist,
            vec!["en".to_string(), "ar".to_string()]
        );
        assert_eq!(
            outcome.reason,
            Some(ModelSwitchReason::LanguageLockClearedForModel)
        );
        assert_eq!(
            serde_json::to_value(outcome.reason).expect("serialize model-switch reason"),
            serde_json::json!("language_lock_cleared_for_model")
        );
    }

    #[test]
    fn compatible_model_switch_preserves_language_tuple() {
        let mut settings = get_default_settings();
        settings.selected_language = "pt-BR".to_string();
        settings.dictation_language_mode = DictationLanguageMode::Single;
        settings.adaptive_language_shortlist = vec!["pt-BR".to_string(), "ar".to_string()];
        let before_language = language_tuple(&settings);

        let (_, outcome) = begin_model_switch(
            &mut settings,
            "multilingual-model",
            &["en".to_string(), "pt-BR".to_string(), "ar".to_string()],
        );

        assert_eq!(language_tuple(&settings), before_language);
        assert_eq!(outcome.reason, None);
    }
}
