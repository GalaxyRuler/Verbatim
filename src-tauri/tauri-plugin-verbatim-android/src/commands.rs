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
