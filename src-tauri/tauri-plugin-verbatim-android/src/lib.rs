use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::VerbatimAndroid;
#[cfg(mobile)]
use mobile::VerbatimAndroid;

/// Extensions to [`tauri::App`], [`tauri::AppHandle`] and [`tauri::Window`] to access the verbatim-android APIs.
pub trait VerbatimAndroidExt<R: Runtime> {
  fn verbatim_android(&self) -> &VerbatimAndroid<R>;
}

impl<R: Runtime, T: Manager<R>> crate::VerbatimAndroidExt<R> for T {
  fn verbatim_android(&self) -> &VerbatimAndroid<R> {
    self.state::<VerbatimAndroid<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("verbatim-android")
    .invoke_handler(tauri::generate_handler![
      commands::ping,
      commands::permission_snapshot
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let verbatim_android = mobile::init(app, api)?;
      #[cfg(desktop)]
      let verbatim_android = desktop::init(app, api)?;
      app.manage(verbatim_android);
      Ok(())
    })
    .build()
}
