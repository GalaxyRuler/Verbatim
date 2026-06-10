use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, Default, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CorrectionMemory {
    pub entries: Vec<CorrectionPreference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CorrectionPreference {
    pub context_key: String,
    pub profile_id: String,
    pub weight: u8,
}

impl CorrectionMemory {
    pub fn preferred_profile(&self, context_key: &str) -> Option<&str> {
        self.entries
            .iter()
            .filter(|entry| entry.context_key == context_key)
            .max_by_key(|entry| entry.weight)
            .map(|entry| entry.profile_id.as_str())
    }

    pub fn reset(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correction_memory_prefers_highest_weight_same_context() {
        let memory = CorrectionMemory {
            entries: vec![
                CorrectionPreference {
                    context_key: "outlook|edit".to_string(),
                    profile_id: "email".to_string(),
                    weight: 1,
                },
                CorrectionPreference {
                    context_key: "outlook|edit".to_string(),
                    profile_id: "mixed_multilingual".to_string(),
                    weight: 3,
                },
            ],
        };

        assert_eq!(
            memory.preferred_profile("outlook|edit"),
            Some("mixed_multilingual")
        );
    }

    #[test]
    fn correction_memory_ignores_other_contexts() {
        let memory = CorrectionMemory {
            entries: vec![CorrectionPreference {
                context_key: "code|edit".to_string(),
                profile_id: "technical".to_string(),
                weight: 9,
            }],
        };

        assert_eq!(memory.preferred_profile("outlook|edit"), None);
    }
}
