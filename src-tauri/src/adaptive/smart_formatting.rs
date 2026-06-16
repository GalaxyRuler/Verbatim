use crate::settings::FormattingLevel;
use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};

static SCRATCH_THAT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(?:scratch\s+that|forget\s+that)\b[,\s]*").unwrap());
static NO_I_MEAN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bno[,\s]+i\s+mean\b[,\s]*").unwrap());
static ACTUALLY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bactually\b[,\s]*").unwrap());
static REPLACE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)\breplace\s+(.+?)\s+with\s+(.+?)([.!?])?\s*$").unwrap());
static SPOKEN_PUNCTUATION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(question\s+mark|exclamation\s+mark|comma|period)\b").unwrap());

pub fn format_transcript(raw: &str, level: FormattingLevel) -> String {
    if level == FormattingLevel::None {
        return raw.to_string();
    }

    let mut formatted = apply_backtrack(raw);
    formatted = normalize_spacing(&formatted);

    if matches!(level, FormattingLevel::Medium | FormattingLevel::High) {
        formatted = remove_simple_fillers(&formatted);
        formatted = normalize_spoken_punctuation(&formatted);
        formatted = normalize_spacing(&formatted);
    }

    if level == FormattingLevel::High {
        formatted = collapse_adjacent_duplicate_words(&formatted);
    }

    match validate_formatted_output(raw, &formatted) {
        Ok(()) => formatted,
        Err(_) => raw.to_string(),
    }
}

pub fn validate_formatted_output(raw: &str, output: &str) -> Result<(), String> {
    crate::adaptive::processor::validate_unrequested_translation(raw, output)?;
    validate_non_destructive_formatting(raw, output)
}

fn validate_non_destructive_formatting(raw: &str, output: &str) -> Result<(), String> {
    let raw_tokens = meaningful_token_count(raw);
    if raw_tokens < 6 {
        return Ok(());
    }

    let output_tokens = meaningful_token_count(output);
    if output_tokens * 100 < raw_tokens * 45 {
        return Err(format!(
            "formatted output removed too much text: {output_tokens}/{raw_tokens} tokens"
        ));
    }

    Ok(())
}

fn meaningful_token_count(input: &str) -> usize {
    input
        .split_whitespace()
        .filter(|token| token.chars().any(|ch| ch.is_alphanumeric()))
        .count()
}

fn apply_backtrack(input: &str) -> String {
    if let Some(replaced) = apply_replace_command(input) {
        return replaced;
    }

    if let Some(rewritten) = apply_scratch_that(input) {
        return rewritten;
    }

    if let Some(rewritten) = apply_marker_correction(input, &NO_I_MEAN_RE, false) {
        return rewritten;
    }

    if let Some(rewritten) = apply_marker_correction(input, &ACTUALLY_RE, true) {
        return rewritten;
    }

    input.to_string()
}

fn apply_scratch_that(input: &str) -> Option<String> {
    let marker = SCRATCH_THAT_RE.find_iter(input).last()?;
    let after = input[marker.end()..].trim();
    if after.is_empty() {
        return None;
    }
    Some(after.to_string())
}

fn apply_marker_correction(input: &str, marker_re: &Regex, actually_rules: bool) -> Option<String> {
    let marker = marker_re.find_iter(input).last()?;
    let before = input[..marker.start()].trim_end();
    let after = input[marker.end()..].trim_start();

    if before.is_empty() || after.is_empty() {
        return None;
    }

    if actually_rules && !looks_like_actual_correction(before, after) {
        return None;
    }

    let replace_start = editable_span_start(before)?;
    let mut result = String::new();
    result.push_str(before[..replace_start].trim_end());
    if !result.is_empty() {
        result.push(' ');
    }
    result.push_str(after);
    Some(result)
}

fn looks_like_actual_correction(before: &str, after: &str) -> bool {
    let before_words: Vec<&str> = before.split_whitespace().collect();
    if before_words.len() < 3 || after.split_whitespace().count() > 6 {
        return false;
    }

    let Some(previous_word) = before_words.last() else {
        return false;
    };
    !is_auxiliary_word(previous_word)
}

fn is_auxiliary_word(word: &str) -> bool {
    let cleaned = word
        .trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_ascii_lowercase();
    matches!(
        cleaned.as_str(),
        "am" | "are"
            | "be"
            | "been"
            | "being"
            | "can"
            | "could"
            | "did"
            | "do"
            | "does"
            | "had"
            | "has"
            | "have"
            | "is"
            | "may"
            | "might"
            | "must"
            | "shall"
            | "should"
            | "was"
            | "were"
            | "will"
            | "would"
    )
}

fn editable_span_start(before: &str) -> Option<usize> {
    let trimmed = before.trim_end();
    let last_word_start = trimmed
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .unwrap_or(0);
    Some(last_word_start)
}

fn apply_replace_command(input: &str) -> Option<String> {
    let captures = REPLACE_RE.captures(input)?;
    let command = captures.get(0)?;
    let before = input[..command.start()].trim_end();
    if before.is_empty() {
        return None;
    }

    let target = captures.get(1)?.as_str().trim();
    let replacement = captures.get(2)?.as_str().trim();
    if target.is_empty() || replacement.is_empty() {
        return None;
    }

    let mut replaced = replace_last_case_insensitive(before, target, replacement)?;
    if let Some(punctuation) = captures.get(3).map(|m| m.as_str()) {
        if !replaced.ends_with(['.', '!', '?']) {
            replaced.push_str(punctuation);
        }
    }
    Some(replaced)
}

