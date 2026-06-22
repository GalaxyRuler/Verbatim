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
}
