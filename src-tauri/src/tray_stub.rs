use crate::managers::history::HistoryEntry;
use tauri::AppHandle;

#[derive(Clone, Debug, PartialEq)]
pub enum TrayIconState {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum AppTheme {
    Dark,
    Light,
    Colored,
}

#[allow(dead_code)]
pub fn get_current_theme(_app: &AppHandle) -> AppTheme {
    AppTheme::Dark
}

#[allow(dead_code)]
pub fn get_icon_path(_theme: AppTheme, _state: TrayIconState) -> &'static str {
    "resources/tray_idle.png"
}

pub fn change_tray_icon(_app: &AppHandle, _icon: TrayIconState) {}

#[allow(dead_code)]
pub fn tray_tooltip() -> String {
    format!("Verbatim v{}", env!("CARGO_PKG_VERSION"))
}

pub fn update_tray_menu(_app: &AppHandle, _state: &TrayIconState, _locale: Option<&str>) {}

pub(crate) fn last_transcript_text(entry: &HistoryEntry) -> &str {
    entry
        .post_processed_text
        .as_deref()
        .unwrap_or(&entry.transcription_text)
}

pub fn set_tray_visibility(_app: &AppHandle, _visible: bool) {}

#[allow(dead_code)]
pub fn copy_last_transcript(_app: &AppHandle) {}
