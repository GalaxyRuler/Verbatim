use crate::settings::AppSettings;
use keyring::{Entry, Error};
use log::warn;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

const POST_PROCESS_API_KEY_SERVICE: &str = "com.galaxyruler.verbatim.post-process-api-key";
const CREDENTIAL_HEALTH_PROBE_ACCOUNT: &str = "__verbatim_credential_health_probe__";
const CREDENTIAL_HEALTH_PROBE_VALUE: &str = "__verbatim_health_probe__";
pub const STORED_SECRET_PLACEHOLDER: &str = "__VERBATIM_STORED_IN_OS_KEYRING__";

#[derive(Clone, Debug, Serialize, specta::Type)]
pub struct CredentialStoreStatus {
    pub available: bool,
    pub platform: String,
    pub message: Option<String>,
    pub retained_legacy_api_key_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialStoreFailurePolicy {
    PreserveLegacyValue,
    RejectNewValue,
}

#[derive(Debug, Default)]
pub struct SessionCredentialState {
    post_process_api_keys: Mutex<HashMap<String, String>>,
}

impl SessionCredentialState {
    pub fn set_post_process_api_key(&self, provider_id: String, api_key: String) {
        let mut keys = self
            .post_process_api_keys
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        keys.insert(provider_id, api_key);
    }

    pub fn delete_post_process_api_key(&self, provider_id: &str) {
        let mut keys = self
            .post_process_api_keys
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        keys.remove(provider_id);
    }

    fn snapshot(&self) -> HashMap<String, String> {
        self.post_process_api_keys
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
    }
}

fn entry(provider_id: &str) -> Result<Entry, String> {
    Entry::new(POST_PROCESS_API_KEY_SERVICE, provider_id).map_err(|err| {
        format!(
            "Failed to open OS credential store for provider '{}': {}",
            provider_id, err
        )
    })
}

pub fn credential_store_status_for_settings(settings: &AppSettings) -> CredentialStoreStatus {
    credential_store_status_with_retained_legacy_count(retained_legacy_api_key_count(settings))
}

fn credential_store_status_with_retained_legacy_count(
    retained_legacy_api_key_count: usize,
) -> CredentialStoreStatus {
    credential_store_status_from_probe(
        std::env::consts::OS.to_string(),
        probe_credential_store()
            .map_err(|err| format!("OS credential store probe failed: {}", err)),
        retained_legacy_api_key_count,
    )
}

fn probe_credential_store() -> Result<(), String> {
    let entry = entry(CREDENTIAL_HEALTH_PROBE_ACCOUNT)?;
    entry
        .set_password(CREDENTIAL_HEALTH_PROBE_VALUE)
        .map_err(|err| format!("failed to write credential probe: {}", err))?;

    match entry.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("failed to remove credential probe: {}", err)),
    }
}

fn credential_store_status_from_probe(
    platform: String,
    probe_result: Result<(), String>,
    retained_legacy_api_key_count: usize,
) -> CredentialStoreStatus {
    match probe_result {
        Ok(()) => CredentialStoreStatus {
            available: true,
            platform,
            message: None,
            retained_legacy_api_key_count,
        },
        Err(err) => CredentialStoreStatus {
            available: false,
            platform,
            message: Some(err),
            retained_legacy_api_key_count,
        },
    }
}

pub fn retained_legacy_api_key_count(settings: &AppSettings) -> usize {
    settings
        .post_process_api_keys
        .values()
        .filter(|value| {
            let value = value.trim();
            !value.is_empty() && value != STORED_SECRET_PLACEHOLDER
        })
        .count()
}

pub fn get_post_process_api_key(provider_id: &str) -> Result<Option<String>, String> {
    let entry = entry(provider_id)?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(Error::NoEntry) => Ok(None),
        Err(err) => Err(format!(
            "Failed to read OS credential for provider '{}': {}",
            provider_id, err
        )),
    }
}

