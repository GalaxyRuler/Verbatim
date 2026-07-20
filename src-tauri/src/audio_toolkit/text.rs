use natural::phonetics::soundex;
use once_cell::sync::Lazy;
use regex::Regex;
use strsim::levenshtein;
use unicode_normalization::char::is_combining_mark;

use crate::dictionary_learning::canonicalize;
use crate::settings::{DictionaryEntry, DictionaryEntrySource};

/// Builds an n-gram string by cleaning and concatenating words
///
/// Strips punctuation from each word, lowercases, and joins without spaces.
/// This allows matching "Charge B" against "ChargeBee".
fn build_ngram(words: &[&str]) -> String {
    words
        .iter()
        .map(|w| {
            canonicalize(w)
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .collect::<Vec<_>>()
        .concat()
}

/// Finds the best matching custom word for a candidate string
///
/// Uses Levenshtein distance and Soundex phonetic matching to find
/// the best match above the given threshold.
///
/// # Arguments
/// * `candidate` - The cleaned/lowercased candidate string to match
/// * `custom_words` - Original custom words (for returning the replacement)
/// * `custom_words_nospace` - Custom words with spaces removed, lowercased (for comparison)
/// * `threshold` - Maximum similarity score to accept
///
/// # Returns
/// The best matching custom word and its score, if any match was found
fn find_best_match<'a>(
    candidate: &str,
    custom_words: &'a [String],
    custom_words_nospace: &[String],
    threshold: f64,
) -> Option<(&'a String, f64)> {
    if candidate.is_empty() || candidate.len() > 50 {
        return None;
    }

    let mut best_match: Option<&String> = None;
    let mut best_score = f64::MAX;

    for (i, custom_word_nospace) in custom_words_nospace.iter().enumerate() {
        let exact_match = candidate == custom_word_nospace;
        if !exact_match
            && candidate
                .chars()
                .count()
                .min(custom_word_nospace.chars().count())
                < 3
        {
            continue;
        }

        // Skip if lengths are too different (optimization + prevents over-matching)
        // Use percentage-based check: max 25% length difference (prevents n-grams from
        // matching significantly shorter custom words, e.g., "openaigpt" vs "openai")
        let len_diff = (candidate.len() as i32 - custom_word_nospace.len() as i32).abs() as f64;
        let max_len = candidate.len().max(custom_word_nospace.len()) as f64;
        let max_allowed_diff = (max_len * 0.25).max(2.0); // At least 2 chars difference allowed
        if len_diff > max_allowed_diff {
            continue;
        }

        // Calculate Levenshtein distance (normalized by length)
        let levenshtein_dist = levenshtein(candidate, custom_word_nospace);
        let max_len = candidate.len().max(custom_word_nospace.len()) as f64;
        let levenshtein_score = if max_len > 0.0 {
            levenshtein_dist as f64 / max_len
        } else {
            1.0
        };

        // Calculate phonetic similarity using Soundex
        let phonetic_match = soundex(candidate, custom_word_nospace);

        // Combine scores: favor phonetic matches, but also consider string similarity
        let combined_score = if phonetic_match {
            levenshtein_score * 0.3 // Give significant boost to phonetic matches
        } else {
            levenshtein_score
        };

        // Accept if the score is good enough (configurable threshold)
        if combined_score < threshold && combined_score < best_score {
            best_match = Some(&custom_words[i]);
            best_score = combined_score;
        }
    }

    best_match.map(|m| (m, best_score))
}

/// Gate for non-exact dictionary replacements. Tightens the blind fuzzy
/// replace that could rewrite correct words (audit P0).
const FUZZY_MIN_TOKEN_LEN: usize = 4;
const FUZZY_STRICT_SCORE: f64 = 0.14;

fn fuzzy_replacement_allowed(original: &str, candidate: &str, score: f64) -> bool {
    if canonicalize(original) == canonicalize(candidate) {
        return true;
    }
    if original.chars().count() < FUZZY_MIN_TOKEN_LEN {
        return false;
    }
    score <= FUZZY_STRICT_SCORE
}

/// Applies custom word corrections to transcribed text using fuzzy matching
///
/// This function corrects words in the input text by finding the best matches
/// from a list of custom words using a combination of:
/// - Levenshtein distance for string similarity
/// - Soundex phonetic matching for pronunciation similarity
/// - N-gram matching for multi-word speech artifacts (e.g., "Charge B" -> "ChargeBee")
///
/// # Arguments
/// * `text` - The input text to correct
/// * `custom_words` - List of custom words to match against
/// * `threshold` - Maximum similarity score to accept (0.0 = exact match, 1.0 = any match)
///
/// # Returns
/// The corrected text with custom words applied
pub fn apply_custom_words(text: &str, custom_words: &[String], threshold: f64) -> String {
    if custom_words.is_empty() {
        return text.to_string();
    }

    // Pre-compute comparison keys without changing replacement display strings.
    let custom_word_keys: Vec<String> = custom_words.iter().map(|w| canonicalize(w)).collect();

    // Pre-compute versions with spaces removed for n-gram comparison
    let custom_words_nospace: Vec<String> = custom_word_keys
        .iter()
        .map(|w| w.replace(' ', ""))
        .collect();

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let mut matched = false;

        // Try n-grams from longest (3) to shortest (1) - greedy matching
        for n in (1..=3).rev() {
            if i + n > words.len() {
                continue;
            }

            let ngram_words = &words[i..i + n];
            let ngram = build_ngram(ngram_words);

            if let Some((replacement, score)) =
                find_best_match(&ngram, custom_words, &custom_words_nospace, threshold)
            {
                if !fuzzy_replacement_allowed(&ngram, replacement, score) {
                    continue;
                }

                // Extract punctuation from first and last words of the n-gram
                let (prefix, _) = extract_punctuation(ngram_words[0]);
                let suffix = dictionary_suffix_punctuation(ngram_words[n - 1]);

                // Preserve case from first word
                let corrected = preserve_case_pattern(ngram_words[0], replacement);

                result.push(format!("{}{}{}", prefix, corrected, suffix));
                i += n;
                matched = true;
                break;
            }
        }

        if !matched {
            result.push(words[i].to_string());
            i += 1;
        }
    }

    result.join(" ")
}