fn replace_last_case_insensitive(input: &str, target: &str, replacement: &str) -> Option<String> {
    let target_re = RegexBuilder::new(&regex::escape(target))
        .case_insensitive(true)
        .build()
        .ok()?;
    let matched = target_re
        .find_iter(input)
        .filter(|candidate| is_whole_term_match(input, candidate.start(), candidate.end()))
        .last()?;
    let start = matched.start();
    let end = matched.end();

    let mut output = String::new();
    output.push_str(&input[..start]);
    output.push_str(replacement);
    output.push_str(&input[end..]);
    Some(output)
}

fn is_whole_term_match(input: &str, start: usize, end: usize) -> bool {
    let before = input[..start].chars().next_back();
    let after = input[end..].chars().next();
    is_term_boundary(before) && is_term_boundary(after)
}

fn is_term_boundary(ch: Option<char>) -> bool {
    ch.map(|ch| !ch.is_alphanumeric() && ch != '_' && ch != '-')
        .unwrap_or(true)
}

fn normalize_spacing(input: &str) -> String {
    input
        .lines()
        .map(normalize_line_spacing)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn normalize_line_spacing(input: &str) -> String {
    let mut output = input.split_whitespace().collect::<Vec<_>>().join(" ");
    for punctuation in [",", ".", "?", "!", ":", ";"] {
        output = output.replace(&format!(" {punctuation}"), punctuation);
    }
    output
}

fn remove_simple_fillers(input: &str) -> String {
    input
        .split_whitespace()
        .filter(|token| {
            let cleaned = token
                .trim_matches(|ch: char| ch.is_ascii_punctuation())
                .to_ascii_lowercase();
            !matches!(cleaned.as_str(), "um" | "uh" | "erm" | "ah")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_spoken_punctuation(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    for matched in SPOKEN_PUNCTUATION_RE.find_iter(input) {
        let phrase = matched.as_str().to_ascii_lowercase();
        let Some(symbol) = spoken_punctuation_symbol(&phrase) else {
            continue;
        };
        if !should_convert_spoken_punctuation(input, matched.start(), matched.end(), &phrase) {
            continue;
        }

        output.push_str(&input[cursor..matched.start()]);
        while output.chars().next_back().is_some_and(char::is_whitespace) {
            output.pop();
        }
        output.push(symbol);
        cursor = matched.end();
    }

    output.push_str(&input[cursor..]);
    output
}

fn spoken_punctuation_symbol(phrase: &str) -> Option<char> {
    match phrase {
        "period" => Some('.'),
        "comma" => Some(','),
        "question mark" => Some('?'),
        "exclamation mark" => Some('!'),
        _ => None,
    }
}

fn should_convert_spoken_punctuation(input: &str, start: usize, end: usize, phrase: &str) -> bool {
    let before = input[..start].trim_end();
    if before.is_empty() {
        return false;
    }

    let after = input[end..].trim_start();
    let previous = previous_word(before);
    if is_protected_literal_punctuation_phrase(previous.as_deref(), phrase) {
        return false;
    }

    if phrase == "comma" {
        return !after.is_empty();
    }

    after.is_empty() || after.starts_with(['.', ',', '?', '!', ':', ';'])
}

fn previous_word(input: &str) -> Option<String> {
    input
        .split_whitespace()
        .last()
        .map(|word| {
            word.trim_matches(|ch: char| ch.is_ascii_punctuation())
                .to_ascii_lowercase()
        })
        .filter(|word| !word.is_empty())
}

fn is_protected_literal_punctuation_phrase(previous_word: Option<&str>, phrase: &str) -> bool {
    let Some(previous_word) = previous_word else {
        return false;
    };

    match phrase {
        "period" => matches!(
            previous_word,
            "accounting"
                | "billing"
                | "class"
                | "grace"
                | "historical"
                | "notice"
                | "pay"
                | "probationary"
                | "reporting"
                | "trial"
                | "waiting"
        ),
        "comma" => matches!(previous_word, "decimal" | "oxford" | "serial"),
        "question mark" | "exclamation mark" => {
            matches!(previous_word, "a" | "an" | "the" | "literal")
        }
        _ => false,
    }
}

fn collapse_adjacent_duplicate_words(input: &str) -> String {
    let mut output = Vec::new();
    for token in input.split_whitespace() {
        let is_duplicate = output
            .last()
            .map(|last: &&str| {
                last.trim_matches(|ch: char| ch.is_ascii_punctuation())
                    .eq_ignore_ascii_case(token.trim_matches(|ch: char| ch.is_ascii_punctuation()))
            })
            .unwrap_or(false);
        if !is_duplicate {
            output.push(token);
        }
    }
    output.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_formatting_rejects_destructive_scratch_that_output() {
        let raw = "Please send the first draft scratch that send the final version.";

        let result = format_transcript(raw, FormattingLevel::Light);

        assert_eq!(result, raw);
    }

    #[test]
    fn replace_command_requires_whole_term_match() {
        let raw = "The code is catalog replace cat with dog.";

        let result = format_transcript(raw, FormattingLevel::Light);

        assert_eq!(result, raw);
    }

    #[test]
    fn spoken_punctuation_keeps_common_literal_period_phrases() {
        let raw = "Please confirm the grace period";

        let result = format_transcript(raw, FormattingLevel::Medium);

        assert_eq!(result, raw);
    }

    #[test]
    fn spoken_punctuation_converts_standalone_dictation_commands() {
        let raw = "hello comma world period";

        let result = format_transcript(raw, FormattingLevel::Medium);

        assert_eq!(result, "hello, world.");
    }
}
