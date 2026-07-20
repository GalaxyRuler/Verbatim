use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter};

use crate::settings::{DictionaryEntry, DictionaryEntryPriority};

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct DictionaryEntryInput {
    pub phrase: String,
    #[serde(default)]
    pub replacement_of: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct DictionaryEntryUpdate {
    #[serde(default)]
    pub phrase: Option<String>,
    #[serde(default)]
    pub replacement_of: Option<Option<String>>,
    #[serde(default)]
    pub priority: Option<DictionaryEntryPriority>,
}

#[tauri::command]
#[specta::specta]
pub fn list_dictionary_entries(app: AppHandle) -> Result<Vec<DictionaryEntry>, String> {
    let settings = crate::settings::get_settings(&app);
    Ok(settings.dictionary_entries)
}

#[tauri::command]
#[specta::specta]
pub fn add_dictionary_entry(
    app: AppHandle,
    input: DictionaryEntryInput,
) -> Result<DictionaryEntry, String> {
    let now_ms = crate::dictionary::current_unix_ms();
    crate::settings::try_mutate_settings_locked_and_save(&app, |settings| {
        crate::dictionary::upsert_manual_entry(settings, now_ms, input.phrase, input.replacement_of)
    })
}

#[tauri::command]
#[specta::specta]
pub fn update_dictionary_entry(
    app: AppHandle,
    id: String,
    update: DictionaryEntryUpdate,
) -> Result<DictionaryEntry, String> {
    let now_ms = crate::dictionary::current_unix_ms();
    crate::settings::try_mutate_settings_locked_and_save(&app, |settings| {
        crate::dictionary::update_entry(
            settings,
            now_ms,
            &id,
            update.phrase,
            update.replacement_of,
            update.priority,
        )
    })
}

#[tauri::command]
#[specta::specta]
pub fn delete_dictionary_entry(app: AppHandle, id: String) -> Result<(), String> {
    crate::settings::try_mutate_settings_locked_and_save(&app, |settings| {
        crate::dictionary::delete_entries(settings, &[id]).map(|_| ())
    })
}

#[tauri::command]
#[specta::specta]
pub fn undo_dictionary_entries(
    app: AppHandle,
    ids: Vec<String>,
) -> Result<Vec<DictionaryEntry>, String> {
    crate::settings::try_mutate_settings_locked_and_save(&app, |settings| {
        crate::dictionary::delete_entries(settings, &ids)
    })
}

#[tauri::command]
#[specta::specta]
pub fn learn_custom_words_from_correction(
    app: AppHandle,
    dictated_text: String,
    corrected_text: String,
) -> Result<Vec<String>, String> {
    learn_custom_words_from_correction_with_app(&app, dictated_text, corrected_text)
}

fn learn_custom_words_from_correction_with_app<R: tauri::Runtime>(
    app: &AppHandle<R>,
    dictated_text: String,
    corrected_text: String,
) -> Result<Vec<String>, String> {
    let now_ms = crate::dictionary::current_unix_ms();
    // Mint a session id local to this command invocation. Running every inferred candidate
    // through `observe_correction` (rather than `upsert_auto_learn_entry` directly) keeps this
    // command on the same provisional-candidate state machine as post-paste learning, so a
    // single correction can no longer mint a permanent entry outright.
    let session = format!("command_{now_ms}");

    let persisted = crate::settings::try_mutate_settings_locked_and_save(app, |settings| {
        let candidates = crate::dictionary_learning::infer_auto_learn_candidates(
            &dictated_text,
            &corrected_text,
            &settings.custom_words,
        );

        let mut promoted = Vec::new();
        let mut learned_count = 0usize;
        for candidate in candidates {
            let dictated = candidate.replacement_of.as_deref().unwrap_or("");
            match crate::dictionary::observe_correction(
                settings,
                now_ms,
                &session,
                dictated,
                Some(&candidate.phrase),
            ) {
                crate::dictionary::ObserveOutcome::Promoted => {
                    // Promotion pushes the new entry last onto `dictionary_entries` (see
                    // `promote_candidate_to_entry`), so grabbing `.last()` here is safe.
                    if let Some(entry) = settings.dictionary_entries.last() {
                        promoted.push(entry.phrase.clone());
                    }
                }
                crate::dictionary::ObserveOutcome::Learned => learned_count += 1,
                _ => {}
            }
        }
        Ok((promoted, learned_count))
    });

    finish_learn_command_after_persist(persisted, |learned_count| {
        let _ = app.emit("dictionary-candidates-learned", learned_count);
    })
}

fn finish_learn_command_after_persist(
    persisted: Result<(Vec<String>, usize), String>,
    emit_candidates_learned: impl FnOnce(usize),
) -> Result<Vec<String>, String> {
    let (promoted, learned_count) = persisted?;
    if learned_count > 0 {
        emit_candidates_learned(learned_count);
    }
    Ok(promoted)
}

#[tauri::command]
#[specta::specta]
pub fn list_learn_candidates(
    app: AppHandle,
) -> Result<Vec<crate::settings::LearnCandidate>, String> {
    Ok(crate::settings::get_settings(&app).dictionary_learn_candidates)
}

#[tauri::command]
#[specta::specta]
pub fn approve_learn_candidate(
    app: AppHandle,
    phrase: String,
    replacement_of: Option<String>,
) -> Result<Option<DictionaryEntry>, String> {
    let now_ms = crate::dictionary::current_unix_ms();
    crate::settings::try_mutate_settings_locked_and_save(&app, |settings| {
        Ok(crate::dictionary::approve_candidate(
            settings,
            now_ms,
            &phrase,
            replacement_of.as_deref(),
        ))
    })
}

#[tauri::command]
#[specta::specta]
pub fn reject_learn_candidate(app: AppHandle, phrase: String) -> Result<(), String> {
    crate::settings::try_mutate_settings_locked_and_save(&app, |settings| {
        crate::dictionary::reject_candidate(settings, &phrase);
        Ok(())
    })
}

#[tauri::command]
#[specta::specta]
pub fn set_dictionary_entry_active(app: AppHandle, id: String, active: bool) -> Result<(), String> {
    let now_ms = crate::dictionary::current_unix_ms();
    crate::settings::try_mutate_settings_locked_and_save(&app, |settings| {
        crate::dictionary::set_entry_active(settings, now_ms, &id, active).map(|_| ())
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_dictionary_diagnostics(
    app: AppHandle,
) -> Result<crate::settings::DictionaryDiagnostics, String> {
    Ok(crate::settings::get_settings(&app).dictionary_diagnostics)
}

#[tauri::command]
#[specta::specta]
pub fn reset_dictionary_diagnostics(app: AppHandle) -> Result<(), String> {
    let now_ms = crate::dictionary::current_unix_ms();
    crate::settings::try_mutate_settings_locked_and_save(&app, |settings| {
        crate::dictionary::reset_dictionary_diagnostics(settings, now_ms);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::{finish_learn_command_after_persist, learn_custom_words_from_correction_with_app};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tauri::Listener;
    use tauri_plugin_store::StoreExt;

    struct TestPathCleanup(PathBuf);

    impl Drop for TestPathCleanup {
        fn drop(&mut self) {
            if self.0.is_dir() {
                let _ = std::fs::remove_dir_all(&self.0);
            } else {
                let _ = std::fs::remove_file(&self.0);
            }
        }
    }

    #[test]
    fn failed_durable_learn_returns_error_without_emitting_event() {
        let emitted = AtomicUsize::new(0);

        let result = finish_learn_command_after_persist(
            Err("atomically persist settings: forced failure".to_string()),
            |_| {
                emitted.fetch_add(1, Ordering::SeqCst);
            },
        );

        assert_eq!(
            result.expect_err("persistence failure must reach the command caller"),
            "atomically persist settings: forced failure"
        );
        assert_eq!(emitted.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn learn_command_save_failure_rolls_back_and_emits_no_event() {
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after Unix epoch")
            .as_nanos();
        context.config_mut().identifier = format!(
            "com.galaxyruler.verbatim.dictionary-command-test.{}.{}",
            std::process::id(),
            unique
        );
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_store::Builder::new().build())
            .build(context)
            .expect("build isolated command test app");
        let app_data_dir = crate::portable::app_data_dir(app.handle())
            .expect("resolve isolated app data directory");
        let _cleanup = TestPathCleanup(app_data_dir.clone());
        let store = app
            .store_builder(PathBuf::from(crate::settings::SETTINGS_STORE_PATH))
            .disable_auto_save()
            .build()
            .expect("build cached settings store");
        store.set(
            "settings",
            serde_json::to_value(crate::settings::get_default_settings())
                .expect("serialize original settings"),
        );
        assert!(
            !crate::dictionary_learning::infer_auto_learn_candidates(
                "meet robin.",
                "meet Robyn.",
                &[],
            )
            .is_empty(),
            "fixture must reach the learned-candidate event path"
        );

        let emitted = Arc::new(AtomicUsize::new(0));
        let emitted_for_listener = Arc::clone(&emitted);
        let _listener = app.listen("dictionary-candidates-learned", move |_| {
            emitted_for_listener.fetch_add(1, Ordering::SeqCst);
        });

        if app_data_dir.exists() {
            std::fs::remove_dir_all(&app_data_dir).expect("remove test app data directory");
        }
        std::fs::write(&app_data_dir, "block settings directory")
            .expect("replace app data directory with a file");

        let error = learn_custom_words_from_correction_with_app(
            app.handle(),
            "meet robin.".to_string(),
            "meet Robyn.".to_string(),
        )
        .expect_err("forced settings save failure must reach the command caller");

        assert!(error.contains("atomically persist settings"));
        let cached: crate::settings::AppSettings = serde_json::from_value(
            store
                .get("settings")
                .expect("original settings remain cached"),
        )
        .expect("cached settings deserialize");
        assert!(cached.dictionary_entries.is_empty());
        assert!(cached.dictionary_learn_candidates.is_empty());
        assert_eq!(emitted.load(Ordering::SeqCst), 0);
    }
}
