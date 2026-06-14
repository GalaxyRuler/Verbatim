use crate::settings::FormattingLevel;
use once_cell::sync::Lazy;
use regex::Regex;

static SCRATCH_THAT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\b(?:scratch\s+that|forget\s+that)\b[,\s]*").unwrap());
static NO_I_MEAN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\bno[,\s]+i\s+mean\b[,\s]*").unwrap());
static ACTUALLY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bactually\b[,\s]*").unwrap());
static REPLACE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)\breplace\s+(.+?)\s+with\s+(.+?)([.!?])?\s*$").unwrap());

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
    crate::adaptive::processor::validate_unrequested_translation(raw, output)
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
    let lower_input = input.to_lowercase();
    let lower_target = target.to_lowercase();
    let start = lower_input.rfind(&lower_target)?;
    let end = start + lower_target.len();
    if !input.is_char_boundary(start) || !input.is_char_boundary(end) {
        return None;
    }

    let mut output = String::new();
    output.push_str(&input[..start]);
    output.push_str(replacement);
    output.push_str(&input[end..]);
    Some(output)
}

fn normalize_spacing(input: &str) -> String {
    let mut output = input.split_whitespace().collect::<Vec<_>>().join(" ");
    for punctuation in [",", ".", "?", "!", ":", ";"] {
        output = output.replace(&format!(" {punctuation}"), punctuation);
    }
    output.trim().to_string()
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
    input
        .replace(" period", ".")
        .replace(" comma", ",")
        .replace(" question mark", "?")
        .replace(" exclamation mark", "!")
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
