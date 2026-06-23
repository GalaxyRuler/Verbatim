use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};
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
pub async fn copy_last_transform_result(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<bool, String> {
    let Some(entry) = history_manager
        .get_latest_transform_entry()
        .map_err(|err| err.to_string())?
    else {
        return Ok(false);
    };

    let text = entry
        .transform_result_text
        .as_deref()
        .or(entry.post_processed_text.as_deref())
        .unwrap_or("");
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
    let (sender, receiver) = std::sync::mpsc::channel();

    app.run_on_main_thread(move || {
        let mut insertion_transaction = crate::insertion::InsertionTransaction::new(|request| {
            crate::clipboard::paste_with_receipt_with_auto_learn(
                request.text,
                app_for_paste.clone(),
                request.target_verified,
                request.auto_learn_eligible,
            )
        });
        let outcome = insertion_transaction.run(
            crate::insertion::InsertionAttempt::paste_last_transcript(text),
        );
        if !outcome.receipt.succeeded {
            log::error!(
                "Failed to paste last transcript: {:?}",
                outcome.receipt.error.as_deref()
            );
        }
        let _ = sender.send(outcome);
    })
    .map_err(|err| err.to_string())?;

    let outcome = receiver.recv().map_err(|err| err.to_string())?;
    if let Some(recovery) = &outcome.recovery_copy {
        crate::clipboard::copy_text_for_recovery(&app, &recovery.text, recovery.reason)?;
    }
    if outcome.emit_paste_error {
        if let Some(recovery_event) = outcome.paste_recovery_event() {
            let _ = app.emit("paste-error", recovery_event);
        }
    }

    if outcome.receipt.succeeded {
        Ok(true)
    } else {
        Err(outcome
            .receipt
            .error
            .unwrap_or_else(|| "Failed to paste last transcript".to_string()))
    }
}
