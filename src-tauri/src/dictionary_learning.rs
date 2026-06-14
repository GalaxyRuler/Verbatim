const MAX_AUTO_LEARN_WORD_CHARS: usize = 60;
const MAX_AUTO_LEARN_PHRASE_CHARS: usize = 120;
const MAX_AUTO_LEARN_PHRASE_TOKENS: usize = 5;
const MAX_HYPHEN_PART_DISTANCE: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoLearnCandidate {
    pub phrase: String,
    pub replacement_of: Option<String>,
}

pub fn infer_auto_learn_candidates(
    dictated_text: &str,
    corrected_text: &str,
    existing_words: &[String],
) -> Vec<AutoLearnCandidate> {
    let dictated_tokens = word_tokens(dictated_text);
    let corrected_tokens = word_tokens(corrected_text);

    if dictated_tokens.len() != corrected_tokens.len() {
        return infer_hyphen_split_auto_learn_candidates(
            &dictated_tokens,
            &corrected_tokens,
            existing_words,
        );
    }

    let mut candidates = Vec::new();
    let phrase_candidates = infer_auto_learn_phrase_candidates(&dictated_tokens, &corrected_tokens);
    let mut phrase_covered_indices = vec![false; corrected_tokens.len()];

    for (start, end, phrase) in phrase_candidates {
        for covered in phrase_covered_indices.iter_mut().take(end + 1).skip(start) {
            *covered = true;
        }

        if is_existing_word(&phrase, existing_words) || is_existing_candidate(&phrase, &candidates)
        {
            continue;
        }

        candidates.push(AutoLearnCandidate {
            phrase,
            replacement_of: Some(dictated_tokens[start..=end].join(" ")),
        });
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
            || is_existing_candidate(&candidate, &candidates)
        {
            continue;
        }

        if is_likely_custom_dictionary_term(&candidate) {
            candidates.push(AutoLearnCandidate {
                phrase: candidate,
                replacement_of: Some(dictated.clone()),
            });
        }
    }

    candidates
}

fn infer_hyphen_split_auto_learn_candidates(
    dictated_tokens: &[String],
    corrected_tokens: &[String],
    existing_words: &[String],
) -> Vec<AutoLearnCandidate> {
    let mut candidates = Vec::new();
    let mut dictated_index = 0;
    let mut corrected_index = 0;

    while dictated_index < dictated_tokens.len() && corrected_index < corrected_tokens.len() {
        let dictated = &dictated_tokens[dictated_index];
        let corrected = &corrected_tokens[corrected_index];

        if dictated.eq_ignore_ascii_case(corrected) {
            dictated_index += 1;
            corrected_index += 1;
            continue;
        }

        let dictated_parts = split_hyphenated_token(dictated);
        if dictated_parts.len() < 2 {
            return Vec::new();
        }

        let corrected_end = corrected_index + dictated_parts.len();
        if corrected_end > corrected_tokens.len() {
            return Vec::new();
        }

        let corrected_parts = &corrected_tokens[corrected_index..corrected_end];
        if !hyphen_parts_match_correction(&dictated_parts, corrected_parts) {
            return Vec::new();
        }

        let raw_phrase = corrected_parts.join(" ");
        let Some(phrase) = sanitize_custom_phrase_candidate(&raw_phrase) else {
            return Vec::new();
        };

        if is_likely_custom_dictionary_phrase(&phrase)
            && !is_existing_word(&phrase, existing_words)
            && !is_existing_candidate(&phrase, &candidates)
        {
            candidates.push(AutoLearnCandidate {
                phrase,
                replacement_of: Some(dictated.clone()),
            });
        }

        dictated_index += 1;
        corrected_index = corrected_end;
    }

    if dictated_index == dictated_tokens.len() && corrected_index == corrected_tokens.len() {
        candidates
    } else {
        Vec::new()
    }
}

