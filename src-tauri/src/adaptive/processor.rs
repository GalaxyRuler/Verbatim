use crate::adaptive::language::analyze_language;
use crate::adaptive::profile::{AdaptiveProfile, RewriteMode};
use crate::adaptive::types::LanguageClass;

pub fn deterministic_process(raw: &str, profile: &AdaptiveProfile) -> String {
    if profile.rewrite.mode == RewriteMode::Disabled {
        return raw.to_string();
    }

    let mut tokens = Vec::new();
    for token in raw.split_whitespace() {
        if profile.cleanup.remove_fillers && is_simple_filler(token) {
            continue;
        }
        tokens.push(token);
    }

    let mut output = tokens.join(" ");
    if profile.cleanup.normalize_punctuation {
        output = normalize_spacing(&output);
    }
    output
}

pub fn validate_output(raw: &str, output: &str, profile: &AdaptiveProfile) -> Result<(), String> {
    if output.trim().is_empty() && !raw.trim().is_empty() {
        return Err("processed output is empty".to_string());
    }

    if profile.validation.max_expansion_ratio > 0 {
        let raw_len = raw.chars().count().max(1);
        let output_len = output.chars().count();
        if output_len > raw_len * profile.validation.max_expansion_ratio as usize {
            return Err("processed output expanded too much".to_string());
        }
    }

    if profile.validation.forbid_unrequested_translation {
        let raw_language = analyze_language(raw, &[]);
        let output_language = analyze_language(output, &[]);
        if raw_language.class == LanguageClass::Mixed
            && output_language.class != LanguageClass::Mixed
            && output_language.class != LanguageClass::TechnicalMixed
        {
            return Err("mixed-language transcript was collapsed into one language".to_string());
        }
        if raw_language.class == LanguageClass::MostlyArabic
            && output_language.class == LanguageClass::MostlyLatin
        {
            return Err("Arabic transcript appears translated without request".to_string());
        }
    }

    if profile.validation.preserve_urls {
        for token in raw.split_whitespace().filter(|token| token.contains("://")) {
            if !output.contains(token) {
                return Err(format!("URL was not preserved: {}", token));
            }
        }
    }

    if profile.validation.preserve_identifiers {
        for token in raw
            .split_whitespace()
            .filter(|token| token.contains('_') || token.contains("::"))
        {
            if !output.contains(token) {
                return Err(format!("identifier was not preserved: {}", token));
            }
        }
    }

    Ok(())
}

pub fn build_profile_prompt(raw: &str, profile: &AdaptiveProfile) -> Option<String> {
    if profile.rewrite.mode != RewriteMode::LlmOptional {
        return None;
    }

    Some(format!(
        "{}\n\n{}\n\nTranscript:\n{}",
        profile.rewrite.system_instruction, profile.rewrite.user_instruction, raw
    ))
}

fn is_simple_filler(token: &str) -> bool {
    let cleaned = token
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_lowercase();
    matches!(cleaned.as_str(), "um" | "uh" | "erm" | "ah")
}

fn normalize_spacing(input: &str) -> String {
    input
        .replace(" ,", ",")
        .replace(" .", ".")
        .replace("  ", " ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::profile::default_profiles;

    fn profile(id: &str) -> AdaptiveProfile {
        default_profiles()
            .into_iter()
            .find(|profile| profile.id == id)
            .unwrap()
    }

    #[test]
    fn raw_profile_returns_input_unchanged() {
        let result = deterministic_process("um hello hello", &profile("raw"));
        assert_eq!(result, "um hello hello");
    }

    #[test]
    fn clean_profile_removes_simple_english_fillers() {
        let result = deterministic_process("um hello, uh we should go", &profile("default_clean"));
        assert_eq!(result, "hello, we should go");
    }

    #[test]
    fn technical_profile_preserves_identifiers() {
        let result = deterministic_process(
            "uh run cargo_test in src-tauri/src/actions.rs",
            &profile("technical"),
        );
        assert!(result.contains("cargo_test"));
        assert!(result.contains("src-tauri/src/actions.rs"));
    }

    #[test]
    fn validator_rejects_unrequested_translation_when_language_disappears() {
        let email = profile("email");
        let validation = validate_output("خلينا send it", "Send it tomorrow", &email);
        assert!(validation.is_err());
    }

    #[test]
    fn default_clean_profile_rejects_arabic_to_english_translation() {
        let default_clean = profile("default_clean");
        let validation = validate_output("هذا نص عربي", "This is Arabic text", &default_clean);
        assert!(validation.is_err());
    }
}
