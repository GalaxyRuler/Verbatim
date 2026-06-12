const MAX_AUTO_LEARN_WORD_CHARS: usize = 60;
const MAX_AUTO_LEARN_PHRASE_CHARS: usize = 120;
const MAX_AUTO_LEARN_PHRASE_TOKENS: usize = 5;

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
    let phrase_candidates = infer_auto_learn_phrase_candidates(&dictated_tokens, &corrected_tokens);
    let mut phrase_covered_indices = vec![false; corrected_tokens.len()];

    for (start, end, phrase) in phrase_candidates {
        for covered in phrase_covered_indices.iter_mut().take(end + 1).skip(start) {
            *covered = true;
        }

        if is_existing_word(&phrase, existing_words) || is_existing_word(&phrase, &candidates) {
            continue;
        }

        candidates.push(phrase);
    }

    for (index, (dictated, corrected)) in dictated_tokens
        .iter()
        .zip(corrected_tokens.iter())
        .enumerate()
    {
        if dictated == corrected || phrase_covered_indices[index] {
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

fn infer_auto_learn_phrase_candidates(
    dictated_tokens: &[String],
    corrected_tokens: &[String],
) -> Vec<(usize, usize, String)> {
    let changed_indices = dictated_tokens
        .iter()
        .zip(corrected_tokens.iter())
        .enumerate()
        .filter_map(|(index, (dictated, corrected))| (dictated != corrected).then_some(index))
        .collect::<Vec<_>>();

    if changed_indices.len() < 2 {
        return Vec::new();
    }

    let mut phrase_candidates = Vec::new();
    let mut start = changed_indices[0];
    let mut end = changed_indices[0];
    let mut changed_count = 1;

    for &index in changed_indices.iter().skip(1) {
        let gap_is_connective = corrected_tokens[end + 1..index]
            .iter()
            .all(|token| is_phrase_connector(token));

        if gap_is_connective && index - start < MAX_AUTO_LEARN_PHRASE_TOKENS {
            end = index;
            changed_count += 1;
            continue;
        }

        push_phrase_candidate(
            &mut phrase_candidates,
            corrected_tokens,
            start,
            end,
            changed_count,
        );
        start = index;
        end = index;
        changed_count = 1;
    }

    push_phrase_candidate(
        &mut phrase_candidates,
        corrected_tokens,
        start,
        end,
        changed_count,
    );

    phrase_candidates
}

fn push_phrase_candidate(
    phrase_candidates: &mut Vec<(usize, usize, String)>,
    corrected_tokens: &[String],
    start: usize,
    end: usize,
    changed_count: usize,
) {
    if changed_count < 2 {
        return;
    }

    let raw_phrase = corrected_tokens[start..=end].join(" ");
    let Some(phrase) = sanitize_custom_phrase_candidate(&raw_phrase) else {
        return;
    };

    if !is_likely_custom_dictionary_phrase(&phrase) {
        return;
    }

    phrase_candidates.push((start, end, phrase));
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

fn sanitize_custom_phrase_candidate(raw: &str) -> Option<String> {
    let tokens = raw
        .split_whitespace()
        .map(sanitize_custom_word_candidate)
        .collect::<Option<Vec<_>>>()?;

    if tokens.len() < 2 || tokens.len() > MAX_AUTO_LEARN_PHRASE_TOKENS {
        return None;
    }

    let cleaned = tokens.join(" ");
    if cleaned.chars().count() > MAX_AUTO_LEARN_PHRASE_CHARS {
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

fn is_likely_custom_dictionary_phrase(candidate: &str) -> bool {
    let notable_terms = candidate
        .split_whitespace()
        .filter(|token| !is_phrase_connector(token) && is_likely_custom_dictionary_term(token))
        .count();

    notable_terms >= 2
}

fn is_phrase_connector(token: &str) -> bool {
    matches!(
        token.to_lowercase().as_str(),
        "al" | "el"
            | "bin"
            | "ibn"
            | "bint"
            | "abu"
            | "van"
            | "von"
            | "de"
            | "da"
            | "del"
            | "di"
            | "du"
            | "la"
            | "le"
    )
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
    fn suggests_corrected_multi_word_name_phrase() {
        let candidates = infer_auto_learn_candidates(
            "my name is abdullah al kulaib",
            "my name is Abdullah al Kulaib",
            &[],
        );

        assert_eq!(candidates, vec!["Abdullah al Kulaib"]);
    }

    #[test]
    fn existing_multi_word_phrase_suppresses_component_words() {
        let existing = vec!["Abdullah al Kulaib".to_string()];
        let candidates = infer_auto_learn_candidates(
            "my name is abdullah al kulaib",
            "my name is Abdullah al Kulaib",
            &existing,
        );

        assert!(candidates.is_empty());
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
