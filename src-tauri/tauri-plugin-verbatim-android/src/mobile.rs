use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_verbatim_android);

// initializes the Kotlin or Swift plugin classes
pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<VerbatimAndroid<R>> {
  // The real plugin class lives in the APP module so it can reach the app's services.
  // Resolved by Tauri as "com/galaxyruler/verbatim/VerbatimAndroidPlugin".
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.galaxyruler.verbatim", "VerbatimAndroidPlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_verbatim_android)?;
  Ok(VerbatimAndroid(handle))
}

/// Access to the verbatim-android APIs.
pub struct VerbatimAndroid<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> VerbatimAndroid<R> {
  pub fn ping(&self, payload: PingRequest) -> crate::Result<PingResponse> {
    self
      .0
      .run_mobile_plugin("ping", payload)
      .map_err(Into::into)
  }

  pub fn permission_snapshot(&self) -> crate::Result<serde_json::Value> {
    self
      .0
      .run_mobile_plugin("permissionSnapshot", ())
      .map_err(Into::into)
  }

  pub fn native_transcript_history(&self) -> crate::Result<serde_json::Value> {
    self
      .0
      .run_mobile_plugin("nativeTranscriptHistory", ())
      .map_err(Into::into)
  }

  pub fn delete_history_entry(&self, id: i64) -> crate::Result<serde_json::Value> {
    self
      .0
      .run_mobile_plugin("deleteHistoryEntry", serde_json::json!({ "id": id }))
      .map_err(Into::into)
  }

  pub fn sync_text_formatter(&self, snapshot: String) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>(
        "syncTextFormatter",
        serde_json::json!({ "snapshot": snapshot }),
      )
      .map(|_| ())
      .map_err(Into::into)
  }

  pub fn bubble_corner_snapshot(&self) -> crate::Result<serde_json::Value> {
    self
      .0
      .run_mobile_plugin("bubbleCornerSnapshot", ())
      .map_err(Into::into)
  }

  pub fn set_bubble_corner(&self, corner: String) -> crate::Result<serde_json::Value> {
    self
      .0
      .run_mobile_plugin("setBubbleCorner", serde_json::json!({ "corner": corner }))
      .map_err(Into::into)
  }

  pub fn start_bubble(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("startBubble", ())
      .map(|_| ())
      .map_err(Into::into)
  }

  pub fn stop_bubble(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("stopBubble", ())
      .map(|_| ())
      .map_err(Into::into)
  }

  pub fn request_microphone(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("requestMicrophone", ())
      .map(|_| ())
      .map_err(Into::into)
  }

  pub fn open_overlay_settings(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("openOverlaySettings", ())
      .map(|_| ())
      .map_err(Into::into)
  }

  pub fn open_accessibility_settings(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("openAccessibilitySettings", ())
      .map(|_| ())
      .map_err(Into::into)
  }

  pub fn request_speech_model_download(&self) -> crate::Result<()> {
    self
      .0
      .run_mobile_plugin::<serde_json::Value>("requestSpeechModelDownload", ())
      .map(|_| ())
      .map_err(Into::into)
  }

  pub fn open_external_url(&self, url: String) -> crate::Result<serde_json::Value> {
    self
      .0
      .run_mobile_plugin("openExternalUrl", serde_json::json!({ "url": url }))
      .map_err(Into::into)
  }
}
