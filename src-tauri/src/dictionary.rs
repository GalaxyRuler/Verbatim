use crate::settings::{
    AppSettings, DictionaryEntry, DictionaryEntryPriority, DictionaryEntrySource,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_DICTIONARY_PHRASE_CHARS: usize = 120;

pub fn sanitize_dictionary_phrase(raw: &str) -> Option<String> {
    let cleaned = raw
        .split_whitespace()
        .map(|token| {
            token
                .chars()
                .filter(|ch| !matches!(ch, '<' | '>' | '"' | '\'' | '&'))
                .collect::<String>()
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if cleaned.is_empty()
        || cleaned.chars().count() > MAX_DICTIONARY_PHRASE_CHARS
        || !cleaned.chars().any(char::is_alphabetic)
        || !dictionary_phrase_has_valid_token_boundaries(&cleaned)
    {
        return None;
    }

    Some(cleaned)
}

pub fn normalize_dictionary_key(raw: &str) -> String {
    sanitize_dictionary_phrase(raw)
        .unwrap_or_default()
        .to_lowercase()
}

pub fn make_dictionary_entry_id(now_ms: u64, phrase: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for ch in phrase.chars() {
        if slug.len() >= 32 {
            break;
        }

        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_separator = false;
        } else if !last_was_separator && !slug.is_empty() {
            slug.push('-');
            last_was_separator = true;
        }
    }

    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "entry" } else { slug };

    format!("dict_{}_{}", now_ms, slug)
}

pub fn dictionary_phrases(entries: &[DictionaryEntry]) -> Vec<String> {
    entries.iter().map(|entry| entry.phrase.clone()).collect()
}

pub fn sync_legacy_custom_words(settings: &mut AppSettings) -> bool {
    let mut changed = false;

    let before_len = settings.dictionary_entries.len();
    settings.dictionary_entries.retain(|entry| {
        entry.source != DictionaryEntrySource::AutoLearned
            || auto_learn_phrase_is_valid(&entry.phrase)
    });
    if settings.dictionary_entries.len() != before_len {
        changed = true;
    }

    if settings.dictionary_entries.is_empty() && !settings.custom_words.is_empty() {
        let mut entries = Vec::new();
        for (index, word) in settings.custom_words.iter().enumerate() {
            let Some(phrase) = sanitize_dictionary_phrase(word) else {
                continue;
            };

            if has_phrase(&entries, &phrase, None) {
                continue;
            }

            entries.push(DictionaryEntry {
                id: make_dictionary_entry_id(index as u64, &phrase),
                phrase,
                replacement_of: None,
                source: DictionaryEntrySource::Manual,
                priority: DictionaryEntryPriority::Normal,
                created_at_ms: 0,
                updated_at_ms: 0,
            });
        }

        if !entries.is_empty() {
            settings.dictionary_entries = entries;
            changed = true;
        }
    }

    let phrases = dictionary_phrases(&settings.dictionary_entries);
    if settings.custom_words != phrases {
        settings.custom_words = phrases;
        changed = true;
    }

    changed
}

pub fn upsert_manual_entry(
    settings: &mut AppSettings,
    now_ms: u64,
    phrase: String,
    replacement_of: Option<String>,
) -> Result<DictionaryEntry, String> {
    let phrase = sanitize_dictionary_phrase(&phrase).ok_or("Dictionary entry is empty")?;
    if has_phrase(&settings.dictionary_entries, &phrase, None) {
        return Err(format!("{} is already in your dictionary", phrase));
    }

    let entry = DictionaryEntry {
        id: make_dictionary_entry_id(now_ms, &phrase),
        phrase,
        replacement_of: sanitize_optional_phrase(replacement_of),
        source: DictionaryEntrySource::Manual,
        priority: DictionaryEntryPriority::Normal,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
    };

    settings.dictionary_entries.push(entry.clone());
    sync_legacy_custom_words(settings);

    Ok(entry)
}

pub fn upsert_auto_learn_entry(
    settings: &mut AppSettings,
    now_ms: u64,
    phrase: String,
    replacement_of: Option<String>,
) -> Result<Option<DictionaryEntry>, String> {
    let Some(phrase) = sanitize_auto_learn_phrase(&phrase) else {
        return Ok(None);
    };
    if has_phrase(&settings.dictionary_entries, &phrase, None) {
        return Ok(None);
    }

    let entry = DictionaryEntry {
        id: make_dictionary_entry_id(now_ms, &phrase),
        phrase,
        replacement_of: sanitize_optional_phrase(replacement_of),
        source: DictionaryEntrySource::AutoLearned,
        priority: DictionaryEntryPriority::Normal,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
    };

    settings.dictionary_entries.push(entry.clone());
    sync_legacy_custom_words(settings);

    Ok(Some(entry))
}

pub fn update_entry(
    settings: &mut AppSettings,
    now_ms: u64,
    id: &str,
    phrase: Option<String>,
    replacement_of: Option<Option<String>>,
    priority: Option<DictionaryEntryPriority>,
) -> Result<DictionaryEntry, String> {
    let index = settings
        .dictionary_entries
        .iter()
        .position(|entry| entry.id == id)
        .ok_or("Dictionary entry not found")?;

    if let Some(next_phrase) = phrase.as_deref().and_then(sanitize_dictionary_phrase) {
        if has_phrase(&settings.dictionary_entries, &next_phrase, Some(id)) {
            return Err(format!("{} is already in your dictionary", next_phrase));
        }
    }

    let entry = &mut settings.dictionary_entries[index];

    if let Some(next_phrase) = phrase {
        entry.phrase =
            sanitize_dictionary_phrase(&next_phrase).ok_or("Dictionary entry is empty")?;
    }

    if let Some(next_replacement) = replacement_of {
        entry.replacement_of = sanitize_optional_phrase(next_replacement);
    }

    if let Some(next_priority) = priority {
        entry.priority = next_priority;
    }

    entry.updated_at_ms = now_ms;
    let updated = entry.clone();
    sync_legacy_custom_words(settings);

    Ok(updated)
}

pub fn delete_entries(settings: &mut AppSettings, ids: &[String]) -> Vec<DictionaryEntry> {
    let mut deleted = Vec::new();
    settings.dictionary_entries.retain(|entry| {
        if ids.iter().any(|id| id == &entry.id) {
            deleted.push(entry.clone());
            false
        } else {
            true
        }
    });

    if !deleted.is_empty() {
        sync_legacy_custom_words(settings);
    }

    deleted
}

pub fn replace_dictionary_phrases(
    settings: &mut AppSettings,
    now_ms: u64,
    phrases: Vec<String>,
) -> Vec<DictionaryEntry> {
    let mut desired_phrases = Vec::new();
    for phrase in phrases {
        let Some(phrase) = sanitize_dictionary_phrase(&phrase) else {
            continue;
        };
        if !desired_phrases.iter().any(|existing: &String| {
            normalize_dictionary_key(existing) == normalize_dictionary_key(&phrase)
        }) {
            desired_phrases.push(phrase);
        }
    }

    settings.dictionary_entries.retain(|entry| {
        desired_phrases.iter().any(|phrase| {
            normalize_dictionary_key(phrase) == normalize_dictionary_key(&entry.phrase)
        })
    });

    for phrase in desired_phrases {
        if has_phrase(&settings.dictionary_entries, &phrase, None) {
            continue;
        }

        settings.dictionary_entries.push(DictionaryEntry {
            id: make_dictionary_entry_id(now_ms, &phrase),
            phrase,
            replacement_of: None,
            source: DictionaryEntrySource::Manual,
            priority: DictionaryEntryPriority::Normal,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        });
    }

    sync_legacy_custom_words(settings);
    settings.dictionary_entries.clone()
}

pub fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn sanitize_optional_phrase(raw: Option<String>) -> Option<String> {
    raw.and_then(|value| sanitize_dictionary_phrase(&value))
}

fn sanitize_auto_learn_phrase(raw: &str) -> Option<String> {
    let phrase = sanitize_dictionary_phrase(raw)?;
    auto_learn_phrase_is_valid(&phrase).then_some(phrase)
}

fn auto_learn_phrase_is_valid(phrase: &str) -> bool {
    dictionary_phrase_has_valid_token_boundaries(phrase)
}

fn dictionary_phrase_has_valid_token_boundaries(phrase: &str) -> bool {
    phrase
        .split_whitespace()
        .all(|token| !token.starts_with('-') && !token.ends_with('-'))
}

fn has_phrase(entries: &[DictionaryEntry], phrase: &str, except_id: Option<&str>) -> bool {
    let key = normalize_dictionary_key(phrase);
    entries.iter().any(|entry| {
        except_id.is_none_or(|id| entry.id != id) && normalize_dictionary_key(&entry.phrase) == key
    })
}

#[cfg(test)]
mod tests {
    use super::{
        delete_entries, dictionary_phrases, make_dictionary_entry_id, replace_dictionary_phrases,
        sanitize_dictionary_phrase, sync_legacy_custom_words, update_entry,
        upsert_auto_learn_entry, upsert_manual_entry,
    };
    use crate::settings::{
        get_default_settings, DictionaryEntry, DictionaryEntryPriority, DictionaryEntrySource,
    };

    #[test]
    fn sanitize_dictionary_phrase_allows_multi_word_names() {
        assert_eq!(
            sanitize_dictionary_phrase("Abdullah   al   Kulaib"),
            Some("Abdullah al Kulaib".to_string())
        );
    }

    #[test]
    fn sanitize_dictionary_phrase_removes_html_sensitive_chars() {
        assert_eq!(
            sanitize_dictionary_phrase("<Robyn&>"),
            Some("Robyn".to_string())
        );
    }

    #[test]
    fn sanitize_dictionary_phrase_allows_internal_hyphen_names() {
        assert_eq!(
            sanitize_dictionary_phrase("Jean-Luc Picard"),
            Some("Jean-Luc Picard".to_string())
        );
    }

    #[test]
    fn sanitize_dictionary_phrase_rejects_dangling_hyphen_words() {
        assert_eq!(sanitize_dictionary_phrase("Vow-"), None);
        assert_eq!(sanitize_dictionary_phrase("-Vow"), None);
        assert_eq!(sanitize_dictionary_phrase("Robyn -"), None);
    }

    #[test]
    fn make_dictionary_entry_id_is_deterministic_for_tests() {
        assert_eq!(
            make_dictionary_entry_id(42, "Abdullah al Kulaib"),
            "dict_42_abdullah-al-kulaib"
        );
    }

    #[test]
    fn dictionary_phrases_returns_entry_phrases() {
        let entries = vec![entry("dict_1_robyn", "Robyn")];

        assert_eq!(dictionary_phrases(&entries), vec!["Robyn"]);
    }

    #[test]
    fn sync_legacy_custom_words_migrates_old_settings() {
        let mut settings = get_default_settings();
        settings.custom_words = vec!["Robyn".to_string(), "Abdullah al Kulaib".to_string()];

        let changed = sync_legacy_custom_words(&mut settings);

        assert!(changed);
        assert_eq!(settings.dictionary_entries.len(), 2);
        assert_eq!(settings.dictionary_entries[0].phrase, "Robyn");
        assert_eq!(
            settings.dictionary_entries[0].source,
            DictionaryEntrySource::Manual
        );
        assert_eq!(
            settings.custom_words,
            vec!["Robyn".to_string(), "Abdullah al Kulaib".to_string()]
        );
    }

    #[test]
    fn sync_legacy_custom_words_uses_dictionary_entries_when_both_exist() {
        let mut settings = get_default_settings();
        settings.custom_words = vec!["Wrong".to_string()];
        settings.dictionary_entries = vec![entry("dict_1_robyn", "Robyn")];

        let changed = sync_legacy_custom_words(&mut settings);

        assert!(changed);
        assert_eq!(settings.custom_words, vec!["Robyn"]);
    }

    #[test]
    fn sync_legacy_custom_words_removes_dangling_hyphen_auto_learned_entries() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry("dict_1_vow", "Vow-")
            },
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry("dict_2_robyn", "Robyn")
            },
        ];
        settings.custom_words = vec!["Vow-".to_string(), "Robyn".to_string()];

        let changed = sync_legacy_custom_words(&mut settings);

        assert!(changed);
        assert_eq!(settings.dictionary_entries.len(), 1);
        assert_eq!(settings.dictionary_entries[0].phrase, "Robyn");
        assert_eq!(settings.custom_words, vec!["Robyn"]);
    }

    #[test]
    fn sync_legacy_custom_words_rejects_dangling_hyphen_legacy_words_as_manual() {
        let mut settings = get_default_settings();
        settings.custom_words = vec!["Vow-".to_string(), "Robyn".to_string()];

        let changed = sync_legacy_custom_words(&mut settings);

        assert!(changed);
        assert_eq!(settings.dictionary_entries.len(), 1);
        assert_eq!(settings.dictionary_entries[0].phrase, "Robyn");
        assert_eq!(
            settings.dictionary_entries[0].source,
            DictionaryEntrySource::Manual
        );
        assert_eq!(settings.custom_words, vec!["Robyn"]);
    }

    #[test]
    fn upsert_manual_entry_rejects_case_duplicate() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![entry("dict_1_robyn", "Robyn")];

        let result = upsert_manual_entry(&mut settings, 42, "robyn".to_string(), None);

        assert!(result.is_err());
    }

    #[test]
    fn upsert_manual_entry_rejects_dangling_hyphen_words() {
        let mut settings = get_default_settings();

        let result = upsert_manual_entry(&mut settings, 42, "Vow-".to_string(), None);

        assert!(result.is_err());
        assert!(settings.dictionary_entries.is_empty());
        assert!(settings.custom_words.is_empty());
    }

    #[test]
    fn update_entry_updates_phrase_and_legacy_custom_words() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![entry("dict_1_robyn", "Robyn")];
        sync_legacy_custom_words(&mut settings);

        let updated = update_entry(
            &mut settings,
            100,
            "dict_1_robyn",
            Some("Robinette".to_string()),
            None,
            None,
        )
        .expect("updated entry");

        assert_eq!(updated.phrase, "Robinette");
        assert_eq!(settings.custom_words, vec!["Robinette"]);
    }

    #[test]
    fn delete_entries_removes_entries_and_legacy_custom_words() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![
            entry("dict_1_robyn", "Robyn"),
            entry("dict_2_kulaib", "Abdullah al Kulaib"),
        ];
        sync_legacy_custom_words(&mut settings);

        let deleted = delete_entries(&mut settings, &["dict_1_robyn".to_string()]);

        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].phrase, "Robyn");
        assert_eq!(settings.custom_words, vec!["Abdullah al Kulaib"]);
    }

    #[test]
    fn upsert_auto_learn_entry_marks_source_and_replacement() {
        let mut settings = get_default_settings();

        let learned = upsert_auto_learn_entry(
            &mut settings,
            42,
            "Robyn".to_string(),
            Some("robin".to_string()),
        )
        .expect("auto learn")
        .expect("new entry");

        assert_eq!(learned.source, DictionaryEntrySource::AutoLearned);
        assert_eq!(learned.replacement_of, Some("robin".to_string()));
        assert_eq!(settings.custom_words, vec!["Robyn"]);
    }

    #[test]
    fn upsert_auto_learn_entry_skips_duplicates() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![entry("dict_1_robyn", "Robyn")];

        let learned = upsert_auto_learn_entry(&mut settings, 42, "robyn".to_string(), None)
            .expect("auto learn");

        assert!(learned.is_none());
        assert_eq!(settings.dictionary_entries.len(), 1);
    }

    #[test]
    fn upsert_auto_learn_entry_rejects_dangling_hyphen_words() {
        let mut settings = get_default_settings();

        let learned = upsert_auto_learn_entry(&mut settings, 42, "Vow-".to_string(), None)
            .expect("auto learn should ignore dangling hyphen");

        assert!(learned.is_none());
        assert!(settings.dictionary_entries.is_empty());
        assert!(settings.custom_words.is_empty());
    }

    #[test]
    fn replace_dictionary_phrases_preserves_existing_matching_entries() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![
            DictionaryEntry {
                replacement_of: Some("robin".to_string()),
                source: DictionaryEntrySource::AutoLearned,
                ..entry("dict_1_robyn", "Robyn")
            },
            entry("dict_2_kulaib", "Abdullah al Kulaib"),
        ];

        replace_dictionary_phrases(
            &mut settings,
            42,
            vec!["Robyn".to_string(), "ChargeBee".to_string()],
        );

        assert_eq!(settings.dictionary_entries.len(), 2);
        assert_eq!(
            settings.dictionary_entries[0].replacement_of,
            Some("robin".to_string())
        );
        assert_eq!(
            settings.dictionary_entries[0].source,
            DictionaryEntrySource::AutoLearned
        );
        assert_eq!(settings.dictionary_entries[1].phrase, "ChargeBee");
        assert_eq!(settings.custom_words, vec!["Robyn", "ChargeBee"]);
    }

    fn entry(id: &str, phrase: &str) -> DictionaryEntry {
        DictionaryEntry {
            id: id.to_string(),
            phrase: phrase.to_string(),
            replacement_of: None,
            source: DictionaryEntrySource::Manual,
            priority: DictionaryEntryPriority::Normal,
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }
}
