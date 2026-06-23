use crate::actions::process_transcription_output;
use crate::managers::{
    history::{HistoryManager, PaginatedHistory},
    transcription::TranscriptionManager,
};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

fn should_retry_post_process(
    stored_post_process_requested: bool,
    live_post_process_enabled: bool,
) -> bool {
    stored_post_process_requested && live_post_process_enabled
}

fn ensure_retriable_audio_entry(
    entry: &crate::managers::history::HistoryEntry,
) -> Result<(), String> {
    if entry.transform_action.is_some() {
        return Err(
            "Transform history entries do not have recording audio to retranscribe".to_string(),
        );
    }

    if entry.file_name.trim().is_empty() {
        return Err("This history entry does not have recording audio to retranscribe".to_string());
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_history_entries(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    cursor: Option<i64>,
    limit: Option<usize>,
) -> Result<PaginatedHistory, String> {
    history_manager
        .get_history_entries(cursor, limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_history_entry_saved(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i64,
) -> Result<(), String> {
    history_manager
        .toggle_saved_status(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_audio_file_path(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    file_name: String,
) -> Result<String, String> {
    let path = history_manager.get_audio_file_path(&file_name);
    path.to_str()
        .ok_or_else(|| "Invalid file path".to_string())
        .map(|s| s.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_history_entry(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i64,
) -> Result<(), String> {
    history_manager
        .delete_entry(id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn clear_history(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<usize, String> {
    history_manager
        .clear_history()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn clear_recordings(
    _app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<usize, String> {
    history_manager
        .clear_unsaved_recordings()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn retry_history_entry_transcription(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    transcription_manager: State<'_, Arc<TranscriptionManager>>,
    id: i64,
) -> Result<(), String> {
    if crate::private_session::is_enabled(&app) {
        return Err("History retry is disabled while Private Session is on".to_string());
    }

    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {} not found", id))?;
    ensure_retriable_audio_entry(&entry)?;

    let audio_path = history_manager.get_audio_file_path(&entry.file_name);
    let samples = crate::audio_toolkit::read_wav_samples(&audio_path)
        .map_err(|e| format!("Failed to load audio: {}", e))?;

    if samples.is_empty() {
        return Err("Recording has no audio samples".to_string());
    }

    let operation_token = app
        .try_state::<crate::operation_cancellation::OperationCancellationState>()
        .map(|state| state.begin_operation());
    transcription_manager.initiate_model_load();

    let tm = Arc::clone(&transcription_manager);
    let provider_cancellation = operation_token
        .as_ref()
        .map(crate::operation_cancellation::OperationToken::provider_cancellation)
        .unwrap_or_default();
    let transcription = tauri::async_runtime::spawn_blocking(move || {
        tm.transcribe_with_cancellation(samples, provider_cancellation)
    })
    .await
    .map_err(|e| format!("Transcription task panicked: {}", e))?
    .map_err(|e| e.to_string())?;

    if transcription.is_empty() {
        return Err("Recording contains no speech".to_string());
    }

    ensure_retry_not_cancelled(operation_token.as_ref(), "post-processing")?;

    let settings = crate::settings::get_settings(&app);
    let retry_post_process =
        should_retry_post_process(entry.post_process_requested, settings.post_process_enabled);
    let processed = process_transcription_output(
        &app,
        &transcription,
        retry_post_process,
        operation_token.clone(),
    )
    .await;

    ensure_retry_not_cancelled(operation_token.as_ref(), "history update")?;

    history_manager
        .update_transcription(
            id,
            transcription,
            processed.post_processed_text,
            processed.post_process_prompt,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn ensure_retry_not_cancelled(
    operation_token: Option<&crate::operation_cancellation::OperationToken>,
    stage: &str,
) -> Result<(), String> {
    if operation_token.is_some_and(|token| token.is_cancelled()) {
        return Err(format!("History retry cancelled before {stage}"));
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn update_history_limit(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    limit: usize,
) -> Result<(), String> {
    crate::settings::write_settings_domain(
        &app,
        crate::settings::SettingsWriteDomain::Privacy,
        |settings| {
            settings.history_limit = limit;
        },
    )?;

    history_manager
        .cleanup_old_entries()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn update_history_enabled(
    app: AppHandle,
    _history_manager: State<'_, Arc<HistoryManager>>,
    enabled: bool,
) -> Result<(), String> {
    crate::settings::write_settings_domain(
        &app,
        crate::settings::SettingsWriteDomain::Privacy,
        |settings| {
            settings.history_enabled = enabled;
        },
    )?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn update_recordings_enabled(
    app: AppHandle,
    _history_manager: State<'_, Arc<HistoryManager>>,
    enabled: bool,
) -> Result<(), String> {
    crate::settings::write_settings_domain(
        &app,
        crate::settings::SettingsWriteDomain::Privacy,
        |settings| {
            settings.recordings_enabled = enabled;
        },
    )?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn update_recording_retention_period(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    period: String,
) -> Result<(), String> {
    use crate::settings::RecordingRetentionPeriod;

    let retention_period = match period.as_str() {
        "never" => RecordingRetentionPeriod::Never,
        "preserve_limit" => RecordingRetentionPeriod::PreserveLimit,
        "days3" => RecordingRetentionPeriod::Days3,
        "weeks2" => RecordingRetentionPeriod::Weeks2,
        "months3" => RecordingRetentionPeriod::Months3,
        _ => return Err(format!("Invalid retention period: {}", period)),
    };

    crate::settings::write_settings_domain(
        &app,
        crate::settings::SettingsWriteDomain::Privacy,
        |settings| {
            settings.recording_retention_period = retention_period;
        },
    )?;

    history_manager
        .cleanup_old_entries()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_post_processing_requires_stored_request_and_live_toggle() {
        assert!(should_retry_post_process(true, true));
        assert!(!should_retry_post_process(true, false));
        assert!(!should_retry_post_process(false, true));
        assert!(!should_retry_post_process(false, false));
    }

    #[test]
    fn retry_transcription_rejects_transform_history_entries() {
        let entry = crate::managers::history::HistoryEntry {
            id: 1,
            file_name: "transform-1.txt".to_string(),
            timestamp: 1,
            saved: false,
            title: "Transform".to_string(),
            transcription_text: "original".to_string(),
            post_processed_text: Some("result".to_string()),
            post_process_prompt: None,
            post_process_requested: false,
            adaptive_profile_id: None,
            adaptive_profile_name: None,
            adaptive_routing_json: None,
            adaptive_context_json: None,
            adaptive_language_json: None,
            adaptive_insertion_json: None,
            adaptive_parent_entry_id: None,
            transform_action: Some("polish".to_string()),
            transform_original_text: Some("original".to_string()),
            transform_result_text: Some("result".to_string()),
            transform_target_language: None,
            transform_provider_id: Some("verbatim_local".to_string()),
            transform_model: Some("model.gguf".to_string()),
            transform_recovery_status: Some("replaced".to_string()),
        };

        let err = ensure_retriable_audio_entry(&entry).expect_err("transform row cannot retry");

        assert!(err.contains("do not have recording audio"));
    }

    #[test]
    fn retry_transcription_rejects_text_only_history_entries() {
        let entry = crate::managers::history::HistoryEntry {
            id: 1,
            file_name: String::new(),
            timestamp: 1,
            saved: false,
            title: "Text only".to_string(),
            transcription_text: "dictation".to_string(),
            post_processed_text: None,
            post_process_prompt: None,
            post_process_requested: false,
            adaptive_profile_id: None,
            adaptive_profile_name: None,
            adaptive_routing_json: None,
            adaptive_context_json: None,
            adaptive_language_json: None,
            adaptive_insertion_json: None,
            adaptive_parent_entry_id: None,
            transform_action: None,
            transform_original_text: None,
            transform_result_text: None,
            transform_target_language: None,
            transform_provider_id: None,
            transform_model: None,
            transform_recovery_status: None,
        };

        let err = ensure_retriable_audio_entry(&entry).expect_err("text-only row cannot retry");

        assert!(err.contains("does not have recording audio"));
    }

    #[test]
    fn cancelled_retry_token_blocks_history_update_stage() {
        let state = crate::operation_cancellation::OperationCancellationState::default();
        let token = state.begin_operation();
        state.cancel_current_operation();

        let err = ensure_retry_not_cancelled(Some(&token), "history update")
            .expect_err("cancelled retry must block side effect");

        assert!(err.contains("history update"));
    }
}
