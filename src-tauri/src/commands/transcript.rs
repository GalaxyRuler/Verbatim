use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::managers::history::HistoryManager;

#[tauri::command]
#[specta::specta]
pub async fn copy_last_transcript(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<bool, String> {
    let Some(entry) = history_manager
        .get_latest_completed_entry()
        .map_err(|err| err.to_string())?
    else {
        return Ok(false);
    };

    let text = crate::tray::last_transcript_text(&entry);
    if text.trim().is_empty() {
        return Ok(false);
    }

    app.clipboard()
        .write_text(text.to_string())
        .map_err(|err| err.to_string())?;
    Ok(true)
}

#[tauri::command]
#[specta::specta]
pub async fn paste_last_transcript(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<bool, String> {
    let Some(entry) = history_manager
        .get_latest_completed_entry()
        .map_err(|err| err.to_string())?
    else {
        return Ok(false);
    };

    let text = crate::tray::last_transcript_text(&entry).to_string();
    if text.trim().is_empty() {
        return Ok(false);
    }

    let app_for_paste = app.clone();
    app.run_on_main_thread(move || {
        if let Err(err) = crate::clipboard::paste(text, app_for_paste) {
            log::error!("Failed to paste last transcript: {}", err);
        }
    })
    .map_err(|err| err.to_string())?;

    Ok(true)
}
