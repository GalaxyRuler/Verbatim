use serde::{Deserialize, Serialize};
use specta::Type;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::AppSettings;

pub const MAX_SNIPPET_TRIGGER_CHARS: usize = 120;
pub const MAX_SNIPPET_CONTENT_CHARS: usize = 12_000;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct SnippetEntry {
    pub id: String,
    pub trigger: String,
    pub content: String,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub updated_at_ms: u64,
}

pub fn sanitize_snippet_trigger(raw: &str) -> Option<String> {
    let trigger = normalize_trigger(raw);
    if trigger.is_empty()
        || trigger.chars().count() > MAX_SNIPPET_TRIGGER_CHARS
        || !trigger.chars().any(char::is_alphanumeric)
        || trigger.chars().any(|ch| ch.is_control())
    {
        return None;
    }

    Some(trigger)
}

pub fn sanitize_snippet_content(raw: &str) -> Option<String> {
    let content = raw.trim().replace("\r\n", "\n").replace('\r', "\n");
    if content.is_empty()
        || content.chars().count() > MAX_SNIPPET_CONTENT_CHARS
        || content
            .chars()
            .any(|ch| ch.is_control() && ch != '\n' && ch != '\t')
    {
        return None;
    }

    Some(content)
}

pub fn make_snippet_entry_id(now_ms: u64, trigger: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;

    for ch in trigger.chars() {
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

    format!("snippet_{}_{}", now_ms, slug)
}

pub fn current_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn upsert_snippet_entry(
    settings: &mut AppSettings,
    now_ms: u64,
    trigger: String,
    content: String,
) -> Result<SnippetEntry, String> {
    let trigger = sanitize_snippet_trigger(&trigger).ok_or("Snippet trigger is empty")?;
    let content = sanitize_snippet_content(&content).ok_or("Snippet content is empty")?;

    if has_trigger(&settings.snippets, &trigger, None) {
        return Err(format!("{} is already a snippet trigger", trigger));
    }

    let entry = SnippetEntry {
        id: make_snippet_entry_id(now_ms, &trigger),
        trigger,
        content,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
    };

    settings.snippets.push(entry.clone());
    Ok(entry)
}

pub fn update_snippet_entry(
    settings: &mut AppSettings,
    now_ms: u64,
    id: &str,
    trigger: Option<String>,
    content: Option<String>,
) -> Result<SnippetEntry, String> {
    let index = settings
        .snippets
        .iter()
        .position(|entry| entry.id == id)
        .ok_or("Snippet not found")?;

    let next_trigger = if let Some(trigger) = trigger {
        Some(sanitize_snippet_trigger(&trigger).ok_or("Snippet trigger is empty")?)
    } else {
        None
    };

    if let Some(trigger) = &next_trigger {
        if has_trigger(&settings.snippets, trigger, Some(id)) {
            return Err(format!("{} is already a snippet trigger", trigger));
        }
    }

    let next_content = if let Some(content) = content {
        Some(sanitize_snippet_content(&content).ok_or("Snippet content is empty")?)
    } else {
        None
    };

    let entry = &mut settings.snippets[index];
    if let Some(trigger) = next_trigger {
        entry.trigger = trigger;
    }
    if let Some(content) = next_content {
        entry.content = content;
    }
    entry.updated_at_ms = now_ms;

    Ok(entry.clone())
}

pub fn delete_snippet_entries(settings: &mut AppSettings, ids: &[String]) -> Vec<SnippetEntry> {
    let mut deleted = Vec::new();
    settings.snippets.retain(|entry| {
        if ids.iter().any(|id| id == &entry.id) {
            deleted.push(entry.clone());
            false
        } else {
            true
        }
    });

    deleted
}

pub fn sync_snippets(settings: &mut AppSettings) -> bool {
    let original = settings.snippets.clone();
    let mut cleaned = Vec::new();

    for mut entry in original.iter().cloned() {
        let Some(trigger) = sanitize_snippet_trigger(&entry.trigger) else {
            continue;
        };
        let Some(content) = sanitize_snippet_content(&entry.content) else {
            continue;
        };

        if has_trigger(&cleaned, &trigger, None) {
            continue;
        }

        entry.trigger = trigger;
        entry.content = content;
        cleaned.push(entry);
    }

    let changed = settings.snippets != cleaned;
    settings.snippets = cleaned;
    changed
}

pub fn expand_snippets(text: &str, entries: &[SnippetEntry]) -> String {
    let mut candidates = entries
        .iter()
        .filter(|entry| !entry.trigger.trim().is_empty())
        .collect::<Vec<_>>();
    candidates.sort_by_key(|entry| std::cmp::Reverse(entry.trigger.chars().count()));

    let mut replacements = Vec::new();
    for entry in candidates {
        let trigger = normalize_trigger(&entry.trigger);
        for (start, end) in find_trigger_spans(text, &trigger) {
            if replacements.iter().all(|replacement: &SnippetReplacement| {
                end <= replacement.start || start >= replacement.end
            }) {
                replacements.push(SnippetReplacement {
                    start,
                    end,
                    content: entry.content.clone(),
                });
            }
        }
    }

    if replacements.is_empty() {
        return text.to_string();
    }

    replacements.sort_by_key(|replacement| replacement.start);

    let mut expanded = String::with_capacity(text.len());
    let mut cursor = 0;
    for replacement in replacements {
        expanded.push_str(&text[cursor..replacement.start]);
        expanded.push_str(&replacement.content);
        cursor = replacement.end;
    }
    expanded.push_str(&text[cursor..]);
    expanded
}

struct SnippetReplacement {
    start: usize,
    end: usize,
    content: String,
}

fn normalize_trigger(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_trigger_spans(text: &str, trigger: &str) -> Vec<(usize, usize)> {
    if trigger.is_empty() {
        return Vec::new();
    }

    text.char_indices()
        .filter_map(|(start, _)| {
            let end = start.checked_add(trigger.len())?;
            if end > text.len() || !text.is_char_boundary(end) {
                return None;
            }

            let candidate = &text[start..end];
            if trigger_matches(candidate, trigger) && has_trigger_boundaries(text, start, end) {
                Some((start, end))
            } else {
                None
            }
        })
        .collect()
}

fn trigger_matches(candidate: &str, trigger: &str) -> bool {
    candidate == trigger || candidate.eq_ignore_ascii_case(trigger)
}

fn has_trigger_boundaries(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start].chars().next_back();
    let after = text[end..].chars().next();

    before.is_none_or(|ch| !is_word_char(ch)) && after.is_none_or(|ch| !is_word_char(ch))
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '-'
}

fn normalize_trigger_key(raw: &str) -> String {
    sanitize_snippet_trigger(raw)
        .unwrap_or_default()
        .to_lowercase()
}

fn has_trigger(entries: &[SnippetEntry], trigger: &str, except_id: Option<&str>) -> bool {
    let key = normalize_trigger_key(trigger);
    entries.iter().any(|entry| {
        except_id.is_none_or(|id| entry.id != id) && normalize_trigger_key(&entry.trigger) == key
    })
}

#[cfg(test)]
mod tests {
    use super::{
        delete_snippet_entries, expand_snippets, sync_snippets, update_snippet_entry,
        upsert_snippet_entry, SnippetEntry,
    };
    use crate::settings::get_default_settings;

    #[test]
    fn expand_snippets_prefers_longest_exact_trigger() {
        let entries = vec![
            entry("short", "signature", "SHORT"),
            entry("long", "email signature", "LONG"),
        ];

        assert_eq!(expand_snippets("email signature", &entries), "LONG");
    }

    #[test]
    fn expand_snippets_replaces_standalone_trigger_inside_text() {
        let entries = vec![entry("email", "email signature", "Regards,\nAbdullah")];

        assert_eq!(
            expand_snippets("please use email signature today", &entries),
            "please use Regards,\nAbdullah today"
        );
    }

    #[test]
    fn expand_snippets_does_not_replace_inside_words() {
        let entries = vec![entry("sig", "sig", "SIGNATURE")];

        assert_eq!(
            expand_snippets("signature sig assign", &entries),
            "signature SIGNATURE assign"
        );
    }

    #[test]
    fn upsert_snippet_entry_adds_sanitized_entry() {
        let mut settings = get_default_settings();

        let snippet = upsert_snippet_entry(
            &mut settings,
            42,
            "  email   signature  ".to_string(),
            "Regards,\nAbdullah".to_string(),
        )
        .expect("snippet should be added");

        assert_eq!(snippet.trigger, "email signature");
        assert_eq!(snippet.content, "Regards,\nAbdullah");
        assert_eq!(snippet.id, "snippet_42_email-signature");
        assert_eq!(settings.snippets, vec![snippet]);
    }

    #[test]
    fn update_snippet_entry_updates_trigger_and_content() {
        let mut settings = get_default_settings();
        settings.snippets = vec![entry("snippet_1_signature", "signature", "Old")];

        let updated = update_snippet_entry(
            &mut settings,
            100,
            "snippet_1_signature",
            Some("email signature".to_string()),
            Some("New content".to_string()),
        )
        .expect("snippet should update");

        assert_eq!(updated.trigger, "email signature");
        assert_eq!(updated.content, "New content");
        assert_eq!(updated.updated_at_ms, 100);
        assert_eq!(settings.snippets, vec![updated]);
    }

    #[test]
    fn delete_snippet_entries_removes_requested_ids() {
        let mut settings = get_default_settings();
        settings.snippets = vec![
            entry("snippet_1_signature", "signature", "Signature"),
            entry("snippet_2_address", "address", "Address"),
        ];

        let deleted = delete_snippet_entries(&mut settings, &["snippet_1_signature".to_string()]);

        assert_eq!(
            deleted,
            vec![entry("snippet_1_signature", "signature", "Signature")]
        );
        assert_eq!(
            settings.snippets,
            vec![entry("snippet_2_address", "address", "Address")]
        );
    }

    #[test]
    fn sync_snippets_normalizes_and_deduplicates_entries() {
        let mut settings = get_default_settings();
        settings.snippets = vec![
            entry("bad", "  ", "Empty trigger"),
            entry("snippet_1_email", "  email   signature  ", "Signature"),
            entry("snippet_2_email", "Email Signature", "Duplicate"),
            entry("bad_content", "address", "  "),
        ];

        let changed = sync_snippets(&mut settings);

        assert!(changed);
        assert_eq!(
            settings.snippets,
            vec![entry("snippet_1_email", "email signature", "Signature")]
        );
    }

    fn entry(id: &str, trigger: &str, content: &str) -> SnippetEntry {
        SnippetEntry {
            id: id.to_string(),
            trigger: trigger.to_string(),
            content: content.to_string(),
            created_at_ms: 1,
            updated_at_ms: 1,
        }
    }
}