/// Stricter absolute cap for NEW auto-learned fuzzy matching. Lower = stricter
/// (accept iff combined_score < threshold; 0 = exact). Base default is 0.18.
pub const AUTO_FUZZY_THRESHOLD: f64 = 0.12;

/// Returns the fuzzy-match threshold to use for auto-learned (not yet
/// user-confirmed) dictionary entries. Always at least as strict as `base`.
pub fn auto_learned_fuzzy_threshold(base: f64) -> f64 {
    AUTO_FUZZY_THRESHOLD.min(base)
}

/// Trust tier used to decide how strict fuzzy matching should be for an entry.
///
/// Manual, Imported, and user-confirmed AutoLearned entries are trusted at the
/// base threshold. Unconfirmed AutoLearned entries are held to a stricter
/// threshold (see [`auto_learned_fuzzy_threshold`]) since they were never
/// reviewed by the user.
fn is_manual_tier(e: &DictionaryEntry) -> bool {
    e.user_confirmed
        || matches!(
            e.source,
            DictionaryEntrySource::Manual | DictionaryEntrySource::Imported
        )
}

/// Applies dictionary entries to transcribed text, honoring the `active` flag
/// and per-entry trust tier.
///
/// Only entries with `active == true` are considered. Exact `replacement_of`
/// rules are applied first (longest span wins, manual tier wins ties), then
/// fuzzy matching runs once per trust tier: manual/confirmed entries at
/// `base_threshold`, unconfirmed auto-learned entries at a stricter threshold.
pub fn apply_dictionary_entries(
    text: &str,
    entries: &[DictionaryEntry],
    base_threshold: f64,
) -> String {
    let active: Vec<&DictionaryEntry> = entries.iter().filter(|e| e.active).collect();
    if active.is_empty() {
        return text.to_string();
    }

    // 1) Exact replacement_of rules, longest span first; manual tier wins ties.
    let replaced = apply_dictionary_replacement_rules_ranked(text, &active);

    // 2) Fuzzy per trust tier: manual/user-confirmed at base, auto-learned stricter.
    let manual_words: Vec<String> = active
        .iter()
        .filter(|e| is_manual_tier(e))
        .map(|e| e.phrase.clone())
        .collect();
    let auto_words: Vec<String> = active
        .iter()
        .filter(|e| !is_manual_tier(e))
        .map(|e| e.phrase.clone())
        .collect();

    let after_manual = apply_custom_words(&replaced, &manual_words, base_threshold);
    apply_custom_words(
        &after_manual,
        &auto_words,
        auto_learned_fuzzy_threshold(base_threshold),
    )
}

