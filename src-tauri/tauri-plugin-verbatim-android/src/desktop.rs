use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<VerbatimAndroid<R>> {
  Ok(VerbatimAndroid(app.clone()))
}

/// Access to the verbatim-android APIs.
pub struct VerbatimAndroid<R: Runtime>(AppHandle<R>);

impl<R: Runtime> VerbatimAndroid<R> {
  pub fn ping(&self, payload: PingRequest) -> crate::Result<PingResponse> {
    Ok(PingResponse {
      value: payload.value,
    })
  }

  pub fn permission_snapshot(&self) -> crate::Result<serde_json::Value> {
    // No native Android surface on desktop; return an empty snapshot.
    Ok(serde_json::json!({}))
  }
}
