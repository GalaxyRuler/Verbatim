use crate::adaptive::profile::{find_profile_or_default, AdaptiveProfile};
use crate::managers::history::{AdaptiveHistoryMetadata, HistoryEntry, HistoryManager};
use crate::settings::{get_settings, mutate_settings_locked};
use std::sync::Arc;
use tauri::{AppHandle, State};

struct ReprocessedAdaptiveEntry {
    file_name: String,
    raw_text: String,
    post_processed_text: Option<String>,
    metadata: AdaptiveHistoryMetadata,
}

fn build_reprocessed_adaptive_entry(
    entry: &HistoryEntry,
    profiles: &[AdaptiveProfile],
    default_profile_id: &str,
    profile_id: Option<String>,
) -> Result<ReprocessedAdaptiveEntry, String> {
    let selected_profile_id = profile_id
        .or(entry.adaptive_profile_id.clone())
        .unwrap_or(default_profile_id.to_string());
    let profile = find_profile_or_default(profiles, &selected_profile_id);
    let final_text =
        crate::adaptive::processor::deterministic_process(&entry.transcription_text, profile);
    crate::adaptive::processor::validate_output(&entry.transcription_text, &final_text, profile)?;

    Ok(ReprocessedAdaptiveEntry {
        file_name: entry.file_name.clone(),
        raw_text: entry.transcription_text.clone(),
        post_processed_text: if final_text == entry.transcription_text {
            None
        } else {
            Some(final_text)
        },
        metadata: AdaptiveHistoryMetadata {
            profile_id: Some(profile.id.clone()),
            profile_name: Some(profile.name.clone()),
            routing_json: entry.adaptive_routing_json.clone(),
            context_json: entry
                .adaptive_context_json
                .as_deref()
                .and_then(crate::adaptive::context::redact_context_json_for_history),
            language_json: entry.adaptive_language_json.clone(),
            insertion_json: None,
            parent_entry_id: Some(entry.id),
        },
    })
}

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
    mutate_settings_locked(&app, |settings| {
        settings.adaptive_correction_memory_enabled = enabled;
    });
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

    let reprocessed = build_reprocessed_adaptive_entry(
        &entry,
        &settings.adaptive_profiles,
        &settings.adaptive_default_profile_id,
        profile_id,
    )?;

    history_manager
        .save_entry_with_metadata(
            reprocessed.file_name,
            reprocessed.raw_text,
            true,
            reprocessed.post_processed_text,
            None,
            reprocessed.metadata,
        )
        .map(|_| ())
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::profile::default_profiles;

    fn adaptive_entry() -> HistoryEntry {
        HistoryEntry {
            id: 42,
            file_name: "verbatim-42.wav".to_string(),
            timestamp: 123,
            saved: false,
            title: "now".to_string(),
            transcription_text: "um please send the file today".to_string(),
            post_processed_text: None,
            post_process_prompt: None,
            post_process_requested: true,
            adaptive_profile_id: Some("email".to_string()),
            adaptive_profile_name: Some("Email".to_string()),
            adaptive_routing_json: Some("{\"profile_id\":\"email\"}".to_string()),
            adaptive_context_json: Some(
                serde_json::json!({
                    "captured_at_ms": 1,
                    "process_name": "OUTLOOK.EXE",
                    "window_title": "Inbox - private@example.com - Outlook",
                    "window_title_hash": null,
                    "window_class": "rctrl_renwnd32",
                    "target_kind": "Email",
                    "target_fingerprint": "outlook.exe|rctrl_renwnd32",
                    "is_sensitive": false
                })
                .to_string(),
            ),
            adaptive_language_json: Some("{\"class\":\"MostlyLatin\"}".to_string()),
            adaptive_insertion_json: Some("{\"succeeded\":true}".to_string()),
            adaptive_parent_entry_id: None,
            transform_action: None,
            transform_original_text: None,
            transform_result_text: None,
            transform_target_language: None,
            transform_provider_id: None,
            transform_model: None,
            transform_recovery_status: None,
        }
    }

    #[test]
    fn reprocess_helper_preserves_raw_text_and_links_parent() {
        let entry = adaptive_entry();
        let reprocessed = build_reprocessed_adaptive_entry(
            &entry,
            &default_profiles(),
            "default_clean",
            Some("default_clean".to_string()),
        )
        .expect("reprocess succeeds");

        assert_eq!(reprocessed.file_name, "verbatim-42.wav");
        assert_eq!(reprocessed.raw_text, entry.transcription_text);
        assert_eq!(
            reprocessed.post_processed_text.as_deref(),
            Some("please send the file today")
        );
        assert_eq!(reprocessed.metadata.parent_entry_id, Some(42));
        assert_eq!(
            reprocessed.metadata.profile_id.as_deref(),
            Some("default_clean")
        );
        let context_json = reprocessed
            .metadata
            .context_json
            .as_deref()
            .expect("redacted context metadata");
        assert!(context_json.contains("\"target_kind\":\"Email\""));
        assert!(!context_json.contains("private@example.com"));
        assert!(!context_json.contains("window_title"));
        assert!(reprocessed.metadata.insertion_json.is_none());
    }

    #[test]
    fn reprocess_helper_falls_back_to_entry_profile() {
        let entry = adaptive_entry();
        let reprocessed =
            build_reprocessed_adaptive_entry(&entry, &default_profiles(), "default_clean", None)
                .expect("reprocess succeeds");

        assert_eq!(reprocessed.metadata.profile_id.as_deref(), Some("email"));
    }
}