fn apply_dictionary_replacement_rules_ranked(text: &str, active: &[&DictionaryEntry]) -> String {
    let mut rules = active
        .iter()
        .filter_map(|entry| {
            let replacement_of = entry.replacement_of.as_deref()?;
            let replacement_tokens = replacement_of
                .split_whitespace()
                .map(str::to_string)
                .collect::<Vec<_>>();
            if replacement_tokens.is_empty() {
                return None;
            }
            let tier_rank = if is_manual_tier(entry) { 0 } else { 1 };
            Some((replacement_tokens, entry.phrase.as_str(), tier_rank))
        })
        .collect::<Vec<_>>();

    if rules.is_empty() {
        return text.to_string();
    }

    // Longest span first; manual tier (rank 0) wins ties over auto tier (rank 1).
    // Stable sort preserves input order as the final tiebreak.
    rules.sort_by(|left, right| right.0.len().cmp(&left.0.len()).then(left.2.cmp(&right.2)));

    let words = text.split_whitespace().collect::<Vec<_>>();
    let mut result = Vec::new();
    let mut index = 0;

    while index < words.len() {
        let mut matched = false;

        for (replacement_tokens, phrase, _tier_rank) in &rules {
            let rule_len = replacement_tokens.len();
            if index + rule_len > words.len() {
                continue;
            }

            let candidate = build_ngram(&words[index..index + rule_len]);
            let expected = build_ngram(
                &replacement_tokens
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            );

            if candidate == expected {
                let (prefix, _) = extract_punctuation(words[index]);
                let suffix = dictionary_suffix_punctuation(words[index + rule_len - 1]);
                result.push(format!("{}{}{}", prefix, phrase, suffix));
                index += rule_len;
                matched = true;
                break;
            }
        }

        if !matched {
            result.push(words[index].to_string());
            index += 1;
        }
    }

    result.join(" ")
}

/// Preserves the case pattern of the original word when applying a replacement
fn preserve_case_pattern(original: &str, replacement: &str) -> String {
    let has_cased = original
        .chars()
        .any(|c| c.is_uppercase() || c.is_lowercase());
    if has_cased
        && original
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(|c| c.is_uppercase())
    {
        replacement.to_uppercase()
    } else if original.chars().next().map_or(false, |c| c.is_uppercase()) {
        let mut chars: Vec<char> = replacement.chars().collect();
        if let Some(first_char) = chars.get_mut(0) {
            *first_char = first_char.to_uppercase().next().unwrap_or(*first_char);
        }
        chars.into_iter().collect()
    } else {
        replacement.to_string()
    }
}

/// Extracts punctuation prefix and suffix from a word
fn extract_punctuation(word: &str) -> (&str, &str) {
    let prefix_end = word
        .char_indices()
        .find(|(_, character)| character.is_alphanumeric())
        .map(|(offset, _)| offset)
        .unwrap_or(word.len());
    let suffix_start = word
        .char_indices()
        .rev()
        .find(|(_, character)| character.is_alphanumeric())
        .map(|(offset, character)| offset + character.len_utf8())
        .unwrap_or(0);

    (&word[..prefix_end], &word[suffix_start..])
}

