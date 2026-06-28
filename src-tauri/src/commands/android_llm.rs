use crate::asr::llm_models::{AndroidLlmModelManager, AndroidLlmModelPackState};
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
#[specta::specta]
pub fn llm_list_model_packs(
    app: AppHandle,
    manager: State<'_, AndroidLlmModelManager>,
) -> Result<Vec<AndroidLlmModelPackState>, String> {
    manager
        .list_for_app(&app)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn llm_get_model_pack_state(
    app: AppHandle,
    manager: State<'_, AndroidLlmModelManager>,
    model_id: String,
) -> Result<AndroidLlmModelPackState, String> {
    manager
        .pack_state_for_app(&app, &model_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn llm_download_model_pack(
    app: AppHandle,
    manager: State<'_, AndroidLlmModelManager>,
    model_id: String,
) -> Result<(), String> {
    let result = manager
        .download_pack(&app, &model_id)
        .await
        .map_err(|error| error.to_string());

    if let Err(ref error) = result {
        let _ = app.emit(
            "android-llm-model-failed",
            serde_json::json!({ "modelId": &model_id, "error": error }),
        );
    }

    result
}

#[tauri::command]
#[specta::specta]
pub fn llm_cancel_model_download(
    manager: State<'_, AndroidLlmModelManager>,
    model_id: String,
) -> Result<(), String> {
    manager
        .cancel_download(&model_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn llm_select_model_pack(
    app: AppHandle,
    manager: State<'_, AndroidLlmModelManager>,
    model_id: String,
) -> Result<AndroidLlmModelPackState, String> {
    manager
        .select_pack(&app, &model_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn llm_delete_model_pack(
    app: AppHandle,
    manager: State<'_, AndroidLlmModelManager>,
    model_id: String,
) -> Result<(), String> {
    manager
        .delete_pack(&app, &model_id)
        .map_err(|error| error.to_string())
}
