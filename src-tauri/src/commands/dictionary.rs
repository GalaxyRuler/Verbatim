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
    // The closure returns the fallible upsert Result as-is; mutate_settings_locked persists
    // the settings regardless of Ok/Err. A failed upsert leaves settings unchanged, so the
    // redundant write-back on the Err path is harmless.
    crate::settings::mutate_settings_locked(&app, |settings| {
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
    // See add_dictionary_entry: the Result is returned as-is; a failed update leaves
    // settings unchanged, so the redundant write-back on the Err path is harmless.
    crate::settings::mutate_settings_locked(&app, |settings| {
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
    crate::settings::mutate_settings_locked(&app, |settings| {
        crate::dictionary::delete_entries(settings, &[id])
    })?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn undo_dictionary_entries(
    app: AppHandle,
    ids: Vec<String>,
) -> Result<Vec<DictionaryEntry>, String> {
    crate::settings::mutate_settings_locked(&app, |settings| {
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
    let now_ms = crate::dictionary::current_unix_ms();
    // Mint a session id local to this command invocation. Running every inferred candidate
    // through `observe_correction` (rather than `upsert_auto_learn_entry` directly) keeps this
    // command on the same provisional-candidate state machine as post-paste learning, so a
    // single correction can no longer mint a permanent entry outright.
    let session = format!("command_{now_ms}");

    let (promoted, learned_count) = crate::settings::mutate_settings_locked(&app, |settings| {
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
        (promoted, learned_count)
    });

    // Emit AFTER the lock is released, mirroring the post-paste learn path, so the
    // review-queue UI refreshes when a first-time correction stages a candidate.
    if learned_count > 0 {
        let _ = app.emit("dictionary-candidates-learned", learned_count);
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
    let entry = crate::settings::mutate_settings_locked(&app, |settings| {
        crate::dictionary::approve_candidate(settings, now_ms, &phrase, replacement_of.as_deref())
    });
    Ok(entry)
}

#[tauri::command]
#[specta::specta]
pub fn reject_learn_candidate(app: AppHandle, phrase: String) -> Result<(), String> {
    crate::settings::mutate_settings_locked(&app, |settings| {
        crate::dictionary::reject_candidate(settings, &phrase);
    });
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_dictionary_entry_active(app: AppHandle, id: String, active: bool) -> Result<(), String> {
    let now_ms = crate::dictionary::current_unix_ms();
    crate::settings::mutate_settings_locked(&app, |settings| {
        crate::dictionary::set_entry_active(settings, now_ms, &id, active)
    })?;
    Ok(())
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
    crate::settings::mutate_settings_locked(&app, |settings| {
        crate::dictionary::reset_dictionary_diagnostics(settings, now_ms);
    });
    Ok(())
}