/// Dictionary replacements already carry their own composed display spelling.
/// Do not append a trailing combining mark from a canonically equivalent input
/// a second time, but keep the general punctuation helper's legacy contract.
fn dictionary_suffix_punctuation(word: &str) -> &str {
    let (_, suffix) = extract_punctuation(word);
    let suffix_offset = word.len() - suffix.len();
    if !word[..suffix_offset]
        .chars()
        .last()
        .is_some_and(char::is_alphanumeric)
    {
        return suffix;
    }

    let punctuation_offset = suffix
        .char_indices()
        .find_map(|(offset, character)| (!is_combining_mark(character)).then_some(offset))
        .unwrap_or(suffix.len());
    &suffix[punctuation_offset..]
}

/// Returns filler words appropriate for the given language code.
///
/// Some words like "um" and "ha" are real words in certain languages
/// (e.g., Portuguese "um" = "a/an", Spanish "ha" = "has"), so we only
/// include them as fillers for languages where they are truly fillers.
fn get_filler_words_for_language(lang: &str) -> &'static [&'static str] {
    let base_lang = lang.split(&['-', '_'][..]).next().unwrap_or(lang);

    match base_lang {
        "en" => &[
            "uh", "um", "uhm", "umm", "uhh", "uhhh", "ah", "hmm", "hm", "mmm", "mm", "mh", "eh",
            "ehh", "ha",
        ],
        "es" => &["ehm", "mmm", "hmm", "hm"],
        "pt" => &["ahm", "hmm", "mmm", "hm"],
        "fr" => &["euh", "hmm", "hm", "mmm"],
        "de" => &["äh", "ähm", "hmm", "hm", "mmm"],
        "it" => &["ehm", "hmm", "mmm", "hm"],
        "cs" => &["ehm", "hmm", "mmm", "hm"],
        "pl" => &["hmm", "mmm", "hm"],
        "tr" => &["hmm", "mmm", "hm"],
        "ru" => &["хм", "ммм", "hmm", "mmm"],
        "uk" => &["хм", "ммм", "hmm", "mmm"],
        "ar" => &["hmm", "mmm"],
        "ja" => &["hmm", "mmm"],
        "ko" => &["hmm", "mmm"],
        "vi" => &["hmm", "mmm", "hm"],
        "zh" => &["hmm", "mmm"],
        // Conservative universal fallback (no "um", "eh", "ha")
        _ => &[
            "uh", "uhm", "umm", "uhh", "uhhh", "ah", "hmm", "hm", "mmm", "mm", "mh", "ehh",
        ],
    }
}

static MULTI_SPACE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s{2,}").unwrap());

/// Collapses repeated words (3+ repetitions) to a single instance.
/// E.g., "wh wh wh wh" -> "wh", "I I I I" -> "I"
fn collapse_stutters(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return text.to_string();
    }

    let mut result: Vec<&str> = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let word = words[i];
        let word_lower = word.to_lowercase();

        if word_lower.chars().all(|c| c.is_alphabetic()) {
            // Count consecutive repetitions (case-insensitive)
            let mut count = 1;
            while i + count < words.len() && words[i + count].to_lowercase() == word_lower {
                count += 1;
            }

            // If 3+ repetitions, collapse to single instance
            if count >= 3 {
                result.push(word);
                i += count;
            } else {
                result.push(word);
                i += 1;
            }
        } else {
            result.push(word);
            i += 1;
        }
    }

    result.join(" ")
}

