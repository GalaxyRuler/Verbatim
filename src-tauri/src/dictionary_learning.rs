const MAX_AUTO_LEARN_WORD_CHARS: usize = 60;
const MAX_AUTO_LEARN_PHRASE_CHARS: usize = 120;
const MAX_AUTO_LEARN_PHRASE_TOKENS: usize = 5;
const MAX_HYPHEN_PART_DISTANCE: usize = 2;

/// The single normalizer used for inverse/refinement detection, exact-rule source
/// matching, conflict grouping, occurrence identity, and suppression keys.
/// Folds case, collapses whitespace, and maps smart punctuation to ASCII.
///
/// NOTE: Does not NFC-normalize (the `unicode-normalization` crate is not a
/// dependency of this crate). If it is added later, prefix with
/// `raw.nfc().collect::<String>()` before the fold below.
pub fn canonicalize(raw: &str) -> String {
    let mut out = String::new();
    let mut last_was_space = false;
    for ch in raw.trim().chars() {
        // Fold smart quotes/dashes to ASCII equivalents.
        let ch = match ch {
            '\u{2018}' | '\u{2019}' | '\u{201B}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2013}' | '\u{2014}' | '\u{2212}' => '-',
            other => other,
        };
        if ch.is_whitespace() {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }
        last_was_space = false;
        for lower in ch.to_lowercase() {
            out.push(lower);
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

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
    if contains_sensitive_learning_context(dictated_text)
        || contains_sensitive_learning_context(corrected_text)
    {
        return Vec::new();
    }

    let dictated_tokens = word_tokens(dictated_text);
    let corrected_tokens = word_tokens(corrected_text);

    if dictated_tokens.len() != corrected_tokens.len() {
        return infer_split_auto_learn_candidates(
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
            replacement_of: Some(sanitized_replacement_phrase(&dictated_tokens[start..=end])),
        });
    }

    for (index, (dictated, corrected)) in dictated_tokens
        .iter()
        .zip(corrected_tokens.iter())
        .enumerate()
    {
        if tokens_equivalent_for_learning(dictated, corrected) || phrase_covered_indices[index] {
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
                replacement_of: Some(sanitized_replacement_word(dictated)),
            });
        }
    }

    candidates
}

