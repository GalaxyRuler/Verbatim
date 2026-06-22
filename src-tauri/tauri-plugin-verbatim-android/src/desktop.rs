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

  pub fn native_transcript_history(&self) -> crate::Result<serde_json::Value> {
    Ok(serde_json::json!({ "json": "[]" }))
  }

  pub fn sync_text_formatter(&self, _snapshot: String) -> crate::Result<()> {
    Ok(())
  }

  pub fn bubble_corner_snapshot(&self) -> crate::Result<serde_json::Value> {
    Ok(serde_json::json!({ "value": "top-right" }))
  }

  pub fn set_bubble_corner(&self, corner: String) -> crate::Result<serde_json::Value> {
    Ok(serde_json::json!({ "value": corner }))
  }

  pub fn start_bubble(&self) -> crate::Result<()> {
    Ok(())
  }

  pub fn stop_bubble(&self) -> crate::Result<()> {
    Ok(())
  }

  pub fn request_microphone(&self) -> crate::Result<()> {
    Ok(())
  }

  pub fn open_overlay_settings(&self) -> crate::Result<()> {
    Ok(())
  }

  pub fn open_accessibility_settings(&self) -> crate::Result<()> {
    Ok(())
  }

  pub fn request_speech_model_download(&self) -> crate::Result<()> {
    Ok(())
  }

  pub fn open_external_url(&self, _url: String) -> crate::Result<serde_json::Value> {
    Ok(serde_json::json!({ "value": false }))
  }
}