fn split_hyphenated_token(token: &str) -> Vec<String> {
    token
        .split('-')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn hyphen_parts_match_correction(dictated_parts: &[String], corrected_parts: &[String]) -> bool {
    dictated_parts.len() == corrected_parts.len()
        && dictated_parts
            .iter()
            .zip(corrected_parts.iter())
            .all(|(dictated, corrected)| tokens_are_close(dictated, corrected))
}

fn tokens_are_close(left: &str, right: &str) -> bool {
    let left = left.to_lowercase();
    let right = right.to_lowercase();

    if left == right {
        return true;
    }

    let max_chars = left.chars().count().max(right.chars().count());
    if max_chars < 4 {
        return false;
    }

    edit_distance(&left, &right) <= MAX_HYPHEN_PART_DISTANCE.min(max_chars / 3).max(1)
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right_chars = right.chars().collect::<Vec<_>>();
    let mut costs = (0..=right_chars.len()).collect::<Vec<_>>();

    for (left_index, left_char) in left.chars().enumerate() {
        let mut previous = costs[0];
        costs[0] = left_index + 1;

        for (right_index, right_char) in right_chars.iter().enumerate() {
            let insertion = costs[right_index + 1] + 1;
            let deletion = costs[right_index] + 1;
            let substitution = previous + usize::from(left_char != *right_char);
            previous = costs[right_index + 1];
            costs[right_index + 1] = insertion.min(deletion).min(substitution);
        }
    }

    costs[right_chars.len()]
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

#[cfg(test)]
pub fn merge_auto_learn_candidates(
    existing_words: &[String],
    candidates: &[AutoLearnCandidate],
) -> Vec<String> {
    let mut merged = existing_words.to_vec();

    for candidate in candidates {
        let Some(candidate) = sanitize_custom_word_candidate(&candidate.phrase) else {
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

    if cleaned.starts_with('-') || cleaned.ends_with('-') {
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

fn is_existing_candidate(candidate: &str, existing_candidates: &[AutoLearnCandidate]) -> bool {
    existing_candidates
        .iter()
        .any(|entry| entry.phrase.eq_ignore_ascii_case(candidate))
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
    let tokens = candidate.split_whitespace().collect::<Vec<_>>();
    let notable_terms = candidate
        .split_whitespace()
        .filter(|token| !is_phrase_connector(token) && is_likely_custom_dictionary_term(token))
        .count();

    notable_terms >= 2
        || matches!(tokens.as_slice(), [connector, term] if is_phrase_connector(connector) && is_likely_custom_dictionary_term(term))
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
    use super::{infer_auto_learn_candidates, merge_auto_learn_candidates, AutoLearnCandidate};

    #[test]
    fn suggests_corrected_uncommon_proper_noun() {
        let candidates = infer_auto_learn_candidates(
            "meet with robin tomorrow",
            "meet with Robyn tomorrow",
            &[],
        );

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "Robyn".to_string(),
                replacement_of: Some("robin".to_string()),
            }]
        );
    }

    #[test]
    fn suggests_corrected_multi_word_name_phrase() {
        let candidates = infer_auto_learn_candidates(
            "my name is abdullah al kulaib",
            "my name is Abdullah al Kulaib",
            &[],
        );

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "Abdullah al Kulaib".to_string(),
                replacement_of: Some("abdullah al kulaib".to_string()),
            }]
        );
    }

    #[test]
    fn suggests_hyphenated_name_correction_as_phrase_mapping() {
        let candidates = infer_auto_learn_candidates(
            "my name is Abdullah Al-Khulayb",
            "my name is Abdullah Al Kulaib",
            &[],
        );

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "Al Kulaib".to_string(),
                replacement_of: Some("Al-Khulayb".to_string()),
            }]
        );
    }

    #[test]
    fn ignores_dangling_hyphen_partial_corrections() {
        let candidates = infer_auto_learn_candidates("Abdullah Al-Kulayb", "Abdullah Al-", &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn ignores_dangling_hyphen_word_corrections() {
        let candidates = infer_auto_learn_candidates("wow", "Vow-", &[]);

        assert!(candidates.is_empty());
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
        let candidates = vec![
            AutoLearnCandidate {
                phrase: "robyn".to_string(),
                replacement_of: Some("robin".to_string()),
            },
            AutoLearnCandidate {
                phrase: "OAuth".to_string(),
                replacement_of: Some("o auth".to_string()),
            },
        ];

        let merged = merge_auto_learn_candidates(&existing, &candidates);

        assert_eq!(merged, vec!["Robyn", "OAuth"]);
    }
}
