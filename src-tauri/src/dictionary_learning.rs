const MAX_AUTO_LEARN_WORD_CHARS: usize = 60;

pub fn infer_auto_learn_candidates(
    dictated_text: &str,
    corrected_text: &str,
    existing_words: &[String],
) -> Vec<String> {
    let dictated_tokens = word_tokens(dictated_text);
    let corrected_tokens = word_tokens(corrected_text);

    if dictated_tokens.len() != corrected_tokens.len() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for (index, (dictated, corrected)) in dictated_tokens
        .iter()
        .zip(corrected_tokens.iter())
        .enumerate()
    {
        if dictated == corrected {
            continue;
        }

        let Some(candidate) = sanitize_custom_word_candidate(corrected) else {
            continue;
        };

        if is_sentence_case_cleanup(index, dictated, &candidate)
            || is_existing_word(&candidate, existing_words)
            || is_existing_word(&candidate, &candidates)
        {
            continue;
        }

        if is_likely_custom_dictionary_term(&candidate) {
            candidates.push(candidate);
        }
    }

    candidates
}

pub fn merge_auto_learn_candidates(
    existing_words: &[String],
    candidates: &[String],
) -> Vec<String> {
    let mut merged = existing_words.to_vec();

    for candidate in candidates {
        let Some(candidate) = sanitize_custom_word_candidate(candidate) else {
            continue;
        };

        if !is_existing_word(&candidate, &merged) {
            merged.push(candidate);
        }
    }

    merged
}

fn word_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        if is_token_char(ch) {
            current.push(ch);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn sanitize_custom_word_candidate(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .trim()
        .trim_matches(|ch: char| !is_token_char(ch))
        .chars()
        .filter(|ch| !matches!(ch, '<' | '>' | '"' | '\'' | '&'))
        .collect();
    let cleaned = cleaned.trim().to_string();

    if cleaned.is_empty()
        || cleaned.chars().any(char::is_whitespace)
        || cleaned.chars().count() > MAX_AUTO_LEARN_WORD_CHARS
        || !cleaned.chars().any(char::is_alphabetic)
    {
        return None;
    }

    Some(cleaned)
}

fn is_token_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.' | '#' | '+')
}

fn is_existing_word(candidate: &str, existing_words: &[String]) -> bool {
    existing_words
        .iter()
        .any(|word| word.eq_ignore_ascii_case(candidate))
}

fn is_sentence_case_cleanup(index: usize, dictated: &str, corrected: &str) -> bool {
    if index != 0 || !dictated.eq_ignore_ascii_case(corrected) {
        return false;
    }

    let mut chars = corrected.chars().filter(|ch| ch.is_alphabetic());
    let Some(first) = chars.next() else {
        return false;
    };

    first.is_uppercase() && chars.all(|ch| ch.is_lowercase())
}

fn is_likely_custom_dictionary_term(candidate: &str) -> bool {
    let has_digit = candidate.chars().any(|ch| ch.is_ascii_digit());
    let has_uppercase = candidate.chars().any(char::is_uppercase);
    let has_lowercase = candidate.chars().any(char::is_lowercase);
    let has_term_punctuation = candidate
        .chars()
        .any(|ch| matches!(ch, '-' | '_' | '.' | '#' | '+'));
    let alphabetic_count = candidate.chars().filter(|ch| ch.is_alphabetic()).count();

    has_term_punctuation
        || (has_digit && alphabetic_count > 0)
        || (has_uppercase && has_lowercase)
        || (has_uppercase && alphabetic_count >= 2)
}

#[cfg(test)]
mod tests {
    use super::{infer_auto_learn_candidates, merge_auto_learn_candidates};

    #[test]
    fn suggests_corrected_uncommon_proper_noun() {
        let candidates = infer_auto_learn_candidates(
            "meet with robin tomorrow",
            "meet with Robyn tomorrow",
            &[],
        );

        assert_eq!(candidates, vec!["Robyn"]);
    }

    #[test]
    fn ignores_sentence_capitalization_cleanup() {
        let candidates = infer_auto_learn_candidates("hello world", "Hello world", &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn merge_appends_new_candidates_without_case_duplicates() {
        let existing = vec!["Robyn".to_string()];
        let candidates = vec!["robyn".to_string(), "OAuth".to_string()];

        let merged = merge_auto_learn_candidates(&existing, &candidates);

        assert_eq!(merged, vec!["Robyn", "OAuth"]);
    }
}
