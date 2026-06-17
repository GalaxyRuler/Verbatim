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
    let mut settings = crate::settings::get_settings(&app);
    let entry = crate::snippets::upsert_snippet_entry(
        &mut settings,
        crate::snippets::current_unix_ms(),
        input.trigger,
        input.content,
    )?;
    crate::settings::write_settings(&app, settings);
    Ok(entry)
}

#[tauri::command]
#[specta::specta]
pub fn update_snippet_entry(
    app: AppHandle,
    id: String,
    update: SnippetEntryUpdate,
) -> Result<SnippetEntry, String> {
    let mut settings = crate::settings::get_settings(&app);
    let entry = crate::snippets::update_snippet_entry(
        &mut settings,
        crate::snippets::current_unix_ms(),
        &id,
        update.trigger,
        update.content,
    )?;
    crate::settings::write_settings(&app, settings);
    Ok(entry)
}

#[tauri::command]
#[specta::specta]
pub fn delete_snippet_entry(app: AppHandle, id: String) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    crate::snippets::delete_snippet_entries(&mut settings, &[id]);
    crate::settings::write_settings(&app, settings);
    Ok(())
}
