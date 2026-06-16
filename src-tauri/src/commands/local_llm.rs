use crate::local_llm::catalog::LocalLlmModelInfo;
use crate::local_llm::download::LocalLlmManager;
use crate::settings::{get_settings, write_settings};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub async fn list_local_llm_models(
    manager: State<'_, Arc<LocalLlmManager>>,
) -> Result<Vec<LocalLlmModelInfo>, String> {
    manager.list_models().map_err(|err| err.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn download_local_llm_model(
    manager: State<'_, Arc<LocalLlmManager>>,
    model_id: String,
) -> Result<(), String> {
    manager
        .download_model(&model_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_local_llm_download(
    manager: State<'_, Arc<LocalLlmManager>>,
    model_id: String,
) -> Result<(), String> {
    manager
        .cancel_download(&model_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_local_llm_model(
    app: AppHandle,
    manager: State<'_, Arc<LocalLlmManager>>,
    model_id: String,
) -> Result<(), String> {
    manager
        .delete_model(&model_id)
        .map_err(|err| err.to_string())?;

    let mut settings = get_settings(&app);
    if settings.local_llm.selected_model_id == model_id {
        settings.local_llm.selected_model_id.clear();
        settings.local_llm.enabled = false;
        write_settings(&app, settings);
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn select_local_llm_model(
    app: AppHandle,
    manager: State<'_, Arc<LocalLlmManager>>,
    model_id: String,
) -> Result<(), String> {
    let model = manager
        .list_models()
        .map_err(|err| err.to_string())?
        .into_iter()
        .find(|model| model.id == model_id)
        .ok_or_else(|| format!("Local LLM model not found: {}", model_id))?;

    if !model.is_downloaded {
        return Err(format!("Local LLM model is not downloaded: {}", model_id));
    }

    let mut settings = get_settings(&app);
    settings.local_llm.selected_model_id = model_id;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn set_local_llm_enabled(
    app: AppHandle,
    manager: State<'_, Arc<LocalLlmManager>>,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = get_settings(&app);

    if enabled {
        let selected_model_id = settings.local_llm.selected_model_id.clone();
        if selected_model_id.trim().is_empty() {
            return Err("Select a downloaded local LLM model first.".to_string());
        }

        let selected_is_downloaded = manager
            .list_models()
            .map_err(|err| err.to_string())?
            .iter()
            .any(|model| model.id == selected_model_id && model.is_downloaded);

        if !selected_is_downloaded {
            return Err(format!(
                "Selected local LLM model is not downloaded: {}",
                selected_model_id
            ));
        }
    }

    settings.local_llm.enabled = enabled;
    write_settings(&app, settings);
    Ok(())
}
