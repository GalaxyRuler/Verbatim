use crate::adaptive::profile::AdaptiveProfile;
use crate::managers::history::{AdaptiveHistoryMetadata, HistoryEntry, HistoryManager};
use crate::settings::{get_settings, try_write_settings_domain, SettingsWriteDomain};
use std::sync::Arc;
use tauri::{AppHandle, State};

const PRIVATE_SESSION_REPROCESS_ERROR: &str = "private_session_active";

struct ReprocessedAdaptiveEntry {
    file_name: String,
    raw_text: String,
    post_process_requested: bool,
    post_processed_text: Option<String>,
    post_process_prompt: Option<String>,
    metadata: AdaptiveHistoryMetadata,
}

fn select_reprocess_profile(
    entry: &HistoryEntry,
    settings: &crate::settings::AppSettings,
    profile_id: Option<String>,
) -> Result<AdaptiveProfile, String> {
    let selected_profile_id = profile_id
        .or(entry.adaptive_profile_id.clone())
        .unwrap_or_else(|| settings.adaptive_default_profile_id.clone());

    settings
        .adaptive_profiles
        .iter()
        .find(|profile| profile.id == selected_profile_id && profile.enabled)
        .cloned()
        .ok_or_else(|| {
            format!(
                "Adaptive profile '{}' is unavailable; reprocess cannot preserve its semantics",
                selected_profile_id
            )
        })
}

fn build_reprocessed_adaptive_entry(
    entry: &HistoryEntry,
    profile: &AdaptiveProfile,
    processed: crate::actions::ProcessedTranscription,
) -> ReprocessedAdaptiveEntry {
    ReprocessedAdaptiveEntry {
        file_name: entry.file_name.clone(),
        raw_text: entry.transcription_text.clone(),
        post_process_requested: entry.post_process_requested,
        post_processed_text: processed.post_processed_text,
        post_process_prompt: processed.post_process_prompt,
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
    }
}

async fn reprocess_last_adaptive_entry_with<Load, LoadFuture, Process, ProcessFuture, Save>(
    private_session_enabled: bool,
    profile_id: Option<String>,
    load_latest: Load,
    process: Process,
    save: Save,
) -> Result<(), String>
where
    Load: FnOnce() -> LoadFuture,
    LoadFuture: std::future::Future<
        Output = Result<(Option<HistoryEntry>, crate::settings::AppSettings), String>,
    >,
    Process: FnOnce(HistoryEntry, crate::settings::AppSettings, AdaptiveProfile) -> ProcessFuture,
    ProcessFuture:
        std::future::Future<Output = Result<crate::actions::ProcessedTranscription, String>>,
    Save: FnOnce(ReprocessedAdaptiveEntry) -> Result<(), String>,
{
    if private_session_enabled {
        return Err(PRIVATE_SESSION_REPROCESS_ERROR.to_string());
    }

    let (entry, settings) = load_latest().await?;
    let entry =
        entry.ok_or_else(|| "No adaptive history entry available to reprocess".to_string())?;
    let profile = select_reprocess_profile(&entry, &settings, profile_id)?;
    let processed = process(entry.clone(), settings, profile.clone()).await?;
    let reprocessed = build_reprocessed_adaptive_entry(&entry, &profile, processed);

    save(reprocessed)
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
    try_write_settings_domain(&app, SettingsWriteDomain::Adaptive, |settings| {
        settings.adaptive_correction_memory_enabled = enabled;
        Ok(())
    })
}

