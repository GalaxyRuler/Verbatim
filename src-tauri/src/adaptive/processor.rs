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
    if profile.id == "email" {
        output = format_email_text(&output);
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
        validate_unrequested_translation(raw, output)?;
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

pub fn validate_unrequested_translation(raw: &str, output: &str) -> Result<(), String> {
    let raw_language = analyze_language(raw, &[]);
    let output_language = analyze_language(output, &[]);

    if raw_language.class == LanguageClass::Mixed
        && output_language.class != LanguageClass::Mixed
        && output_language.class != LanguageClass::TechnicalMixed
    {
        return Err("mixed-language transcript was collapsed into one language".to_string());
    }

    match (&raw_language.class, &output_language.class) {
        (LanguageClass::MostlyArabic, LanguageClass::MostlyLatin) => {
            Err("Arabic transcript appears translated without request".to_string())
        }
        (LanguageClass::MostlyLatin, LanguageClass::MostlyArabic) => {
            Err("Latin transcript appears translated without request".to_string())
        }
        _ => Ok(()),
    }
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

fn format_email_text(input: &str) -> String {
    let trimmed = input.trim();
    let (greeting, body_with_closing) = split_email_greeting(trimmed);
    let (body, closing) = split_email_closing(body_with_closing.trim());

    if greeting.is_none() && closing.is_none() {
        return trimmed.to_string();
    }

    let mut sections = Vec::new();
    if let Some(greeting) = greeting {
        sections.push(greeting.trim().to_string());
    }
    if !body.trim().is_empty() {
        sections.push(body.trim().to_string());
    }
    if let Some(closing) = closing {
        sections.push(format_email_closing(closing.trim()));
    }

    sections.join("\n\n")
}

fn split_email_greeting(input: &str) -> (Option<&str>, &str) {
    let lower = input.to_lowercase();
    let has_email_greeting = ["dear ", "hi ", "hello "]
        .iter()
        .any(|prefix| lower.starts_with(prefix));

    if !has_email_greeting {
        return (None, input);
    }

    if let Some(comma_index) = input.find(',') {
        if comma_index <= 80 {
            let split_index = comma_index + 1;
            return (Some(&input[..split_index]), &input[split_index..]);
        }
    }

    (None, input)
}

fn split_email_closing(input: &str) -> (&str, Option<&str>) {
    let lower = input.to_lowercase();
    let mut best_match: Option<(usize, usize)> = None;

    for marker in [
        "best regards,",
        "kind regards,",
        "regards,",
        "sincerely,",
        "thank you,",
        "thanks,",
    ] {
        if let Some(index) = lower.rfind(marker) {
            let end_index = index + marker.len();
            let should_replace = best_match.is_none_or(|(current_index, current_end)| {
                end_index > current_end || (end_index == current_end && index < current_index)
            });
            if should_replace {
                best_match = Some((index, end_index));
            }
        }
    }

    if let Some((index, _)) = best_match {
        return (&input[..index], Some(&input[index..]));
    }

    (input, None)
}

fn format_email_closing(input: &str) -> String {
    if let Some(comma_index) = input.find(',') {
        let split_index = comma_index + 1;
        let signoff = input[..split_index].trim();
        let signature = input[split_index..].trim();
        if !signature.is_empty() {
            return format!("{}\n{}", signoff, signature);
        }
    }

    input.to_string()
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
    fn email_profile_formats_dictated_greeting_body_and_closing() {
        let result = deterministic_process(
            "Dear James, I have received your Excel file. Sincerely, Abdullah Al-Khalid.",
            &profile("email"),
        );

        assert_eq!(
            result,
            "Dear James,\n\nI have received your Excel file.\n\nSincerely,\nAbdullah Al-Khalid."
        );
    }

    #[test]
    fn email_profile_preserves_multi_word_closing_marker() {
        let result = deterministic_process(
            "Hello Dana, The report is attached. Best regards, Abdullah.",
            &profile("email"),
        );

        assert_eq!(
            result,
            "Hello Dana,\n\nThe report is attached.\n\nBest regards,\nAbdullah."
        );
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

    #[test]
    fn default_clean_profile_rejects_english_to_arabic_translation() {
        let default_clean = profile("default_clean");
        let validation = validate_output(
            "This is a clear English sentence",
            "هذه جملة عربية",
            &default_clean,
        );
        assert!(validation.is_err());
    }

    #[test]
    fn smart_formatting_none_preserves_raw_transcript() {
        let result = crate::adaptive::smart_formatting::format_transcript(
            "Send it Monday no, I mean Tuesday.",
            crate::settings::FormattingLevel::None,
        );
        assert_eq!(result, "Send it Monday no, I mean Tuesday.");
    }

    #[test]
    fn smart_formatting_light_applies_no_i_mean_backtrack() {
        let result = crate::adaptive::smart_formatting::format_transcript(
            "The deadline is Monday no, I mean Tuesday.",
            crate::settings::FormattingLevel::Light,
        );
        assert_eq!(result, "The deadline is Tuesday.");
    }

    #[test]
    fn smart_formatting_light_applies_scratch_that_backtrack() {
        let result = crate::adaptive::smart_formatting::format_transcript(
            "Please send the first draft scratch that send the final version.",
            crate::settings::FormattingLevel::Light,
        );
        assert_eq!(result, "send the final version.");
    }

    #[test]
    fn smart_formatting_light_applies_actually_backtrack() {
        let result = crate::adaptive::smart_formatting::format_transcript(
            "The meeting is Monday actually Tuesday.",
            crate::settings::FormattingLevel::Light,
        );
        assert_eq!(result, "The meeting is Tuesday.");
    }

    #[test]
    fn smart_formatting_light_preserves_ordinary_actually_usage() {
        let result = crate::adaptive::smart_formatting::format_transcript(
            "This is actually important.",
            crate::settings::FormattingLevel::Light,
        );
        assert_eq!(result, "This is actually important.");
    }

    #[test]
    fn smart_formatting_light_applies_replace_command() {
        let result = crate::adaptive::smart_formatting::format_transcript(
            "The contact is Abdullah Al-Khulayb replace Al-Khulayb with Al Kulaib.",
            crate::settings::FormattingLevel::Light,
        );
        assert_eq!(result, "The contact is Abdullah Al Kulaib.");
    }

    #[test]
    fn smart_formatting_rejects_unrequested_translation() {
        let result = crate::adaptive::smart_formatting::validate_formatted_output(
            "This is English.",
            "هذا نص عربي.",
        );
        assert!(result.is_err());
    }
}
