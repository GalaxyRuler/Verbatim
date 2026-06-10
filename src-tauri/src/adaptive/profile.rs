use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub enum RewriteMode {
    Disabled,
    DeterministicOnly,
    LlmOptional,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct CleanupPolicy {
    pub remove_fillers: bool,
    pub remove_false_starts: bool,
    pub normalize_punctuation: bool,
    pub preserve_code_terms: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RewritePolicy {
    pub mode: RewriteMode,
    pub system_instruction: String,
    pub user_instruction: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct ValidationPolicy {
    pub preserve_raw_language: bool,
    pub forbid_unrequested_translation: bool,
    pub preserve_numbers: bool,
    pub preserve_urls: bool,
    pub preserve_identifiers: bool,
    pub max_expansion_ratio: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct RoutingPolicy {
    pub app_hints: Vec<String>,
    pub language_hints: Vec<String>,
    pub target_hints: Vec<String>,
    pub priority: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, PartialEq, Eq)]
pub struct AdaptiveProfile {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub cleanup: CleanupPolicy,
    pub rewrite: RewritePolicy,
    pub validation: ValidationPolicy,
    pub routing: RoutingPolicy,
}

fn base_validation() -> ValidationPolicy {
    ValidationPolicy {
        preserve_raw_language: true,
        forbid_unrequested_translation: true,
        preserve_numbers: true,
        preserve_urls: true,
        preserve_identifiers: true,
        max_expansion_ratio: 3,
    }
}

fn clean_policy() -> CleanupPolicy {
    CleanupPolicy {
        remove_fillers: true,
        remove_false_starts: true,
        normalize_punctuation: true,
        preserve_code_terms: false,
    }
}

pub fn default_profiles() -> Vec<AdaptiveProfile> {
    vec![
        AdaptiveProfile {
            id: "raw".to_string(),
            name: "Raw".to_string(),
            enabled: true,
            cleanup: CleanupPolicy {
                remove_fillers: false,
                remove_false_starts: false,
                normalize_punctuation: false,
                preserve_code_terms: true,
            },
            rewrite: RewritePolicy {
                mode: RewriteMode::Disabled,
                system_instruction: String::new(),
                user_instruction: String::new(),
            },
            validation: base_validation(),
            routing: RoutingPolicy {
                app_hints: vec![],
                language_hints: vec![],
                target_hints: vec!["raw".to_string()],
                priority: 255,
            },
        },
        AdaptiveProfile {
            id: "default_clean".to_string(),
            name: "Default Clean".to_string(),
            enabled: true,
            cleanup: clean_policy(),
            rewrite: RewritePolicy {
                mode: RewriteMode::DeterministicOnly,
                system_instruction: String::new(),
                user_instruction: String::new(),
            },
            validation: base_validation(),
            routing: RoutingPolicy {
                app_hints: vec![],
                language_hints: vec![],
                target_hints: vec!["unknown".to_string()],
                priority: 10,
            },
        },
        AdaptiveProfile {
            id: "mixed_multilingual".to_string(),
            name: "Mixed Multilingual".to_string(),
            enabled: true,
            cleanup: clean_policy(),
            rewrite: RewritePolicy {
                mode: RewriteMode::LlmOptional,
                system_instruction:
                    "Preserve intentional language mixing. Do not translate unless explicitly requested."
                        .to_string(),
                user_instruction:
                    "Clean the transcript while preserving mixed-language intent.".to_string(),
            },
            validation: base_validation(),
            routing: RoutingPolicy {
                app_hints: vec![],
                language_hints: vec!["mixed".to_string()],
                target_hints: vec![],
                priority: 60,
            },
        },
        AdaptiveProfile {
            id: "technical".to_string(),
            name: "Technical / Code".to_string(),
            enabled: true,
            cleanup: CleanupPolicy {
                preserve_code_terms: true,
                ..clean_policy()
            },
            rewrite: RewritePolicy {
                mode: RewriteMode::LlmOptional,
                system_instruction:
                    "Preserve code, identifiers, commands, paths, URLs, and exact technical terms."
                        .to_string(),
                user_instruction: "Clean wording without rewriting technical tokens.".to_string(),
            },
            validation: base_validation(),
            routing: RoutingPolicy {
                app_hints: vec!["code".to_string(), "terminal".to_string(), "vscode".to_string()],
                language_hints: vec!["technical".to_string()],
                target_hints: vec!["technical".to_string()],
                priority: 90,
            },
        },
        AdaptiveProfile {
            id: "email".to_string(),
            name: "Email".to_string(),
            enabled: true,
            cleanup: clean_policy(),
            rewrite: RewritePolicy {
                mode: RewriteMode::LlmOptional,
                system_instruction:
                    "Write clear professional email prose. Do not add greetings or signoffs unless present in the transcript."
                        .to_string(),
                user_instruction: "Turn the transcript into an email-ready paragraph.".to_string(),
            },
            validation: base_validation(),
            routing: RoutingPolicy {
                app_hints: vec!["outlook".to_string(), "mail".to_string()],
                language_hints: vec![],
                target_hints: vec!["email".to_string()],
                priority: 80,
            },
        },
        AdaptiveProfile {
            id: "notes_markdown".to_string(),
            name: "Notes / Markdown".to_string(),
            enabled: true,
            cleanup: clean_policy(),
            rewrite: RewritePolicy {
                mode: RewriteMode::LlmOptional,
                system_instruction:
                    "Write concise notes. Preserve Markdown structure when implied by the transcript."
                        .to_string(),
                user_instruction: "Clean the transcript as note text.".to_string(),
            },
            validation: base_validation(),
            routing: RoutingPolicy {
                app_hints: vec!["obsidian".to_string(), "notepad".to_string()],
                language_hints: vec![],
                target_hints: vec!["notes".to_string()],
                priority: 70,
            },
        },
        AdaptiveProfile {
            id: "translation".to_string(),
            name: "Translation".to_string(),
            enabled: true,
            cleanup: clean_policy(),
            rewrite: RewritePolicy {
                mode: RewriteMode::LlmOptional,
                system_instruction:
                    "Translate only when the selected profile or trigger explicitly requests translation."
                        .to_string(),
                user_instruction:
                    "Translate the transcript according to the active profile direction.".to_string(),
            },
            validation: ValidationPolicy {
                preserve_raw_language: false,
                forbid_unrequested_translation: false,
                ..base_validation()
            },
            routing: RoutingPolicy {
                app_hints: vec![],
                language_hints: vec!["translation_requested".to_string()],
                target_hints: vec![],
                priority: 50,
            },
        },
    ]
}

pub fn find_profile_or_default<'a>(
    profiles: &'a [AdaptiveProfile],
    id: &str,
) -> &'a AdaptiveProfile {
    profiles
        .iter()
        .find(|profile| profile.id == id && profile.enabled)
        .or_else(|| {
            profiles
                .iter()
                .find(|profile| profile.id == "default_clean")
        })
        .expect("default profiles must include default_clean")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_profiles_include_required_contracts() {
        let profiles = default_profiles();
        let ids: Vec<&str> = profiles.iter().map(|profile| profile.id.as_str()).collect();

        assert!(ids.contains(&"raw"));
        assert!(ids.contains(&"default_clean"));
        assert!(ids.contains(&"mixed_multilingual"));
        assert!(ids.contains(&"technical"));
        assert!(ids.contains(&"email"));
        assert!(ids.contains(&"notes_markdown"));
        assert!(ids.contains(&"translation"));
    }

    #[test]
    fn raw_profile_forbids_rewrite_and_cleanup() {
        let raw = default_profiles()
            .into_iter()
            .find(|profile| profile.id == "raw")
            .expect("raw profile exists");

        assert_eq!(raw.rewrite.mode, RewriteMode::Disabled);
        assert!(!raw.cleanup.remove_fillers);
        assert!(!raw.cleanup.normalize_punctuation);
        assert!(raw.validation.preserve_raw_language);
    }

    #[test]
    fn profile_lookup_falls_back_to_default_clean() {
        let profiles = default_profiles();
        let profile = find_profile_or_default(&profiles, "missing-profile");
        assert_eq!(profile.id, "default_clean");
    }
}