pub fn set_post_process_api_key(provider_id: &str, api_key: &str) -> Result<(), String> {
    let entry = entry(provider_id)?;
    entry.set_password(api_key).map_err(|err| {
        format!(
            "Failed to write OS credential for provider '{}': {}",
            provider_id, err
        )
    })
}

pub fn delete_post_process_api_key(provider_id: &str) -> Result<(), String> {
    let entry = entry(provider_id)?;
    match entry.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(err) => Err(format!(
            "Failed to delete OS credential for provider '{}': {}",
            provider_id, err
        )),
    }
}

pub fn hydrate_post_process_api_keys(settings: &mut AppSettings) {
    let provider_ids: Vec<String> = settings
        .post_process_providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect();

    for provider_id in provider_ids {
        match get_post_process_api_key(&provider_id) {
            Ok(Some(api_key)) => {
                settings.post_process_api_keys.insert(provider_id, api_key);
            }
            Ok(None) => {
                if settings
                    .post_process_api_keys
                    .get(&provider_id)
                    .is_some_and(|value| value == STORED_SECRET_PLACEHOLDER)
                {
                    settings
                        .post_process_api_keys
                        .insert(provider_id, String::new());
                }
            }
            Err(err) => {
                warn!("{}", err);
            }
        }
    }
}

pub fn hydrate_runtime_post_process_api_keys(app: &AppHandle, settings: &mut AppSettings) {
    hydrate_post_process_api_keys(settings);

    if let Some(session_credentials) = app.try_state::<SessionCredentialState>() {
        for (provider_id, api_key) in session_credentials.snapshot() {
            settings.post_process_api_keys.insert(provider_id, api_key);
        }
    }
}

pub fn redact_post_process_api_keys_for_frontend(settings: &mut AppSettings) {
    let provider_ids: Vec<String> = settings
        .post_process_providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect();

    for provider_id in provider_ids {
        let stored_in_keyring = match get_post_process_api_key(&provider_id) {
            Ok(Some(secret)) => !secret.trim().is_empty(),
            Ok(None) => false,
            Err(err) => {
                warn!("{}", err);
                false
            }
        };

        let has_current_value = !settings
            .post_process_api_keys
            .get(&provider_id)
            .map(String::as_str)
            .unwrap_or_default()
            .trim()
            .is_empty();

        settings.post_process_api_keys.insert(
            provider_id,
            if stored_in_keyring || has_current_value {
                STORED_SECRET_PLACEHOLDER.to_string()
            } else {
                String::new()
            },
        );
    }
}

pub fn redact_session_post_process_api_keys_for_frontend(
    app: &AppHandle,
    settings: &mut AppSettings,
) {
    let Some(session_credentials) = app.try_state::<SessionCredentialState>() else {
        return;
    };

    for (provider_id, api_key) in session_credentials.snapshot() {
        if !api_key.trim().is_empty() {
            settings
                .post_process_api_keys
                .insert(provider_id, STORED_SECRET_PLACEHOLDER.to_string());
        }
    }
}

pub fn prepare_post_process_api_keys_for_store(
    settings: &mut AppSettings,
    failure_policy: CredentialStoreFailurePolicy,
) -> bool {
    prepare_post_process_api_keys_for_store_with_writer(
        settings,
        failure_policy,
        |provider, key| set_post_process_api_key(provider, key),
    )
}

