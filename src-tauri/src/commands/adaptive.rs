use crate::managers::history::{AdaptiveHistoryMetadata, HistoryManager};
use crate::settings::{get_settings, write_settings};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub fn get_adaptive_profiles(
    app: AppHandle,
) -> Result<Vec<crate::adaptive::profile::AdaptiveProfile>, String> {
    Ok(get_settings(&app).adaptive_profiles)
}

#[tauri::command]
#[specta::specta]
pub fn reset_adaptive_correction_memory(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_adaptive_correction_memory_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.adaptive_correction_memory_enabled = enabled;
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn reprocess_last_adaptive_entry(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    profile_id: Option<String>,
) -> Result<(), String> {
    let settings = get_settings(&app);
    let entry = history_manager
        .get_latest_adaptive_entry()
        .await
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "No adaptive history entry available to reprocess".to_string())?;

    let selected_profile_id = profile_id
        .or(entry.adaptive_profile_id.clone())
        .unwrap_or(settings.adaptive_default_profile_id.clone());
    let profile = crate::adaptive::profile::find_profile_or_default(
        &settings.adaptive_profiles,
        &selected_profile_id,
    );
    let final_text =
        crate::adaptive::processor::deterministic_process(&entry.transcription_text, profile);
    crate::adaptive::processor::validate_output(&entry.transcription_text, &final_text, profile)?;

    history_manager
        .save_entry_with_metadata(
            entry.file_name.clone(),
            entry.transcription_text.clone(),
            true,
            if final_text == entry.transcription_text {
                None
            } else {
                Some(final_text)
            },
            None,
            AdaptiveHistoryMetadata {
                profile_id: Some(profile.id.clone()),
                profile_name: Some(profile.name.clone()),
                routing_json: None,
                context_json: entry.adaptive_context_json.clone(),
                language_json: None,
                insertion_json: None,
                parent_entry_id: Some(entry.id),
            },
        )
        .map(|_| ())
        .map_err(|err| err.to_string())
}
