use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct PrivateSessionState {
    enabled: AtomicBool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct PrivateSessionStatus {
    pub enabled: bool,
}

impl PrivateSessionState {
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) -> PrivateSessionStatus {
        self.enabled.store(enabled, Ordering::Relaxed);
        PrivateSessionStatus { enabled }
    }

    pub fn status(&self) -> PrivateSessionStatus {
        PrivateSessionStatus {
            enabled: self.is_enabled(),
        }
    }
}

pub fn is_enabled(app: &AppHandle) -> bool {
    app.try_state::<PrivateSessionState>()
        .is_some_and(|state| state.is_enabled())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_session_defaults_off_and_updates_in_memory() {
        let state = PrivateSessionState::default();

        assert!(!state.is_enabled());
        assert_eq!(
            state.set_enabled(true),
            PrivateSessionStatus { enabled: true }
        );
        assert!(state.is_enabled());
        assert_eq!(
            state.set_enabled(false),
            PrivateSessionStatus { enabled: false }
        );
        assert!(!state.is_enabled());
    }
}