fn infer_split_auto_learn_candidates(
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

        if tokens_equivalent_for_learning(dictated, corrected) {
            dictated_index += 1;
            corrected_index += 1;
            continue;
        }

        if let Some((phrase, dictated_end)) =
            infer_joined_tokens_correction(dictated_tokens, dictated_index, corrected)
        {
            if !is_existing_word(&phrase, existing_words)
                && !is_existing_candidate(&phrase, &candidates)
            {
                candidates.push(AutoLearnCandidate {
                    phrase,
                    replacement_of: Some(sanitized_replacement_phrase(
                        &dictated_tokens[dictated_index..dictated_end],
                    )),
                });
            }

            dictated_index = dictated_end;
            corrected_index += 1;
            continue;
        }

        let Some((phrase, corrected_end)) =
            infer_split_token_correction(dictated, corrected_tokens, corrected_index)
        else {
            return Vec::new();
        };

        if !is_existing_word(&phrase, existing_words)
            && !is_existing_candidate(&phrase, &candidates)
        {
            candidates.push(AutoLearnCandidate {
                phrase,
                replacement_of: Some(sanitized_replacement_word(dictated)),
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

fn infer_joined_tokens_correction(
    dictated_tokens: &[String],
    dictated_index: usize,
    corrected: &str,
) -> Option<(String, usize)> {
    let corrected_candidate = sanitize_custom_word_candidate(corrected)?;
    if !is_strong_joined_custom_term(&corrected_candidate) {
        return None;
    }

    let max_dictated_len =
        MAX_AUTO_LEARN_PHRASE_TOKENS.min(dictated_tokens.len().saturating_sub(dictated_index));
    let corrected_collapsed = normalize_collapsed_token_for_match(&corrected_candidate);

    for dictated_len in (2..=max_dictated_len).rev() {
        let dictated_end = dictated_index + dictated_len;
        let dictated_collapsed =
            collapse_tokens_for_match(&dictated_tokens[dictated_index..dictated_end]);

        if collapsed_tokens_are_close(&dictated_collapsed, &corrected_collapsed) {
            return Some((corrected_candidate, dictated_end));
        }
    }

    None
}

fn infer_split_token_correction(
    dictated: &str,
    corrected_tokens: &[String],
    corrected_index: usize,
) -> Option<(String, usize)> {
    infer_hyphen_part_correction(dictated, corrected_tokens, corrected_index).or_else(|| {
        infer_collapsed_token_phrase_correction(dictated, corrected_tokens, corrected_index)
    })
}

fn infer_hyphen_part_correction(
    dictated: &str,
    corrected_tokens: &[String],
    corrected_index: usize,
) -> Option<(String, usize)> {
    let dictated_parts = split_hyphenated_token(dictated);
    if dictated_parts.len() < 2 {
        return None;
    }

    let corrected_end = corrected_index + dictated_parts.len();
    if corrected_end > corrected_tokens.len() {
        return None;
    }

    let corrected_parts = &corrected_tokens[corrected_index..corrected_end];
    if !hyphen_parts_match_correction(&dictated_parts, corrected_parts) {
        return None;
    }

    let raw_phrase = corrected_parts.join(" ");
    let phrase = sanitize_custom_phrase_candidate(&raw_phrase)?;
    is_likely_custom_dictionary_phrase(&phrase).then_some((phrase, corrected_end))
}

fn infer_collapsed_token_phrase_correction(
    dictated: &str,
    corrected_tokens: &[String],
    corrected_index: usize,
) -> Option<(String, usize)> {
    let dictated_collapsed = collapse_token_for_phrase_match(dictated)?;
    let max_corrected_len =
        MAX_AUTO_LEARN_PHRASE_TOKENS.min(corrected_tokens.len().saturating_sub(corrected_index));

    for corrected_len in (2..=max_corrected_len).rev() {
        let corrected_end = corrected_index + corrected_len;
        let corrected_parts = &corrected_tokens[corrected_index..corrected_end];
        let raw_phrase = corrected_parts.join(" ");
        let Some(phrase) = sanitize_custom_phrase_candidate(&raw_phrase) else {
            continue;
        };
        if !is_likely_custom_dictionary_phrase(&phrase) {
            continue;
        }

        let corrected_collapsed = corrected_parts.join("");
        if tokens_are_close(&dictated_collapsed, &corrected_collapsed) {
            return Some((phrase, corrected_end));
        }
    }

    None
}

fn collapse_token_for_phrase_match(token: &str) -> Option<String> {
    let collapsed = normalize_collapsed_token_for_match(token);

    (collapsed.chars().count() >= 4).then_some(collapsed)
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
    let left = normalize_learning_token(left).to_lowercase();
    let right = normalize_learning_token(right).to_lowercase();

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
        .filter_map(|(index, (dictated, corrected))| {
            (!tokens_equivalent_for_learning(dictated, corrected)).then_some(index)
        })
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
        .trim_end_matches(is_sentence_boundary_punctuation)
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

fn sanitized_replacement_word(raw: &str) -> String {
    sanitize_custom_word_candidate(raw).unwrap_or_else(|| raw.to_string())
}

fn sanitized_replacement_phrase(tokens: &[String]) -> String {
    let raw_phrase = tokens.join(" ");
    sanitize_custom_phrase_candidate(&raw_phrase).unwrap_or(raw_phrase)
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
    ch.is_alphanumeric() || is_combining_mark(ch) || matches!(ch, '-' | '_' | '.' | '#' | '+')
}

fn is_sentence_boundary_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '.' | '!' | '?' | ':' | ';' | ',' | '。' | '،' | '؛' | '؟' | '！' | '？' | '，' | '、'
    )
}

fn tokens_equivalent_for_learning(left: &str, right: &str) -> bool {
    normalize_learning_token(left) == normalize_learning_token(right)
}

fn normalize_learning_token(token: &str) -> String {
    token.chars().filter(|ch| !is_combining_mark(*ch)).collect()
}

fn collapse_tokens_for_match(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| normalize_spoken_token_for_match(token))
        .collect()
}

fn normalize_spoken_token_for_match(token: &str) -> String {
    let normalized = normalize_collapsed_token_for_match(token);
    match normalized.as_str() {
        "eye" | "aye" => "i".to_string(),
        "car" => "qar".to_string(),
        _ => normalized,
    }
}

fn normalize_collapsed_token_for_match(token: &str) -> String {
    token
        .chars()
        .filter(|ch| ch.is_alphanumeric() && !is_combining_mark(*ch))
        .flat_map(char::to_lowercase)
        .collect()
}

fn collapsed_tokens_are_close(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }

    let max_chars = left.chars().count().max(right.chars().count());
    if max_chars < 4 {
        return false;
    }

    edit_distance(left, right) <= MAX_HYPHEN_PART_DISTANCE.min(max_chars / 3).max(1)
}

fn is_combining_mark(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036F
            | 0x0900..=0x0903
            | 0x093A
            | 0x093C
            | 0x0941..=0x0948
            | 0x094D
            | 0x0951..=0x0957
            | 0x0962..=0x0963
            | 0x0591..=0x05BD
            | 0x05BF
            | 0x05C1..=0x05C2
            | 0x05C4..=0x05C5
            | 0x05C7
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06DC
            | 0x06DF..=0x06E4
            | 0x06E7..=0x06E8
            | 0x06EA..=0x06ED
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
    )
}

