use tauri::{AppHandle, command, Runtime};

use crate::models::*;
use crate::Result;
use crate::VerbatimAndroidExt;

#[command]
pub(crate) async fn ping<R: Runtime>(
    app: AppHandle<R>,
    payload: PingRequest,
) -> Result<PingResponse> {
    app.verbatim_android().ping(payload)
}

#[command]
pub(crate) async fn permission_snapshot<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value> {
    app.verbatim_android().permission_snapshot()
}

#[command]
pub(crate) async fn native_transcript_history<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value> {
    app.verbatim_android().native_transcript_history()
}

#[command]
pub(crate) async fn sync_text_formatter<R: Runtime>(
    app: AppHandle<R>,
    snapshot: String,
) -> Result<()> {
    app.verbatim_android().sync_text_formatter(snapshot)
}

#[command]
pub(crate) async fn bubble_corner_snapshot<R: Runtime>(
    app: AppHandle<R>,
) -> Result<serde_json::Value> {
    app.verbatim_android().bubble_corner_snapshot()
}

#[command]
pub(crate) async fn set_bubble_corner<R: Runtime>(
    app: AppHandle<R>,
    corner: String,
) -> Result<serde_json::Value> {
    app.verbatim_android().set_bubble_corner(corner)
}

#[command]
pub(crate) async fn start_bubble<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.verbatim_android().start_bubble()
}

#[command]
pub(crate) async fn stop_bubble<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.verbatim_android().stop_bubble()
}

#[command]
pub(crate) async fn request_microphone<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.verbatim_android().request_microphone()
}

#[command]
pub(crate) async fn open_overlay_settings<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.verbatim_android().open_overlay_settings()
}

#[command]
pub(crate) async fn open_accessibility_settings<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.verbatim_android().open_accessibility_settings()
}

#[command]
pub(crate) async fn request_speech_model_download<R: Runtime>(app: AppHandle<R>) -> Result<()> {
    app.verbatim_android().request_speech_model_download()
}

#[command]
pub(crate) async fn open_external_url<R: Runtime>(
    app: AppHandle<R>,
    url: String,
) -> Result<serde_json::Value> {
    app.verbatim_android().open_external_url(url)
}
