use crate::adaptive::types::CapturedContext;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Default)]
pub struct ActiveDictationContext {
    contexts: Mutex<HashMap<String, CapturedContext>>,
}

impl ActiveDictationContext {
    pub fn insert(&self, binding_id: &str, context: CapturedContext) {
        if let Ok(mut contexts) = self.contexts.lock() {
            contexts.insert(binding_id.to_string(), context);
        }
    }

    pub fn take(&self, binding_id: &str) -> Option<CapturedContext> {
        self.contexts.lock().ok()?.remove(binding_id)
    }

    pub fn clear(&self, binding_id: &str) {
        if let Ok(mut contexts) = self.contexts.lock() {
            contexts.remove(binding_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::types::TargetKind;

    fn context(process: &str) -> CapturedContext {
        CapturedContext {
            captured_at_ms: 1,
            process_name: Some(process.to_string()),
            window_title: None,
            window_title_hash: None,
            window_class: None,
            target_kind: TargetKind::Unknown,
            target_fingerprint: Some(process.to_string()),
            is_sensitive: false,
        }
    }

    #[test]
    fn store_take_removes_context() {
        let store = ActiveDictationContext::default();
        store.insert("transcribe", context("OUTLOOK.EXE"));

        assert_eq!(
            store.take("transcribe").unwrap().process_name.as_deref(),
            Some("OUTLOOK.EXE")
        );
        assert!(store.take("transcribe").is_none());
    }
}