/// Filters transcription output by removing filler words and stutter artifacts.
///
/// This function cleans up raw transcription text by:
/// 1. Removing filler words based on a validated dictation language (or custom list)
/// 2. Collapsing repeated word stutters (e.g., "wh wh wh" -> "wh")
/// 3. Cleaning up excess whitespace
///
/// # Arguments
/// * `text` - The raw transcription text to filter
/// * `lang` - A validated locked dictation language (e.g., "en", "pt-BR"). `None`
///   skips language-default filler removal.
/// * `custom_filler_words` - Optional user-provided filler word list. `Some(vec)` overrides
///   language defaults; `Some(empty vec)` disables filler removal; `None` uses language
///   defaults only when `lang` is `Some`.
///
/// # Returns
/// The filtered text with filler words and stutters removed
pub fn filter_transcription_output(
    text: &str,
    lang: Option<&str>,
    custom_filler_words: &Option<Vec<String>>,
) -> String {
    let mut filtered = text.to_string();

    // Build filler patterns from custom list or language defaults
    let patterns: Vec<Regex> = if let Some(words) = custom_filler_words {
        words
            .iter()
            .filter_map(|word| Regex::new(&format!(r"(?i)\b{}\b[,.]?", regex::escape(word))).ok())
            .collect()
    } else if let Some(lang) = lang {
        get_filler_words_for_language(lang)
            .iter()
            .map(|word| Regex::new(&format!(r"(?i)\b{}\b[,.]?", regex::escape(word))).unwrap())
            .collect()
    } else {
        Vec::new()
    };

    // Remove filler words
    for pattern in &patterns {
        filtered = pattern.replace_all(&filtered, "").to_string();
    }

    // Collapse repeated 1-2 letter words (stutter artifacts like "wh wh wh wh")
    filtered = collapse_stutters(&filtered);

    // Clean up multiple spaces to single space
    filtered = MULTI_SPACE_PATTERN.replace_all(&filtered, " ").to_string();

    // Trim leading/trailing whitespace
    filtered.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{DictionaryEntry, DictionaryEntryPriority, DictionaryEntrySource};

    #[test]
    fn test_apply_custom_words_exact_match() {
        let text = "hello world";
        let custom_words = vec!["Hello".to_string(), "World".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_apply_custom_words_fuzzy_match() {
        let text = "helo wrold";
        let custom_words = vec!["hello".to_string(), "world".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn fuzzy_dictionary_match_uses_nfc_identity() {
        let custom_words = vec!["Caf\u{e9}".to_string()];

        assert_eq!(
            apply_custom_words("Cafe\u{301}", &custom_words, 0.01),
            "Caf\u{e9}"
        );
    }

    #[test]
    fn short_tokens_require_exact_match() {
        assert!(!fuzzy_replacement_allowed("the", "Théa", 0.10));
        assert!(fuzzy_replacement_allowed("thea", "Théa", 0.0));
    }

    #[test]
    fn fuzzy_gate_requires_min_length_and_score() {
        assert!(fuzzy_replacement_allowed("kubernets", "kubernetes", 0.11));
        assert!(!fuzzy_replacement_allowed("robin", "Robyn", 0.17));
    }

    #[test]
    fn preserve_case_handles_caseless_tokens() {
        assert_eq!(preserve_case_pattern("123", "Robyn"), "Robyn");
        assert_eq!(preserve_case_pattern("HELLO", "robyn"), "ROBYN");
        assert_eq!(preserve_case_pattern("Hello", "robyn"), "Robyn");
    }

    #[test]
    fn test_preserve_case_pattern() {
        assert_eq!(preserve_case_pattern("HELLO", "world"), "WORLD");
        assert_eq!(preserve_case_pattern("Hello", "world"), "World");
        assert_eq!(preserve_case_pattern("hello", "WORLD"), "WORLD");
    }

    #[test]
    fn test_extract_punctuation() {
        assert_eq!(extract_punctuation("hello"), ("", ""));
        assert_eq!(extract_punctuation("!hello?"), ("!", "?"));
        assert_eq!(extract_punctuation("...hello..."), ("...", "..."));
    }

    #[test]
    fn extract_punctuation_handles_arabic_punctuation() {
        assert_eq!(extract_punctuation("،مرحبا؟"), ("،", "؟"));
    }

    #[test]
    fn extract_punctuation_handles_combining_marks() {
        assert_eq!(
            extract_punctuation("\u{301}e\u{301}"),
            ("\u{301}", "\u{301}")
        );
    }

    #[test]
    fn extract_punctuation_handles_emoji() {
        assert_eq!(extract_punctuation("🔥hello🙂"), ("🔥", "🙂"));
    }

    #[test]
    fn extract_punctuation_handles_mixed_multibyte_edges() {
        assert_eq!(extract_punctuation("(🔥مرحبا؟!)"), ("(🔥", "؟!)"));
    }

    #[test]
    fn extract_punctuation_preserves_ascii_regression() {
        assert_eq!(extract_punctuation("[...hello?!]"), ("[...", "?!]"));
    }

    #[test]
    fn test_empty_custom_words() {
        let text = "hello world";
        let custom_words = vec![];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn auto_threshold_is_stricter_and_clamped() {
        assert!((auto_learned_fuzzy_threshold(0.18) - 0.12).abs() < f64::EPSILON);
        assert!((auto_learned_fuzzy_threshold(0.10) - 0.10).abs() < f64::EPSILON);
        // never looser than base
    }

    #[test]
    fn apply_dictionary_entries_uses_replacement_rule() {
        let result = apply_dictionary_entries(
            "robin joined",
            &[dictionary_entry("Robyn", Some("robin"))],
            0.18,
        );

        assert_eq!(result, "Robyn joined");
    }

    #[test]
    fn exact_dictionary_rule_uses_nfc_identity() {
        let result = apply_dictionary_entries(
            "\u{627}\u{654}\u{62d}\u{645}\u{62f} joined",
            &[dictionary_entry(
                "\u{623}\u{62d}\u{645}\u{62f}",
                Some("\u{623}\u{62d}\u{645}\u{62f}"),
            )],
            0.18,
        );

        assert_eq!(result, "\u{623}\u{62d}\u{645}\u{62f} joined");
    }

    #[test]
    fn apply_dictionary_entries_handles_multi_word_replacement() {
        let result = apply_dictionary_entries(
            "abdullah al kulaib spoke",
            &[dictionary_entry(
                "Abdullah al Kulaib",
                Some("abdullah al kulaib"),
            )],
            0.18,
        );

        assert_eq!(result, "Abdullah al Kulaib spoke");
    }

    #[test]
    fn apply_dictionary_entries_handles_hyphenated_name_replacement() {
        let result = apply_dictionary_entries(
            "Abdullah Al-Khulayb joined",
            &[dictionary_entry("Al Kulaib", Some("Al-Khulayb"))],
            0.18,
        );

        assert_eq!(result, "Abdullah Al Kulaib joined");
    }

    #[test]
    fn apply_dictionary_entries_handles_spelled_letter_replacement() {
        let result = apply_dictionary_entries(
            "A-L-K-U-L-A-Y-B joined",
            &[dictionary_entry("Al Kulaib", Some("A-L-K-U-L-A-Y-B"))],
            0.18,
        );

        assert_eq!(result, "Al Kulaib joined");
    }

    #[test]
    fn apply_dictionary_entries_preserves_sentence_punctuation() {
        let result = apply_dictionary_entries(
            "robin, joined.",
            &[dictionary_entry("Robyn", Some("robin"))],
            0.18,
        );

        assert_eq!(result, "Robyn, joined.");
    }

    #[test]
    fn apply_dictionary_entries_does_not_overmatch_short_words() {
        let result = apply_dictionary_entries("a test", &[dictionary_entry("AI", None)], 0.5);

        assert_eq!(result, "a test");
    }

    #[test]
    fn apply_dictionary_entries_preserves_exact_short_term_matches() {
        let result = apply_dictionary_entries("ai test", &[dictionary_entry("AI", None)], 0.5);

        assert_eq!(result, "AI test");
    }

    #[test]
    fn apply_dictionary_entries_falls_back_to_custom_word_fuzzy_match() {
        let result =
            apply_dictionary_entries("charge b", &[dictionary_entry("ChargeBee", None)], 0.5);

        assert_eq!(result, "ChargeBee");
    }

    #[test]
    fn apply_excludes_inactive_entries() {
        let mut e = dictionary_entry("Robyn", Some("robin"));
        e.active = false;
        assert_eq!(
            apply_dictionary_entries("meet robin", &[e], 0.18),
            "meet robin"
        );
    }

    #[test]
    fn apply_exact_rule_beats_fuzzy_and_uses_original_tokens() {
        let e = dictionary_entry("Node.js", Some("nodejs"));
        assert_eq!(
            apply_dictionary_entries("use nodejs today", &[e], 0.18),
            "use Node.js today"
        );
    }

    #[test]
    fn apply_auto_entry_uses_stricter_threshold_than_manual() {
        let mut auto = dictionary_entry("Postgres", None);
        auto.source = DictionaryEntrySource::AutoLearned;
        // manual-tier entry with same phrase rewrites at base threshold 0.18:
        let manual = dictionary_entry("Postgres", None); // source Manual by helper default
        assert_eq!(
            apply_dictionary_entries("posgres is running", &[manual], 0.18),
            "Postgres is running"
        );
        // auto tier at base 0.18 -> effective 0.12 -> rejected (score is 0.125):
        assert_eq!(
            apply_dictionary_entries("posgres is running", &[auto], 0.18),
            "posgres is running"
        );
    }

    #[test]
    fn user_confirmed_auto_entry_gets_manual_tier_fuzzy() {
        let mut e = dictionary_entry("Postgres", None);
        e.source = DictionaryEntrySource::AutoLearned;
        e.user_confirmed = true; // manual-tier trust
        assert_eq!(
            apply_dictionary_entries("posgres is running", &[e], 0.18),
            "Postgres is running"
        );
    }

    #[test]
    fn test_filter_filler_words() {
        let text = "So uhm I was thinking uh about this";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "So I was thinking about this");
    }

    fn dictionary_entry(phrase: &str, replacement_of: Option<&str>) -> DictionaryEntry {
        DictionaryEntry {
            id: format!("dict_test_{}", phrase),
            phrase: phrase.to_string(),
            replacement_of: replacement_of.map(str::to_string),
            source: DictionaryEntrySource::Manual,
            priority: DictionaryEntryPriority::Normal,
            created_at_ms: 1,
            updated_at_ms: 1,
            active: true,
            user_confirmed: false,
            needs_review: false,
        }
    }

    #[test]
    fn test_filter_filler_words_case_insensitive() {
        let text = "UHM this is UH a test";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "this is a test");
    }

    #[test]
    fn test_filter_filler_words_with_punctuation() {
        let text = "Well, uhm, I think, uh. that's right";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "Well, I think, that's right");
    }

    #[test]
    fn test_filter_cleans_whitespace() {
        let text = "Hello    world   test";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "Hello world test");
    }

    #[test]
    fn test_filter_trims() {
        let text = "  Hello world  ";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_filter_combined() {
        let text = "  Uhm, so I was, uh, thinking about this  ";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "so I was, thinking about this");
    }

    #[test]
    fn test_filter_preserves_valid_text() {
        let text = "This is a completely normal sentence.";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "This is a completely normal sentence.");
    }

    #[test]
    fn test_filter_stutter_collapse() {
        let text = "w wh wh wh wh wh wh wh wh wh why";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "w wh why");
    }

    #[test]
    fn test_filter_stutter_short_words() {
        let text = "I I I I think so so so so";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "I think so");
    }

    #[test]
    fn test_filter_stutter_longer_words() {
        let text = "Check data doc doc doc doc documentation.";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "Check data doc documentation.");
    }

    #[test]
    fn test_filter_stutter_mixed_case() {
        let text = "No NO no NO no";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "No");
    }

    #[test]
    fn test_filter_stutter_preserves_two_repetitions() {
        let text = "no no is fine";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "no no is fine");
    }

    #[test]
    fn test_filter_english_removes_um() {
        let text = "um I think um this is good";
        let result = filter_transcription_output(text, Some("en"), &None);
        assert_eq!(result, "I think this is good");
    }

    #[test]
    fn test_filter_portuguese_preserves_um() {
        // "um" means "a/an" in Portuguese
        let text = "um gato bonito";
        let result = filter_transcription_output(text, Some("pt"), &None);
        assert_eq!(result, "um gato bonito");
    }

    #[test]
    fn test_filter_spanish_preserves_ha() {
        // "ha" means "has" in Spanish
        let text = "ha sido un buen día";
        let result = filter_transcription_output(text, Some("es"), &None);
        assert_eq!(result, "ha sido un buen día");
    }

    #[test]
    fn test_filter_language_code_with_region() {
        // "pt-BR" should normalize to "pt"
        let text = "um gato bonito";
        let result = filter_transcription_output(text, Some("pt-BR"), &None);
        assert_eq!(result, "um gato bonito");
    }

    #[test]
    fn test_filter_custom_filler_words_override() {
        let custom = Some(vec!["okay".to_string(), "right".to_string()]);
        let text = "okay so I think right this works";
        let result = filter_transcription_output(text, Some("en"), &custom);
        assert_eq!(result, "so I think this works");
    }

    #[test]
    fn test_filter_custom_filler_words_empty_disables() {
        let custom = Some(vec![]);
        let text = "So uhm I was thinking uh about this";
        let result = filter_transcription_output(text, Some("en"), &custom);
        // No filler words removed since custom list is empty
        assert_eq!(result, "So uhm I was thinking uh about this");
    }

    #[test]
    fn test_filter_unknown_language_uses_fallback() {
        let text = "uh I think uhm this works";
        let result = filter_transcription_output(text, Some("xx"), &None);
        assert_eq!(result, "I think this works");
    }

    #[test]
    fn test_filter_fallback_does_not_remove_um() {
        // Fallback (unknown language) should not remove "um" since it's a real word in some languages
        let text = "um I think this works";
        let result = filter_transcription_output(text, Some("xx"), &None);
        assert_eq!(result, "um I think this works");
    }

    #[test]
    fn no_validated_language_skips_default_fillers() {
        let result = filter_transcription_output("um this stays", None, &None);

        assert_eq!(result, "um this stays");
    }

    #[test]
    fn custom_fillers_apply_without_a_validated_language() {
        let custom = Some(vec!["deliberate".to_string()]);
        let result = filter_transcription_output("keep deliberate words", None, &custom);

        assert_eq!(result, "keep words");
    }

    #[test]
    fn test_apply_custom_words_ngram_two_words() {
        let text = "il cui nome è Charge B,";
        let custom_words = vec!["ChargeBee".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "il cui nome è ChargeBee,");
    }

    #[test]
    fn test_apply_custom_words_ngram_three_words() {
        let text = "use Chat G P T for this";
        let custom_words = vec!["ChatGPT".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("ChatGPT"));
    }

    #[test]
    fn test_apply_custom_words_prefers_longer_ngram() {
        let text = "Open AI GPT model";
        let custom_words = vec!["OpenAI".to_string(), "GPT".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "OpenAI GPT model");
    }

    #[test]
    fn test_apply_custom_words_ngram_preserves_case() {
        let text = "CHARGE B is great";
        let custom_words = vec!["ChargeBee".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("CHARGEBEE"));
    }

    #[test]
    fn test_apply_custom_words_ngram_with_spaces_in_custom() {
        // Custom word with space should also match against split words
        let text = "using Mac Book Pro";
        let custom_words = vec!["MacBook Pro".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("MacBook"));
    }

    #[test]
    fn test_apply_custom_words_trailing_number_not_doubled() {
        // Verify that trailing non-alpha chars (like numbers) aren't double-counted
        // between build_ngram stripping them and extract_punctuation capturing them
        let text = "use GPT4 for this";
        let custom_words = vec!["GPT-4".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        // Should NOT produce "GPT-44" (double-counting the trailing 4)
        assert!(
            !result.contains("GPT-44"),
            "got double-counted result: {}",
            result
        );
    }
}