fn prepare_post_process_api_keys_for_store_with_writer<W>(
    settings: &mut AppSettings,
    failure_policy: CredentialStoreFailurePolicy,
    mut write_api_key: W,
) -> bool
where
    W: FnMut(&str, &str) -> Result<(), String>,
{
    let mut changed = false;
    let provider_ids: Vec<String> = settings.post_process_api_keys.keys().cloned().collect();

    for provider_id in provider_ids {
        let Some(api_key) = settings.post_process_api_keys.get(&provider_id).cloned() else {
            continue;
        };

        if api_key.is_empty() {
            continue;
        }

        if api_key == STORED_SECRET_PLACEHOLDER {
            settings
                .post_process_api_keys
                .insert(provider_id, String::new());
            changed = true;
            continue;
        }

        match write_api_key(&provider_id, &api_key) {
            Ok(()) => {
                settings
                    .post_process_api_keys
                    .insert(provider_id, String::new());
                changed = true;
            }
            Err(err) => match failure_policy {
                CredentialStoreFailurePolicy::PreserveLegacyValue => {
                    warn!(
                        "{}; retaining legacy settings value to avoid dropping the user's API key",
                        err
                    );
                }
                CredentialStoreFailurePolicy::RejectNewValue => {
                    warn!(
                            "{}; clearing rejected API key value instead of persisting it in app settings",
                            err
                        );
                    settings
                        .post_process_api_keys
                        .insert(provider_id, String::new());
                    changed = true;
                }
            },
        }
    }

    changed
}

pub fn has_stored_post_process_api_key(settings: &AppSettings, provider_id: &str) -> bool {
    settings
        .post_process_api_keys
        .get(provider_id)
        .is_some_and(|value| !value.trim().is_empty() && value != STORED_SECRET_PLACEHOLDER)
        || get_post_process_api_key(provider_id)
            .ok()
            .flatten()
            .is_some_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_secret_placeholder_is_removed_before_settings_persist() {
        let mut settings = crate::settings::get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), STORED_SECRET_PLACEHOLDER.to_string());

        assert!(prepare_post_process_api_keys_for_store(
            &mut settings,
            CredentialStoreFailurePolicy::RejectNewValue
        ));
        assert_eq!(
            settings
                .post_process_api_keys
                .get("openai")
                .map(String::as_str),
            Some("")
        );
    }

    #[test]
    fn migration_policy_preserves_legacy_api_key_when_credential_write_fails() {
        let mut settings = crate::settings::get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), "legacy-key".to_string());

        assert!(!prepare_post_process_api_keys_for_store_with_writer(
            &mut settings,
            CredentialStoreFailurePolicy::PreserveLegacyValue,
            |_provider, _api_key| Err("credential store unavailable".to_string())
        ));
        assert_eq!(
            settings
                .post_process_api_keys
                .get("openai")
                .map(String::as_str),
            Some("legacy-key")
        );
    }

    #[test]
    fn retained_legacy_api_key_count_ignores_empty_and_placeholder_values() {
        let mut settings = crate::settings::get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), "legacy-key".to_string());
        settings
            .post_process_api_keys
            .insert("anthropic".to_string(), String::new());
        settings
            .post_process_api_keys
            .insert("groq".to_string(), STORED_SECRET_PLACEHOLDER.to_string());

        assert_eq!(retained_legacy_api_key_count(&settings), 1);
    }

    #[test]
    fn strict_policy_rejects_new_api_key_when_credential_write_fails() {
        let mut settings = crate::settings::get_default_settings();
        settings
            .post_process_api_keys
            .insert("openai".to_string(), "new-key".to_string());

        assert!(prepare_post_process_api_keys_for_store_with_writer(
            &mut settings,
            CredentialStoreFailurePolicy::RejectNewValue,
            |_provider, _api_key| Err("credential store unavailable".to_string())
        ));
        assert_eq!(
            settings
                .post_process_api_keys
                .get("openai")
                .map(String::as_str),
            Some("")
        );
    }

    #[test]
    fn credential_store_status_reports_probe_failure_without_secret_values() {
        let status = credential_store_status_from_probe(
            "linux".to_string(),
            Err("Secret Service is unavailable".to_string()),
            0,
        );

        assert!(!status.available);
        assert_eq!(status.platform, "linux");
        let message = status.message.unwrap();
        assert!(message.contains("Secret Service is unavailable"));
        assert!(!message.contains(CREDENTIAL_HEALTH_PROBE_VALUE));
    }
}
