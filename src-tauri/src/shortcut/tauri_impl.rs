//! Tauri global-shortcut implementation
//!
//! This module provides shortcut functionality using Tauri's built-in
//! global-shortcut plugin.

use log::{error, warn};
use tauri::{AppHandle, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[cfg(not(target_os = "linux"))]
use crate::settings::get_settings;
use crate::settings::{self, is_unbound_shortcut, AppSettings, ShortcutBinding};

use super::{emit_shortcut_registration_failure, handler::handle_shortcut_event};

/// Initialize shortcuts using Tauri's global-shortcut plugin
pub fn init_shortcuts(app: &AppHandle) {
    let user_settings = settings::load_or_create_app_settings(app);
    init_shortcuts_with_registration(app, &user_settings, |binding| {
        register_shortcut(app, binding)
    });
}

fn init_shortcuts_with_registration<R: Runtime>(
    app: &AppHandle<R>,
    user_settings: &AppSettings,
    mut register: impl FnMut(ShortcutBinding) -> Result<(), String>,
) {
    let default_bindings = settings::get_default_settings().bindings;

    // Register all default shortcuts, applying user customizations
    for (id, default_binding) in default_bindings {
        if id == "cancel" {
            continue; // Skip cancel shortcut, it will be registered dynamically
        }
        // Skip post-processing shortcut when the feature is disabled
        if id == "transcribe_with_post_process" && !user_settings.post_process_enabled {
            continue;
        }
        let binding = user_settings
            .bindings
            .get(&id)
            .cloned()
            .unwrap_or(default_binding);
        let hotkey = binding.current_binding.clone();

        if let Err(e) = register(binding) {
            error!("Failed to register shortcut {} during init: {}", id, e);
            emit_shortcut_registration_failure(app, &hotkey);
        }
    }
}

/// Validate a shortcut string for the Tauri global-shortcut implementation.
/// Tauri requires at least one non-modifier key and doesn't support the fn key.
pub fn validate_shortcut(raw: &str) -> Result<(), String> {
    if is_unbound_shortcut(raw) {
        return Ok(());
    }

    let modifiers = [
        "ctrl", "control", "shift", "alt", "option", "meta", "command", "cmd", "super", "win",
        "windows",
    ];

    // Check for fn key which Tauri doesn't support
    let parts: Vec<String> = raw.split('+').map(|p| p.trim().to_lowercase()).collect();
    for part in &parts {
        if part == "fn" || part == "function" {
            return Err("The 'fn' key is not supported by Tauri global shortcuts".into());
        }
    }

    // Check for at least one non-modifier key
    let has_non_modifier = parts.iter().any(|part| !modifiers.contains(&part.as_str()));

    if has_non_modifier {
        Ok(())
    } else {
        Err("Tauri shortcuts must include a main key (letter, number, F-key, etc.) in addition to modifiers".into())
    }
}

/// Register a shortcut using Tauri's global-shortcut plugin
pub fn register_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    if is_unbound_shortcut(&binding.current_binding) {
        return Ok(());
    }

    // Validate for Tauri requirements
    if let Err(e) = validate_shortcut(&binding.current_binding) {
        warn!(
            "register_tauri_shortcut validation error for binding '{}': {}",
            binding.current_binding, e
        );
        return Err(e);
    }

    // Parse shortcut and return error if it fails
    let shortcut = match binding.current_binding.parse::<Shortcut>() {
        Ok(s) => s,
        Err(e) => {
            let error_msg = format!(
                "Failed to parse shortcut '{}': {}",
                binding.current_binding, e
            );
            error!("register_tauri_shortcut parse error: {}", error_msg);
            return Err(error_msg);
        }
    };

    // Prevent duplicate registrations that would silently shadow one another
    if app.global_shortcut().is_registered(shortcut) {
        let error_msg = format!("Shortcut '{}' is already in use", binding.current_binding);
        warn!("register_tauri_shortcut duplicate error: {}", error_msg);
        return Err(error_msg);
    }

    // Clone binding.id for use in the closure
    let binding_id_for_closure = binding.id.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |app_handle, scut, event| {
            if scut == &shortcut {
                let shortcut_string = scut.into_string();
                let is_pressed = event.state == ShortcutState::Pressed;
                handle_shortcut_event(
                    app_handle,
                    &binding_id_for_closure,
                    &shortcut_string,
                    is_pressed,
                );
            }
        })
        .map_err(|e| {
            let error_msg = format!(
                "Couldn't register shortcut '{}': {}",
                binding.current_binding, e
            );
            error!("register_tauri_shortcut registration error: {}", error_msg);
            error_msg
        })?;

    Ok(())
}

