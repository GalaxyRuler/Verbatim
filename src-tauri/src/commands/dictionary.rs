use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

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
    let mut settings = crate::settings::get_settings(&app);
    let entry = crate::dictionary::upsert_manual_entry(
        &mut settings,
        crate::dictionary::current_unix_ms(),
        input.phrase,
        input.replacement_of,
    )?;
    crate::settings::write_settings(&app, settings);
    Ok(entry)
}

#[tauri::command]
#[specta::specta]
pub fn update_dictionary_entry(
    app: AppHandle,
    id: String,
    update: DictionaryEntryUpdate,
) -> Result<DictionaryEntry, String> {
    let mut settings = crate::settings::get_settings(&app);
    let entry = crate::dictionary::update_entry(
        &mut settings,
        crate::dictionary::current_unix_ms(),
        &id,
        update.phrase,
        update.replacement_of,
        update.priority,
    )?;
    crate::settings::write_settings(&app, settings);
    Ok(entry)
}

#[tauri::command]
#[specta::specta]
pub fn delete_dictionary_entry(app: AppHandle, id: String) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    crate::dictionary::delete_entries(&mut settings, &[id]);
    crate::settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn undo_dictionary_entries(
    app: AppHandle,
    ids: Vec<String>,
) -> Result<Vec<DictionaryEntry>, String> {
    let mut settings = crate::settings::get_settings(&app);
    let deleted = crate::dictionary::delete_entries(&mut settings, &ids);
    crate::settings::write_settings(&app, settings);
    Ok(deleted)
}

#[tauri::command]
#[specta::specta]
pub fn learn_custom_words_from_correction(
    app: AppHandle,
    dictated_text: String,
    corrected_text: String,
) -> Result<Vec<String>, String> {
    let mut settings = crate::settings::get_settings(&app);
    let candidates = crate::dictionary_learning::infer_auto_learn_candidates(
        &dictated_text,
        &corrected_text,
        &settings.custom_words,
    );

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let mut learned_entries = Vec::new();
    let now_ms = crate::dictionary::current_unix_ms();
    for candidate in candidates {
        if let Some(entry) = crate::dictionary::upsert_auto_learn_entry(
            &mut settings,
            now_ms,
            candidate.phrase,
            candidate.replacement_of,
        )? {
            learned_entries.push(entry);
        }
    }

    if learned_entries.is_empty() {
        return Ok(Vec::new());
    }

    crate::settings::write_settings(&app, settings);

    Ok(learned_entries
        .into_iter()
        .map(|entry| entry.phrase)
        .collect())
}