fn contains_sensitive_learning_context(text: &str) -> bool {
    let tokens = word_tokens(text)
        .into_iter()
        .map(|token| normalize_collapsed_token_for_match(&token))
        .collect::<Vec<_>>();

    tokens.iter().enumerate().any(|(index, token)| {
        matches!(
            token.as_str(),
            "password"
                | "passcode"
                | "credential"
                | "credentials"
                | "secret"
                | "token"
                | "ssn"
                | "pin"
        ) || (token == "key"
            && index > 0
            && matches!(tokens[index - 1].as_str(), "api" | "private" | "secret"))
            || (token == "id"
                && index > 0
                && matches!(tokens[index - 1].as_str(), "national" | "medical"))
    })
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
    let has_non_cased_alphabetic = candidate
        .chars()
        .any(|ch| ch.is_alphabetic() && !ch.is_uppercase() && !ch.is_lowercase());
    let has_term_punctuation = candidate
        .chars()
        .any(|ch| matches!(ch, '-' | '_' | '.' | '#' | '+'));
    let alphabetic_count = candidate.chars().filter(|ch| ch.is_alphabetic()).count();

    has_term_punctuation
        || (has_digit && alphabetic_count > 0)
        || (has_uppercase && has_lowercase)
        || (has_uppercase && alphabetic_count >= 2)
        || (has_non_cased_alphabetic && alphabetic_count >= 2)
}