/// Unregister a shortcut from Tauri's global-shortcut plugin
pub fn unregister_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    if is_unbound_shortcut(&binding.current_binding) {
        return Ok(());
    }

    let shortcut = match binding.current_binding.parse::<Shortcut>() {
        Ok(s) => s,
        Err(e) => {
            let error_msg = format!(
                "Failed to parse shortcut '{}' for unregistration: {}",
                binding.current_binding, e
            );
            error!("unregister_tauri_shortcut parse error: {}", error_msg);
            return Err(error_msg);
        }
    };

    app.global_shortcut().unregister(shortcut).map_err(|e| {
        let error_msg = format!(
            "Failed to unregister shortcut '{}': {}",
            binding.current_binding, e
        );
        error!("unregister_tauri_shortcut error: {}", error_msg);
        error_msg
    })?;

    Ok(())
}

/// Register the cancel shortcut (called when recording starts)
pub fn register_cancel_shortcut(app: &AppHandle) {
    // Cancel shortcut is disabled on Linux due to instability with dynamic shortcut registration
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let cancel_binding = get_settings(app).bindings.get("cancel").cloned();
        let registration_app = app.clone();
        let _ = register_cancel_shortcut_with_registration(
            app.clone(),
            cancel_binding,
            move |binding| register_shortcut(&registration_app, binding),
        );
    }
}

#[cfg(not(target_os = "linux"))]
fn register_cancel_shortcut_with_registration<R: Runtime>(
    app: AppHandle<R>,
    cancel_binding: Option<ShortcutBinding>,
    register: impl FnOnce(ShortcutBinding) -> Result<(), String> + Send + 'static,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        if let Some(cancel_binding) = cancel_binding {
            let hotkey = cancel_binding.current_binding.clone();
            if let Err(e) = register(cancel_binding) {
                error!("Failed to register cancel shortcut: {}", e);
                emit_shortcut_registration_failure(&app, &hotkey);
            }
        }
    })
}

/// Unregister the cancel shortcut (called when recording stops)
pub fn unregister_cancel_shortcut(app: &AppHandle) {
    // Cancel shortcut is disabled on Linux due to instability with dynamic shortcut registration
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        return;
    }

    #[cfg(not(target_os = "linux"))]
    {
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Some(cancel_binding) = get_settings(&app_clone).bindings.get("cancel").cloned() {
                // We ignore errors here as it might already be unregistered
                let _ = unregister_shortcut(&app_clone, cancel_binding);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tauri::Listener;

    fn event_payloads(app: &tauri::App<tauri::test::MockRuntime>) -> Arc<Mutex<Vec<String>>> {
        let payloads = Arc::new(Mutex::new(Vec::new()));
        let payloads_for_listener = Arc::clone(&payloads);
        app.handle().listen("recording-error", move |event| {
            payloads_for_listener
                .lock()
                .expect("lock event payloads")
                .push(event.payload().to_string());
        });
        payloads
    }

    fn assert_registration_failure(payloads: &Mutex<Vec<String>>, hotkey: &str) {
        let payloads = payloads.lock().expect("lock collected payloads");
        assert_eq!(payloads.len(), 1);
        let payload = serde_json::from_str::<serde_json::Value>(&payloads[0])
            .expect("recording-error payload is JSON");
        assert_eq!(payload["error_type"], "shortcut_registration_failed");
        assert_eq!(payload["detail"], hotkey);
    }

    #[test]
    fn init_shortcuts_reports_the_failed_registration_hotkey() {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build shortcut event test app");
        let payloads = event_payloads(&app);
        let mut settings = settings::get_default_settings();
        for binding in settings.bindings.values_mut() {
            binding.current_binding.clear();
        }
        settings
            .bindings
            .get_mut("transcribe")
            .unwrap()
            .current_binding = "ctrl+space".into();

        init_shortcuts_with_registration(app.handle(), &settings, |binding| {
            if is_unbound_shortcut(&binding.current_binding) {
                Ok(())
            } else {
                Err("injected".into())
            }
        });

        assert_registration_failure(&payloads, "ctrl+space");
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn register_cancel_shortcut_reports_the_failed_registration_hotkey() {
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("build shortcut event test app");
        let payloads = event_payloads(&app);
        let cancel_binding = settings::get_default_settings().bindings.remove("cancel");

        let _ = tauri::async_runtime::block_on(register_cancel_shortcut_with_registration(
            app.handle().clone(),
            cancel_binding,
            |_| Err("injected".into()),
        ));

        assert_registration_failure(&payloads, "escape");
    }
}