#[tauri::command]
#[specta::specta]
pub async fn reprocess_last_adaptive_entry(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    profile_id: Option<String>,
) -> Result<(), String> {
    let process_app = app.clone();
    reprocess_last_adaptive_entry_with(
        crate::private_session::is_enabled(&app),
        profile_id,
        || async {
            let mut settings = get_settings(&app);
            crate::credentials::hydrate_runtime_post_process_api_keys(&app, &mut settings);
            let entry = history_manager
                .get_latest_adaptive_entry()
                .await
                .map_err(|err| err.to_string())?;
            Ok((entry, settings))
        },
        move |entry, settings, profile| async move {
            Ok(
                crate::actions::process_transcription_output_with_profile_on_app(
                    &process_app,
                    &settings,
                    &entry.transcription_text,
                    Some(&profile),
                    None,
                    entry.post_process_requested,
                    false,
                    None,
                )
                .await,
            )
        },
        |reprocessed| {
            history_manager
                .save_entry_with_metadata(
                    reprocessed.file_name,
                    reprocessed.raw_text,
                    reprocessed.post_process_requested,
                    reprocessed.post_processed_text,
                    reprocessed.post_process_prompt,
                    reprocessed.metadata,
                )
                .map(|_| ())
                .map_err(|err| err.to_string())
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::profile::default_profiles;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

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

    fn processed_entry(
        final_text: &str,
        post_processed_text: Option<&str>,
        prompt: Option<&str>,
    ) -> crate::actions::ProcessedTranscription {
        crate::actions::ProcessedTranscription {
            final_text: final_text.to_string(),
            post_processed_text: post_processed_text.map(str::to_string),
            post_process_prompt: prompt.map(str::to_string),
        }
    }

    #[test]
    fn reprocess_helper_preserves_raw_text_and_links_parent() {
        let entry = adaptive_entry();
        let mut settings = crate::settings::get_default_settings();
        settings.adaptive_profiles = default_profiles();
        let profile =
            select_reprocess_profile(&entry, &settings, Some("default_clean".to_string()))
                .expect("profile is available");
        let reprocessed = build_reprocessed_adaptive_entry(
            &entry,
            &profile,
            processed_entry(&entry.transcription_text, None, None),
        );

        assert_eq!(reprocessed.file_name, "verbatim-42.wav");
        assert_eq!(reprocessed.raw_text, entry.transcription_text);
        assert!(reprocessed.post_processed_text.is_none());
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
    fn reprocess_helper_uses_stored_entry_profile() {
        let entry = adaptive_entry();
        let mut settings = crate::settings::get_default_settings();
        settings.adaptive_profiles = default_profiles();
        let profile =
            select_reprocess_profile(&entry, &settings, None).expect("stored profile is available");
        let reprocessed = build_reprocessed_adaptive_entry(
            &entry,
            &profile,
            processed_entry(&entry.transcription_text, None, None),
        );

        assert_eq!(reprocessed.metadata.profile_id.as_deref(), Some("email"));
    }

    #[test]
    fn reprocess_rejects_an_unavailable_stored_profile() {
        let mut entry = adaptive_entry();
        entry.adaptive_profile_id = Some("removed_profile".to_string());
        let settings = crate::settings::get_default_settings();

        let error = select_reprocess_profile(&entry, &settings, None)
            .expect_err("missing profile must not silently change semantics");

        assert!(error.contains("cannot preserve its semantics"));
    }

    #[tokio::test]
    async fn private_session_blocks_reprocess_before_history_query_process_or_save() {
        let query_count = Arc::new(AtomicUsize::new(0));
        let process_count = Arc::new(AtomicUsize::new(0));
        let save_count = Arc::new(AtomicUsize::new(0));
        let query_count_for_load = Arc::clone(&query_count);
        let process_count_for_process = Arc::clone(&process_count);
        let save_count_for_save = Arc::clone(&save_count);

        let result = reprocess_last_adaptive_entry_with(
            true,
            None,
            move || {
                query_count_for_load.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Ok((
                    Some(adaptive_entry()),
                    crate::settings::get_default_settings(),
                )))
            },
            move |entry, _, _| {
                process_count_for_process.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Ok(processed_entry(&entry.transcription_text, None, None)))
            },
            move |_| {
                save_count_for_save.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await;

        assert_eq!(result, Err(PRIVATE_SESSION_REPROCESS_ERROR.to_string()));
        assert_eq!(query_count.load(Ordering::SeqCst), 0);
        assert_eq!(process_count.load(Ordering::SeqCst), 0);
        assert_eq!(save_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn public_session_reprocesses_latest_entry_with_stored_semantics() {
        let query_count = Arc::new(AtomicUsize::new(0));
        let process_count = Arc::new(AtomicUsize::new(0));
        let save_count = Arc::new(AtomicUsize::new(0));
        let query_count_for_load = Arc::clone(&query_count);
        let process_count_for_process = Arc::clone(&process_count);
        let save_count_for_save = Arc::clone(&save_count);

        let result = reprocess_last_adaptive_entry_with(
            false,
            None,
            move || {
                query_count_for_load.fetch_add(1, Ordering::SeqCst);
                std::future::ready(Ok((
                    Some(adaptive_entry()),
                    crate::settings::get_default_settings(),
                )))
            },
            move |entry, _, profile| {
                process_count_for_process.fetch_add(1, Ordering::SeqCst);
                assert_eq!(profile.id, "email");
                assert!(entry.post_process_requested);
                std::future::ready(Ok(processed_entry(
                    "Please send the file today.",
                    Some("Please send the file today."),
                    Some("email prompt"),
                )))
            },
            move |reprocessed| {
                save_count_for_save.fetch_add(1, Ordering::SeqCst);
                assert_eq!(reprocessed.file_name, "verbatim-42.wav");
                assert!(reprocessed.post_process_requested);
                assert_eq!(
                    reprocessed.post_processed_text.as_deref(),
                    Some("Please send the file today.")
                );
                assert_eq!(
                    reprocessed.post_process_prompt.as_deref(),
                    Some("email prompt")
                );
                assert_eq!(reprocessed.metadata.profile_id.as_deref(), Some("email"));
                assert_eq!(reprocessed.metadata.parent_entry_id, Some(42));
                Ok(())
            },
        )
        .await;

        assert_eq!(result, Ok(()));
        assert_eq!(query_count.load(Ordering::SeqCst), 1);
        assert_eq!(process_count.load(Ordering::SeqCst), 1);
        assert_eq!(save_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn reprocess_preserves_stored_post_process_request() {
        let mut entry = adaptive_entry();
        entry.post_process_requested = false;
        let mut settings = crate::settings::get_default_settings();
        settings.adaptive_profiles = default_profiles();
        let profile =
            select_reprocess_profile(&entry, &settings, None).expect("stored profile is available");
        let reprocessed = build_reprocessed_adaptive_entry(
            &entry,
            &profile,
            processed_entry(&entry.transcription_text, None, None),
        );

        assert!(!reprocessed.post_process_requested);
    }
}
