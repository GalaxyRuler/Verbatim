use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::snippets::SnippetEntry;

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct SnippetEntryInput {
    pub trigger: String,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct SnippetEntryUpdate {
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn list_snippet_entries(app: AppHandle) -> Result<Vec<SnippetEntry>, String> {
    let settings = crate::settings::get_settings(&app);
    Ok(settings.snippets)
}

#[tauri::command]
#[specta::specta]
pub fn add_snippet_entry(app: AppHandle, input: SnippetEntryInput) -> Result<SnippetEntry, String> {
    let now_ms = crate::snippets::current_unix_ms();
    // The closure returns the fallible upsert Result as-is; a failed upsert leaves settings
    // unchanged, so the redundant write-back on the Err path is harmless (same pattern as
    // commands/dictionary.rs).
    crate::settings::mutate_settings_locked(&app, |settings| {
        crate::snippets::upsert_snippet_entry(settings, now_ms, input.trigger, input.content)
    })
}

#[tauri::command]
#[specta::specta]
pub fn update_snippet_entry(
    app: AppHandle,
    id: String,
    update: SnippetEntryUpdate,
) -> Result<SnippetEntry, String> {
    let now_ms = crate::snippets::current_unix_ms();
    // See add_snippet_entry: a failed update leaves settings unchanged.
    crate::settings::mutate_settings_locked(&app, |settings| {
        crate::snippets::update_snippet_entry(settings, now_ms, &id, update.trigger, update.content)
    })
}

#[tauri::command]
#[specta::specta]
pub fn delete_snippet_entry(app: AppHandle, id: String) -> Result<(), String> {
    crate::settings::mutate_settings_locked(&app, |settings| {
        crate::snippets::delete_snippet_entries(settings, &[id]);
    });
    Ok(())
}