fn is_strong_joined_custom_term(candidate: &str) -> bool {
    let mut alphabetic_index = 0;
    let mut has_internal_uppercase = false;
    let mut has_digit = false;
    let mut has_term_punctuation = false;

    for ch in candidate.chars() {
        if ch.is_alphabetic() {
            if alphabetic_index > 0 && ch.is_uppercase() {
                has_internal_uppercase = true;
            }
            alphabetic_index += 1;
        }
        has_digit |= ch.is_ascii_digit();
        has_term_punctuation |= matches!(ch, '-' | '_' | '.' | '#' | '+');
    }

    has_internal_uppercase || has_digit || has_term_punctuation
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
    fn canonicalize_folds_case_whitespace_and_smart_punct() {
        assert_eq!(
            super::canonicalize("  ACME,\u{2019}s   Corp  "),
            super::canonicalize("acme,'s corp")
        );
        assert_eq!(super::canonicalize("Robyn"), "robyn");
        assert_eq!(
            super::canonicalize("Node.js"),
            super::canonicalize("node.js")
        );
        assert_eq!(
            super::canonicalize("Wi\u{2013}Fi"),
            super::canonicalize("wi-fi")
        );
        assert_eq!(super::canonicalize("a   b\t c"), "a b c");
        assert_eq!(super::canonicalize(""), "");
    }

    #[test]
    fn auto_learn_candidate_matrix_covers_realistic_scenarios() {
        struct Scenario {
            name: &'static str,
            dictated: &'static str,
            corrected: &'static str,
            existing: &'static [&'static str],
            expected_phrases: &'static [&'static str],
        }

        let scenarios = [
            Scenario {
                name: "proper noun correction",
                dictated: "meet with robin tomorrow",
                corrected: "meet with Robyn tomorrow",
                existing: &[],
                expected_phrases: &["Robyn"],
            },
            Scenario {
                name: "proper noun with sentence punctuation",
                dictated: "meet robin.",
                corrected: "meet Robyn.",
                existing: &[],
                expected_phrases: &["Robyn"],
            },
            Scenario {
                name: "proper noun already exists",
                dictated: "meet with robin tomorrow",
                corrected: "meet with Robyn tomorrow",
                existing: &["Robyn"],
                expected_phrases: &[],
            },
            Scenario {
                name: "multi word name",
                dictated: "my name is abdullah al kulaib",
                corrected: "my name is Abdullah al Kulaib",
                existing: &[],
                expected_phrases: &["Abdullah al Kulaib"],
            },
            Scenario {
                name: "multi word name already exists",
                dictated: "my name is abdullah al kulaib",
                corrected: "my name is Abdullah al Kulaib",
                existing: &["Abdullah al Kulaib"],
                expected_phrases: &[],
            },
            Scenario {
                name: "hyphenated name to spaced phrase",
                dictated: "my name is Abdullah Al-Khulayb",
                corrected: "my name is Abdullah Al Kulaib",
                existing: &[],
                expected_phrases: &["Al Kulaib"],
            },
            Scenario {
                name: "spelled hyphenated name to spaced phrase",
                dictated: "my name is A-L-K-U-L-A-Y-B",
                corrected: "my name is Al Kulaib",
                existing: &[],
                expected_phrases: &["Al Kulaib"],
            },
            Scenario {
                name: "connector phrase keeps notable token",
                dictated: "talk to al kulayb",
                corrected: "talk to al Kulaib",
                existing: &[],
                expected_phrases: &["Kulaib"],
            },
            Scenario {
                name: "arabic non cased word",
                dictated: "قابلت عبدالة اليوم",
                corrected: "قابلت عبدالله اليوم",
                existing: &[],
                expected_phrases: &["عبدالله"],
            },
            Scenario {
                name: "devanagari non cased word",
                dictated: "say namaste now",
                corrected: "say नमस्ते now",
                existing: &[],
                expected_phrases: &["नमस्ते"],
            },
            Scenario {
                name: "cjk non cased word",
                dictated: "visit tokyo today",
                corrected: "visit 東京 today",
                existing: &[],
                expected_phrases: &["東京"],
            },
            Scenario {
                name: "node dot js technical punctuation",
                dictated: "use nodejs today",
                corrected: "use Node.js today",
                existing: &[],
                expected_phrases: &["Node.js"],
            },
            Scenario {
                name: "notebook lm collapsed product",
                dictated: "open notebook LM today",
                corrected: "open NotebookLM today",
                existing: &[],
                expected_phrases: &["NotebookLM"],
            },
            Scenario {
                name: "iAqar phonetic product",
                dictated: "open eye car today",
                corrected: "open iAqar today",
                existing: &[],
                expected_phrases: &["iAqar"],
            },
            Scenario {
                name: "gpt version technical token",
                dictated: "use gpt 5.5",
                corrected: "use GPT-5.5",
                existing: &[],
                expected_phrases: &["GPT-5.5"],
            },
            Scenario {
                name: "c plus plus token",
                dictated: "use cplusplus today",
                corrected: "use C++ today",
                existing: &[],
                expected_phrases: &["C++"],
            },
            Scenario {
                name: "f sharp token",
                dictated: "use fsharp today",
                corrected: "use F# today",
                existing: &[],
                expected_phrases: &["F#"],
            },
            Scenario {
                name: "oauth collapsed token",
                dictated: "enable o auth",
                corrected: "enable OAuth",
                existing: &[],
                expected_phrases: &["OAuth"],
            },
            Scenario {
                name: "aws s3 collapsed token",
                dictated: "open aws s3",
                corrected: "open AWS_S3",
                existing: &[],
                expected_phrases: &["AWS_S3"],
            },
            Scenario {
                name: "github mixed case",
                dictated: "open github",
                corrected: "open GitHub",
                existing: &[],
                expected_phrases: &["GitHub"],
            },
            Scenario {
                name: "chatgpt collapsed product",
                dictated: "open chat gpt",
                corrected: "open ChatGPT",
                existing: &[],
                expected_phrases: &["ChatGPT"],
            },
            Scenario {
                name: "claude code collapsed product",
                dictated: "open claude code",
                corrected: "open ClaudeCode",
                existing: &[],
                expected_phrases: &["ClaudeCode"],
            },
            Scenario {
                name: "powershell collapsed product",
                dictated: "open power shell",
                corrected: "open PowerShell",
                existing: &[],
                expected_phrases: &["PowerShell"],
            },
            Scenario {
                name: "ffmpeg mixed case",
                dictated: "run ffmpeg",
                corrected: "run FFmpeg",
                existing: &[],
                expected_phrases: &["FFmpeg"],
            },
            Scenario {
                name: "youtube collapsed product",
                dictated: "open you tube",
                corrected: "open YouTube",
                existing: &[],
                expected_phrases: &["YouTube"],
            },
            Scenario {
                name: "iphone phonetic product",
                dictated: "use eye phone",
                corrected: "use iPhone",
                existing: &[],
                expected_phrases: &["iPhone"],
            },
            Scenario {
                name: "qwen model token",
                dictated: "load qwen3",
                corrected: "load Qwen3",
                existing: &[],
                expected_phrases: &["Qwen3"],
            },
            Scenario {
                name: "llama dot cpp token",
                dictated: "use llamacpp",
                corrected: "use Llama.cpp",
                existing: &[],
                expected_phrases: &["Llama.cpp"],
            },
            Scenario {
                name: "rust analyzer lowercase hyphen token",
                dictated: "run rustanalyzer",
                corrected: "run rust-analyzer",
                existing: &[],
                expected_phrases: &["rust-analyzer"],
            },
            Scenario {
                name: "z dot ai token",
                dictated: "try zai",
                corrected: "try Z.AI",
                existing: &[],
                expected_phrases: &["Z.AI"],
            },
            Scenario {
                name: "verbatim brand away from first word",
                dictated: "open verbatim now",
                corrected: "open Verbatim now",
                existing: &[],
                expected_phrases: &["Verbatim"],
            },
            Scenario {
                name: "claude proper noun",
                dictated: "ask clod now",
                corrected: "ask Claude now",
                existing: &[],
                expected_phrases: &["Claude"],
            },
            Scenario {
                name: "raycast proper noun",
                dictated: "open raycast",
                corrected: "open Raycast",
                existing: &[],
                expected_phrases: &["Raycast"],
            },
            Scenario {
                name: "k8s token",
                dictated: "deploy kates",
                corrected: "deploy K8s",
                existing: &[],
                expected_phrases: &["K8s"],
            },
            Scenario {
                name: "wifi hyphenated token",
                dictated: "join wifi",
                corrected: "join Wi-Fi",
                existing: &[],
                expected_phrases: &["Wi-Fi"],
            },
            Scenario {
                name: "typescript mixed case",
                dictated: "write typescript",
                corrected: "write TypeScript",
                existing: &[],
                expected_phrases: &["TypeScript"],
            },
            Scenario {
                name: "pytorch mixed case",
                dictated: "use pytorch",
                corrected: "use PyTorch",
                existing: &[],
                expected_phrases: &["PyTorch"],
            },
            Scenario {
                name: "numpy mixed case",
                dictated: "import numpy",
                corrected: "import NumPy",
                existing: &[],
                expected_phrases: &["NumPy"],
            },
            Scenario {
                name: "graphql mixed case",
                dictated: "query graphql",
                corrected: "query GraphQL",
                existing: &[],
                expected_phrases: &["GraphQL"],
            },
            Scenario {
                name: "openai collapsed product",
                dictated: "use open ai",
                corrected: "use OpenAI",
                existing: &[],
                expected_phrases: &["OpenAI"],
            },
            Scenario {
                name: "sentence capitalization ignored",
                dictated: "hello world",
                corrected: "Hello world",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "common spelling correction ignored",
                dictated: "open teh file",
                corrected: "open the file",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "meaning change ignored",
                dictated: "John is late",
                corrected: "John was late",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "password context ignored",
                dictated: "my password is abc123",
                corrected: "my password is ABC123",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "api key context ignored",
                dictated: "my API key is abc123",
                corrected: "my API key is ABC123",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "token context ignored",
                dictated: "the token is abc123",
                corrected: "the token is ABC123",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "private key context ignored",
                dictated: "the private key is abc123",
                corrected: "the private key is ABC123",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "medical id context ignored",
                dictated: "medical id abc123",
                corrected: "medical ID ABC123",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "pin context ignored",
                dictated: "my pin is r2",
                corrected: "my PIN is R2",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "abbreviation replacement ignored",
                dictated: "Saudi Arabia",
                corrected: "KSA",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "diacritic only correction ignored",
                dictated: "زر الرياض",
                corrected: "زر الرِّياض",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "distant split phrase ignored",
                dictated: "my name is A-L-K-U-L-A-Y-B",
                corrected: "my name is Completely Different",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "dangling hyphen phrase ignored",
                dictated: "Abdullah Al-Kulayb",
                corrected: "Abdullah Al-",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "dangling hyphen word ignored",
                dictated: "wow",
                corrected: "Vow-",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "plain punctuation cleanup ignored",
                dictated: "hello",
                corrected: "hello.",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "numeric formatting ignored",
                dictated: "twenty five",
                corrected: "25",
                existing: &[],
                expected_phrases: &[],
            },
            Scenario {
                name: "long candidate ignored",
                dictated: "use longterm",
                corrected:
                    "use SupercalifragilisticexpialidociousSupercalifragilisticexpialidocious",
                existing: &[],
                expected_phrases: &[],
            },
        ];

        assert!(scenarios.len() >= 50);

        for scenario in scenarios {
            let existing = scenario
                .existing
                .iter()
                .map(|word| word.to_string())
                .collect::<Vec<_>>();
            let phrases =
                infer_auto_learn_candidates(scenario.dictated, scenario.corrected, &existing)
                    .into_iter()
                    .map(|candidate| candidate.phrase)
                    .collect::<Vec<_>>();
            let expected = scenario
                .expected_phrases
                .iter()
                .map(|phrase| phrase.to_string())
                .collect::<Vec<_>>();

            assert_eq!(phrases, expected, "{}", scenario.name);
        }
    }

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
    fn suggests_spelled_hyphenated_name_correction_as_phrase_mapping() {
        let candidates =
            infer_auto_learn_candidates("my name is A-L-K-U-L-A-Y-B", "my name is Al Kulaib", &[]);

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "Al Kulaib".to_string(),
                replacement_of: Some("A-L-K-U-L-A-Y-B".to_string()),
            }]
        );
    }

    #[test]
    fn suggests_non_cased_script_word_correction() {
        let candidates =
            infer_auto_learn_candidates("قابلت عبدالة اليوم", "قابلت عبدالله اليوم", &[]);

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "عبدالله".to_string(),
                replacement_of: Some("عبدالة".to_string()),
            }]
        );
    }

    #[test]
    fn trims_sentence_punctuation_from_learned_word_correction() {
        let candidates = infer_auto_learn_candidates("meet robin.", "meet Robyn.", &[]);

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "Robyn".to_string(),
                replacement_of: Some("robin".to_string()),
            }]
        );
    }

    #[test]
    fn preserves_internal_technical_punctuation_in_learned_word() {
        let candidates = infer_auto_learn_candidates("use nodejs today", "use Node.js today", &[]);

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "Node.js".to_string(),
                replacement_of: Some("nodejs".to_string()),
            }]
        );
    }

    #[test]
    fn suggests_collapsed_multi_token_product_name_correction() {
        let candidates =
            infer_auto_learn_candidates("open notebook LM today", "open NotebookLM today", &[]);

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "NotebookLM".to_string(),
                replacement_of: Some("notebook LM".to_string()),
            }]
        );
    }

    #[test]
    fn suggests_mixed_case_product_name_from_phonetic_tokens() {
        let candidates = infer_auto_learn_candidates("open eye car today", "open iAqar today", &[]);

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "iAqar".to_string(),
                replacement_of: Some("eye car".to_string()),
            }]
        );
    }

    #[test]
    fn suggests_technical_token_from_split_tokens() {
        let candidates = infer_auto_learn_candidates("use gpt 5.5", "use GPT-5.5", &[]);

        assert_eq!(
            candidates,
            vec![AutoLearnCandidate {
                phrase: "GPT-5.5".to_string(),
                replacement_of: Some("gpt 5.5".to_string()),
            }]
        );
    }

    #[test]
    fn ignores_common_lowercase_spelling_corrections() {
        let candidates = infer_auto_learn_candidates("open teh file", "open the file", &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn ignores_meaning_changes_between_common_words() {
        let candidates = infer_auto_learn_candidates("John is late", "John was late", &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn ignores_sensitive_password_context_corrections() {
        let candidates =
            infer_auto_learn_candidates("my password is abc123", "my password is ABC123", &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn ignores_abbreviation_replacements_without_explicit_confirmation() {
        let candidates = infer_auto_learn_candidates("Saudi Arabia", "KSA", &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn ignores_arabic_diacritic_only_corrections() {
        let candidates = infer_auto_learn_candidates("زر الرياض", "زر الرِّياض", &[]);

        assert!(candidates.is_empty());
    }

    #[test]
    fn ignores_distant_split_token_phrase_corrections() {
        let candidates = infer_auto_learn_candidates(
            "my name is A-L-K-U-L-A-Y-B",
            "my name is Completely Different",
            &[],
        );

        assert!(candidates.is_empty());
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
