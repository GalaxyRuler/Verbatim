use crate::dictionary_learning::canonicalize;
use crate::settings::{
    AppSettings, DictionaryEntry, DictionaryEntryPriority, DictionaryEntrySource, LearnCandidate,
};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub const AMBIGUOUS_ENTRY_ID: &str = "ambiguous_entry_id";

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

fn dictionary_entry_id_base(now_ms: u64, phrase: &str) -> String {
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

fn allocate_dictionary_entry_id<'a>(
    base: &str,
    existing_ids: impl IntoIterator<Item = &'a str>,
) -> String {
    let existing_ids = existing_ids.into_iter().collect::<HashSet<_>>();
    if !existing_ids.contains(base) {
        return base.to_string();
    }

    let mut suffix = 2u64;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !existing_ids.contains(candidate.as_str()) {
            return candidate;
        }
        suffix += 1;
    }
}

pub fn make_dictionary_entry_id(entries: &[DictionaryEntry], now_ms: u64, phrase: &str) -> String {
    let base = dictionary_entry_id_base(now_ms, phrase);
    allocate_dictionary_entry_id(&base, entries.iter().map(|entry| entry.id.as_str()))
}

/// Phrases of ACTIVE entries only. Feeds the ASR prompt context and the legacy
/// `custom_words` mirror — quarantined (inactive) entries must not keep biasing
/// transcription toward the very phrase the user reversed. Canonical duplicates
/// keep the first entry's display text.
pub fn dictionary_phrases(entries: &[DictionaryEntry]) -> Vec<String> {
    let mut seen = HashSet::new();
    entries
        .iter()
        .filter(|entry| entry.active)
        .filter_map(|entry| {
            seen.insert(canonicalize(&entry.phrase))
                .then(|| entry.phrase.clone())
        })
        .collect()
}

/// v0 -> v1: sanitize entry fields, then grandfather existing auto-learned entries to
/// manual-tier trust (`user_confirmed = true`) so their fuzzy behaviour does not silently
/// tighten under the new stricter auto tier. Idempotent via `dictionary_schema_version`.
pub fn migrate_dictionary_v1(settings: &mut AppSettings) -> bool {
    if settings.dictionary_schema_version >= 1 {
        return false;
    }
    for entry in &mut settings.dictionary_entries {
        // Sanitize before classifying.
        if let Some(phrase) = sanitize_dictionary_phrase(&entry.phrase) {
            entry.phrase = phrase;
        }
        entry.replacement_of = entry
            .replacement_of
            .as_deref()
            .and_then(sanitize_dictionary_phrase);
        if entry.source == DictionaryEntrySource::AutoLearned {
            entry.user_confirmed = true; // grandfather at manual-tier (base threshold)
        }
    }
    settings.dictionary_schema_version = 1;
    true
}

/// v1 -> v2: keep the first occurrence of every persisted entry ID and deterministically
/// suffix later duplicates. Array order and every non-ID field remain unchanged.
pub fn migrate_dictionary_v2(settings: &mut AppSettings) -> bool {
    if settings.dictionary_schema_version >= 2 {
        return false;
    }

    let mut reserved_ids = settings
        .dictionary_entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<HashSet<_>>();
    let mut seen_ids = HashSet::new();
    for entry in &mut settings.dictionary_entries {
        if seen_ids.insert(entry.id.clone()) {
            continue;
        }

        let repaired_id =
            allocate_dictionary_entry_id(&entry.id, reserved_ids.iter().map(String::as_str));
        entry.id = repaired_id.clone();
        seen_ids.insert(repaired_id.clone());
        reserved_ids.insert(repaired_id);
    }

    settings.dictionary_schema_version = 2;
    true
}

fn is_manual_trust_entry(entry: &DictionaryEntry) -> bool {
    entry.user_confirmed
        || matches!(
            entry.source,
            DictionaryEntrySource::Manual | DictionaryEntrySource::Imported
        )
}

fn record_discarded_replacement_rule(
    candidates: &mut Vec<LearnCandidate>,
    discarded: &DictionaryEntry,
    retained: &DictionaryEntry,
) {
    let Some(discarded_source) = discarded.replacement_of.as_deref() else {
        return;
    };
    if retained
        .replacement_of
        .as_deref()
        .is_some_and(|source| canonicalize(source) == canonicalize(discarded_source))
    {
        return;
    }

    let source_key = canonicalize(discarded_source);
    let phrase_key = canonicalize(&discarded.phrase);
    if candidates.iter().any(|candidate| {
        candidate
            .replacement_of
            .as_deref()
            .is_some_and(|source| canonicalize(source) == source_key)
            && canonicalize(&candidate.phrase) == phrase_key
    }) {
        return;
    }

    candidates.push(LearnCandidate {
        replacement_of: Some(discarded_source.to_string()),
        phrase: discarded.phrase.clone(),
        occurrences: 1,
        last_evidence_session: None,
        created_at_ms: discarded.created_at_ms,
        updated_at_ms: discarded.updated_at_ms,
    });
}

/// v2 -> v3: recompute dictionary identity using the NFC canonical key.
///
/// Canonically equal entries collapse to one. Manual-tier entries (Manual,
/// Imported, or user-confirmed) outrank unconfirmed auto-learned entries; ties
/// keep the first persisted entry. A discarded, distinct `replacement_of` is
/// retained as a learn candidate for review instead of being silently lost.
/// Display fields are copied from persisted values and are never normalized.
pub fn migrate_dictionary_v3(settings: &mut AppSettings) -> bool {
    if settings.dictionary_schema_version >= 3 {
        return false;
    }

    let mut groups: Vec<Vec<DictionaryEntry>> = Vec::new();
    let mut group_by_key = HashMap::new();
    for entry in std::mem::take(&mut settings.dictionary_entries) {
        let key = canonicalize(&entry.phrase);
        let group_index = *group_by_key.entry(key).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[group_index].push(entry);
    }

    let mut reconciled = Vec::with_capacity(groups.len());
    for group in groups {
        let mut winner_index = 0;
        for index in 1..group.len() {
            if is_manual_trust_entry(&group[index]) && !is_manual_trust_entry(&group[winner_index])
            {
                winner_index = index;
            }
        }

        let retained = group[winner_index].clone();
        for (index, discarded) in group.iter().enumerate() {
            if index != winner_index {
                record_discarded_replacement_rule(
                    &mut settings.dictionary_learn_candidates,
                    discarded,
                    &retained,
                );
            }
        }
        reconciled.push(retained);
    }
    settings.dictionary_entries = reconciled;

    sync_auto_learn_suppression_keys(settings);
    settings.custom_words = dictionary_phrases(&settings.dictionary_entries);
    settings.dictionary_schema_version = 3;
    true
}

pub fn sync_legacy_custom_words(settings: &mut AppSettings) -> bool {
    sync_legacy_custom_words_with_migration(settings, true)
}

pub fn sync_legacy_custom_words_with_migration(
    settings: &mut AppSettings,
    migrate_legacy_custom_words: bool,
) -> bool {
    let mut changed = false;

    changed |= sync_auto_learn_suppression_keys(settings);

    let before_len = settings.dictionary_entries.len();
    settings
        .dictionary_entries
        .retain(|entry| sanitize_dictionary_phrase(&entry.phrase).is_some());
    if settings.dictionary_entries.len() != before_len {
        changed = true;
    }

    if migrate_legacy_custom_words
        && settings.dictionary_entries.is_empty()
        && !settings.custom_words.is_empty()
    {
        let mut entries = Vec::new();
        for (index, word) in settings.custom_words.iter().enumerate() {
            let Some(phrase) = sanitize_dictionary_phrase(word) else {
                continue;
            };

            if has_phrase(&entries, &phrase, None) {
                continue;
            }

            let id = make_dictionary_entry_id(&entries, index as u64, &phrase);
            entries.push(DictionaryEntry {
                id,
                phrase,
                replacement_of: None,
                source: DictionaryEntrySource::Manual,
                priority: DictionaryEntryPriority::Normal,
                created_at_ms: 0,
                updated_at_ms: 0,
                active: true,
                user_confirmed: false,
                needs_review: false,
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

    unsuppress_auto_learn_phrase(settings, &phrase);

    let entry = DictionaryEntry {
        id: make_dictionary_entry_id(&settings.dictionary_entries, now_ms, &phrase),
        phrase,
        replacement_of: sanitize_optional_phrase(replacement_of),
        source: DictionaryEntrySource::Manual,
        priority: DictionaryEntryPriority::Normal,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        active: true,
        user_confirmed: false,
        needs_review: false,
    };

    settings.dictionary_entries.push(entry.clone());
    sync_legacy_custom_words(settings);

    Ok(entry)
}

/// No longer called from production code paths — `learn_custom_words_from_correction` now
/// routes through `observe_correction` (the provisional-candidate state machine) instead of
/// minting entries directly. Kept for its test coverage; candidate for removal in a future
/// cleanup pass.
#[cfg_attr(not(test), allow(dead_code))]
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
    if auto_learn_phrase_is_suppressed(settings, &phrase) {
        return Ok(None);
    }

    let entry = DictionaryEntry {
        id: make_dictionary_entry_id(&settings.dictionary_entries, now_ms, &phrase),
        phrase,
        replacement_of: sanitize_optional_phrase(replacement_of),
        source: DictionaryEntrySource::AutoLearned,
        priority: DictionaryEntryPriority::Normal,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        active: true,
        user_confirmed: false,
        needs_review: false,
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
    let mut matching_indices = settings
        .dictionary_entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (entry.id == id).then_some(index));
    let index = matching_indices
        .next()
        .ok_or("Dictionary entry not found")?;
    if matching_indices.next().is_some() {
        return Err(AMBIGUOUS_ENTRY_ID.to_string());
    }

    if let Some(next_phrase) = phrase.as_deref().and_then(sanitize_dictionary_phrase) {
        if has_phrase(&settings.dictionary_entries, &next_phrase, Some(id)) {
            return Err(format!("{} is already in your dictionary", next_phrase));
        }
    }

    let phrase_to_unsuppress;
    let updated = {
        let entry = &mut settings.dictionary_entries[index];

        if let Some(next_phrase) = phrase {
            entry.phrase =
                sanitize_dictionary_phrase(&next_phrase).ok_or("Dictionary entry is empty")?;
            phrase_to_unsuppress = Some(entry.phrase.clone());
        } else {
            phrase_to_unsuppress = None;
        }

        if let Some(next_replacement) = replacement_of {
            entry.replacement_of = sanitize_optional_phrase(next_replacement);
        }

        if let Some(next_priority) = priority {
            entry.priority = next_priority;
        }

        entry.updated_at_ms = now_ms;
        entry.clone()
    };

    if let Some(phrase) = phrase_to_unsuppress {
        unsuppress_auto_learn_phrase(settings, &phrase);
    }
    sync_legacy_custom_words(settings);

    Ok(updated)
}

pub fn delete_entries(
    settings: &mut AppSettings,
    ids: &[String],
) -> Result<Vec<DictionaryEntry>, String> {
    for id in ids {
        if settings
            .dictionary_entries
            .iter()
            .filter(|entry| entry.id == *id)
            .take(2)
            .count()
            > 1
        {
            return Err(AMBIGUOUS_ENTRY_ID.to_string());
        }
    }

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
        for entry in &deleted {
            suppress_auto_learn_phrase(settings, &entry.phrase);
        }
        settings.custom_words = dictionary_phrases(&settings.dictionary_entries);
        sync_legacy_custom_words(settings);
    }

    Ok(deleted)
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
        if !desired_phrases
            .iter()
            .any(|existing: &String| canonicalize(existing) == canonicalize(&phrase))
        {
            desired_phrases.push(phrase);
        }
    }

    settings.dictionary_entries.retain(|entry| {
        desired_phrases
            .iter()
            .any(|phrase| canonicalize(phrase) == canonicalize(&entry.phrase))
    });

    for phrase in desired_phrases {
        if has_phrase(&settings.dictionary_entries, &phrase, None) {
            unsuppress_auto_learn_phrase(settings, &phrase);
            continue;
        }

        unsuppress_auto_learn_phrase(settings, &phrase);
        let id = make_dictionary_entry_id(&settings.dictionary_entries, now_ms, &phrase);
        settings.dictionary_entries.push(DictionaryEntry {
            id,
            phrase,
            replacement_of: None,
            source: DictionaryEntrySource::Manual,
            priority: DictionaryEntryPriority::Normal,
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
            active: true,
            user_confirmed: false,
            needs_review: false,
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

fn sync_auto_learn_suppression_keys(settings: &mut AppSettings) -> bool {
    let mut changed = false;
    let mut normalized = Vec::new();

    for phrase in &settings.dictionary_auto_learn_suppressed {
        let key = canonicalize(phrase);
        if key.is_empty() {
            changed = true;
            continue;
        }

        if !normalized.contains(&key) {
            normalized.push(key);
        } else {
            changed = true;
        }
    }

    if settings.dictionary_auto_learn_suppressed != normalized {
        settings.dictionary_auto_learn_suppressed = normalized;
        changed = true;
    }

    changed
}

fn auto_learn_phrase_is_suppressed(settings: &AppSettings, phrase: &str) -> bool {
    let key = canonicalize(phrase);
    !key.is_empty()
        && settings
            .dictionary_auto_learn_suppressed
            .iter()
            .any(|suppressed| suppressed == &key)
}

fn suppress_auto_learn_phrase(settings: &mut AppSettings, phrase: &str) -> bool {
    let key = canonicalize(phrase);
    if key.is_empty() || settings.dictionary_auto_learn_suppressed.contains(&key) {
        return false;
    }

    settings.dictionary_auto_learn_suppressed.push(key);
    true
}

fn unsuppress_auto_learn_phrase(settings: &mut AppSettings, phrase: &str) -> bool {
    let key = canonicalize(phrase);
    if key.is_empty() {
        return false;
    }

    let before_len = settings.dictionary_auto_learn_suppressed.len();
    settings
        .dictionary_auto_learn_suppressed
        .retain(|suppressed| suppressed != &key);

    settings.dictionary_auto_learn_suppressed.len() != before_len
}

fn dictionary_phrase_has_valid_token_boundaries(phrase: &str) -> bool {
    phrase
        .split_whitespace()
        .all(|token| !token.starts_with('-') && !token.ends_with('-'))
}

fn has_phrase(entries: &[DictionaryEntry], phrase: &str, except_id: Option<&str>) -> bool {
    let key = canonicalize(phrase);
    entries.iter().any(|entry| {
        except_id.is_none_or(|id| entry.id != id) && canonicalize(&entry.phrase) == key
    })
}

#[derive(Debug, PartialEq, Eq)]
pub enum ObserveOutcome {
    Learned,    // new candidate
    Reinforced, // existing candidate, +1, not yet promoted (or blocked by conflict)
    Promoted,   // reached threshold -> became an active entry
    NoChange,   // suppressed / duplicate session / already active
    Routed,     // produced-output feedback: edit of dictionary-produced text
}

pub const PROMOTE_THRESHOLD: u32 = 2;

/// If `replacement_of` (the post-apply dictated form) matches an ACTIVE entry's phrase, the
/// user is editing text the dictionary produced. Reversal -> quarantine auto entry;
/// refinement -> flag for review. Returns Some(Routed) when handled (caller must NOT learn
/// a standalone rule).
pub fn produced_output_feedback(
    settings: &mut AppSettings,
    now_ms: u64,
    replacement_of: &Option<String>,
    corrected_phrase: &str,
) -> Option<ObserveOutcome> {
    let dictated = replacement_of.as_deref()?;
    let dictated_key = canonicalize(dictated);
    let corrected_key = canonicalize(corrected_phrase);

    let index = settings
        .dictionary_entries
        .iter()
        .position(|e| e.active && canonicalize(&e.phrase) == dictated_key)?;

    let is_reversal = settings.dictionary_entries[index]
        .replacement_of
        .as_deref()
        .map(canonicalize)
        .as_deref()
        == Some(corrected_key.as_str());

    let entry = &mut settings.dictionary_entries[index];
    entry.needs_review = true;
    entry.updated_at_ms = now_ms;
    let is_auto = entry.source == DictionaryEntrySource::AutoLearned && !entry.user_confirmed;
    if is_reversal && is_auto {
        entry.active = false; // quarantine from apply
        sync_legacy_custom_words(settings);
    }
    Some(ObserveOutcome::Routed)
}

fn candidate_pair(replacement_of: &Option<String>, phrase: &str) -> (Option<String>, String) {
    (
        replacement_of.as_deref().map(canonicalize),
        canonicalize(phrase),
    )
}

fn source_is_conflicted(
    settings: &AppSettings,
    canon_src: &Option<String>,
    canon_phrase: &str,
) -> bool {
    let Some(src) = canon_src else { return false };
    settings.dictionary_learn_candidates.iter().any(|c| {
        c.replacement_of.as_deref().map(canonicalize).as_deref() == Some(src.as_str())
            && canonicalize(&c.phrase) != canon_phrase
    })
}

/// Max inert auto-learn candidates kept in the review queue.
pub const MAX_LEARN_CANDIDATES: usize = 50;
/// Candidates untouched longer than this are expired (30 days).
pub const MAX_CANDIDATE_AGE_MS: u64 = 30 * 24 * 60 * 60 * 1000;

/// Bound the inert candidate store: drop candidates untouched past the age limit, then, if
/// still over the size cap, evict the oldest (least-recently-updated). Returns count removed.
pub fn prune_learn_candidates(settings: &mut AppSettings, now_ms: u64) -> usize {
    let before = settings.dictionary_learn_candidates.len();
    settings
        .dictionary_learn_candidates
        .retain(|c| now_ms.saturating_sub(c.updated_at_ms) <= MAX_CANDIDATE_AGE_MS);
    if settings.dictionary_learn_candidates.len() > MAX_LEARN_CANDIDATES {
        // newest first, then keep the cap
        settings
            .dictionary_learn_candidates
            .sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        settings
            .dictionary_learn_candidates
            .truncate(MAX_LEARN_CANDIDATES);
    }
    before - settings.dictionary_learn_candidates.len()
}

pub fn observe_correction(
    settings: &mut AppSettings,
    now_ms: u64,
    session: &str,
    dictated: &str,
    corrected: Option<&str>,
) -> ObserveOutcome {
    let Some(phrase) = corrected.and_then(sanitize_dictionary_phrase) else {
        return ObserveOutcome::NoChange;
    };
    let replacement_of = sanitize_dictionary_phrase(dictated);

    // Feedback is intentionally checked BEFORE suppression: it concerns an existing
    // ACTIVE entry (the dictated form is dictionary output), not learning the corrected
    // phrase — a phrase tombstone must not mask reversal/refinement signals.
    if let Some(outcome) = produced_output_feedback(settings, now_ms, &replacement_of, &phrase) {
        return outcome;
    }

    if auto_learn_phrase_is_suppressed(settings, &phrase) {
        return ObserveOutcome::NoChange;
    }
    if has_phrase(&settings.dictionary_entries, &phrase, None) {
        return ObserveOutcome::NoChange;
    }

    // Bound the candidate store on every learn pass (part of this locked mutation); a pair
    // re-corrected after the age limit simply starts fresh.
    prune_learn_candidates(settings, now_ms);

    let (canon_src, canon_phrase) = candidate_pair(&replacement_of, &phrase);
    if let Some(index) = settings.dictionary_learn_candidates.iter().position(|c| {
        c.replacement_of.as_deref().map(canonicalize) == canon_src
            && canonicalize(&c.phrase) == canon_phrase
    }) {
        {
            let c = &mut settings.dictionary_learn_candidates[index];
            if c.last_evidence_session.as_deref() == Some(session) {
                return ObserveOutcome::NoChange;
            }
            c.occurrences += 1;
            c.updated_at_ms = now_ms;
            c.last_evidence_session = Some(session.to_string());
        }
        let promote = settings.dictionary_learn_candidates[index].occurrences >= PROMOTE_THRESHOLD
            && !source_is_conflicted(settings, &canon_src, &canon_phrase);
        if promote {
            let candidate = settings.dictionary_learn_candidates.remove(index);
            promote_candidate_to_entry(settings, now_ms, candidate, false);
            return ObserveOutcome::Promoted;
        }
        return ObserveOutcome::Reinforced;
    }

    settings.dictionary_learn_candidates.push(LearnCandidate {
        replacement_of,
        phrase,
        occurrences: 1,
        last_evidence_session: Some(session.to_string()),
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
    });
    ObserveOutcome::Learned
}

/// Convert a candidate into an active auto-learned entry. `user_confirmed` is true when
/// promotion came from an explicit Approve (manual-tier trust); false for recurrence.
pub fn promote_candidate_to_entry(
    settings: &mut AppSettings,
    now_ms: u64,
    mut candidate: LearnCandidate,
    user_confirmed: bool,
) {
    let Some(phrase) = sanitize_dictionary_phrase(&candidate.phrase) else {
        return;
    };
    let candidate_rule = candidate
        .replacement_of
        .as_deref()
        .and_then(sanitize_dictionary_phrase);
    if let Some(existing_index) = settings
        .dictionary_entries
        .iter()
        .position(|entry| entry.active && canonicalize(&entry.phrase) == canonicalize(&phrase))
    {
        let existing_rule = settings.dictionary_entries[existing_index]
            .replacement_of
            .as_deref()
            .and_then(sanitize_dictionary_phrase);
        let conflicting_rules = existing_rule
            .as_deref()
            .zip(candidate_rule.as_deref())
            .is_some_and(|(existing, recovered)| canonicalize(existing) != canonicalize(recovered));

        if conflicting_rules {
            candidate.phrase = phrase;
            candidate.replacement_of = candidate_rule;
            let candidate_phrase_key = canonicalize(&candidate.phrase);
            let candidate_rule_key = candidate.replacement_of.as_deref().map(canonicalize);
            if !settings.dictionary_learn_candidates.iter().any(|pending| {
                canonicalize(&pending.phrase) == candidate_phrase_key
                    && pending.replacement_of.as_deref().map(canonicalize) == candidate_rule_key
            }) {
                settings.dictionary_learn_candidates.push(candidate);
            }
            return;
        }

        let existing = &mut settings.dictionary_entries[existing_index];
        if existing.replacement_of.is_none() {
            existing.replacement_of = candidate_rule;
        }
        if user_confirmed {
            existing.user_confirmed = true;
        }
        existing.updated_at_ms = now_ms;
        return;
    }
    unsuppress_auto_learn_phrase(settings, &phrase);
    let id = make_dictionary_entry_id(&settings.dictionary_entries, now_ms, &phrase);
    settings.dictionary_entries.push(DictionaryEntry {
        id,
        phrase,
        replacement_of: candidate_rule,
        source: DictionaryEntrySource::AutoLearned,
        priority: DictionaryEntryPriority::Normal,
        created_at_ms: now_ms,
        updated_at_ms: now_ms,
        active: true,
        user_confirmed,
        needs_review: false,
    });
    sync_legacy_custom_words(settings);
}

/// Approve a pending candidate: promote it with user_confirmed (manual-tier) trust.
/// Matches by canonical pair; None replacement_of matches only candidates with None source.
/// Returns the promoted entry (the one now owning the phrase), or None if no candidate matched.
pub fn approve_candidate(
    settings: &mut AppSettings,
    now_ms: u64,
    phrase: &str,
    replacement_of: Option<&str>,
) -> Option<DictionaryEntry> {
    let phrase_key = canonicalize(phrase);
    let source_key = replacement_of.map(canonicalize);
    let index = settings.dictionary_learn_candidates.iter().position(|c| {
        canonicalize(&c.phrase) == phrase_key
            && c.replacement_of.as_deref().map(canonicalize) == source_key
    })?;
    let candidate = settings.dictionary_learn_candidates.remove(index);
    promote_candidate_to_entry(settings, now_ms, candidate, true);
    settings
        .dictionary_entries
        .iter()
        .find(|entry| canonicalize(&entry.phrase) == canonicalize(phrase))
        .cloned()
}

/// Reject a pending candidate: drop ALL candidates with this phrase (any source variant)
/// and suppress the phrase so it is not re-learned (explicit user rejection).
pub fn reject_candidate(settings: &mut AppSettings, phrase: &str) {
    let phrase_key = canonicalize(phrase);
    settings
        .dictionary_learn_candidates
        .retain(|c| canonicalize(&c.phrase) != phrase_key);
    suppress_auto_learn_phrase(settings, phrase);
}

/// Reactivate / quarantine an entry from the review UI; clears the review flag either way
/// (the user has looked at it).
pub fn set_entry_active(
    settings: &mut AppSettings,
    now_ms: u64,
    id: &str,
    active: bool,
) -> Result<bool, String> {
    let mut matching_indices = settings
        .dictionary_entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| (entry.id == id).then_some(index));
    let Some(index) = matching_indices.next() else {
        return Ok(false);
    };
    if matching_indices.next().is_some() {
        return Err(AMBIGUOUS_ENTRY_ID.to_string());
    }

    let entry = &mut settings.dictionary_entries[index];
    entry.active = active;
    entry.needs_review = false;
    entry.updated_at_ms = now_ms;
    sync_legacy_custom_words(settings);
    Ok(true)
}

fn stamp_since(diag: &mut crate::settings::DictionaryDiagnostics, now_ms: u64) {
    if diag.since_ms == 0 {
        diag.since_ms = now_ms;
    }
}

/// Accumulate learn-pass outcome counts (phrase-free) into the persisted diagnostics.
pub fn record_learn_outcomes(
    settings: &mut AppSettings,
    now_ms: u64,
    learned: u32,
    promoted: u32,
    reinforced: u32,
    routed: u32,
) {
    let diag = &mut settings.dictionary_diagnostics;
    if learned + promoted + reinforced + routed > 0 {
        stamp_since(diag, now_ms);
    }
    diag.learned += learned;
    diag.promoted += promoted;
    diag.reinforced += reinforced;
    diag.routed += routed;
}

/// Map a phrase-free `skip: <reason>` token to its counter. Unknown reasons are ignored.
pub fn record_skip(settings: &mut AppSettings, now_ms: u64, reason: &str) {
    let key = reason.trim_start_matches("skip:").trim();
    let diag = &mut settings.dictionary_diagnostics;
    let field = match key {
        "secure_field" => &mut diag.skip_secure_field,
        "secure_check_error" => &mut diag.skip_secure_check_error,
        "read_cap_exceeded" => &mut diag.skip_read_cap_exceeded,
        "target_changed" => &mut diag.skip_target_changed,
        "no_post_paste_change" => &mut diag.skip_no_post_paste_change,
        "runtime_id_missing" | "runtime_id_changed" => &mut diag.skip_runtime_id,
        _ => return,
    };
    *field += 1;
    stamp_since(diag, now_ms);
}

/// Zero all counters and start a fresh counting window.
pub fn reset_dictionary_diagnostics(settings: &mut AppSettings, now_ms: u64) {
    settings.dictionary_diagnostics = crate::settings::DictionaryDiagnostics {
        since_ms: now_ms,
        ..Default::default()
    };
}

#[cfg(test)]
mod tests {
    use super::{
        approve_candidate, delete_entries, dictionary_phrases, make_dictionary_entry_id,
        migrate_dictionary_v1, migrate_dictionary_v2, migrate_dictionary_v3, observe_correction,
        promote_candidate_to_entry, prune_learn_candidates, record_learn_outcomes,
        reject_candidate, replace_dictionary_phrases, sanitize_dictionary_phrase, set_entry_active,
        sync_legacy_custom_words, update_entry, upsert_auto_learn_entry, upsert_manual_entry,
        ObserveOutcome, MAX_CANDIDATE_AGE_MS, MAX_LEARN_CANDIDATES,
    };
    use crate::settings::{
        get_default_settings, DictionaryEntry, DictionaryEntryPriority, DictionaryEntrySource,
        LearnCandidate,
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
    fn sanitize_dictionary_phrase_allows_internal_technical_punctuation() {
        assert_eq!(
            sanitize_dictionary_phrase("Node.js C++ F#"),
            Some("Node.js C++ F#".to_string())
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
            make_dictionary_entry_id(&[], 42, "Abdullah al Kulaib"),
            "dict_42_abdullah-al-kulaib"
        );
    }

    #[test]
    fn same_millisecond_arabic_batch_receives_distinct_ids() {
        let mut settings = get_default_settings();

        replace_dictionary_phrases(
            &mut settings,
            42,
            vec![
                "عبدالله".to_string(),
                "العربية".to_string(),
                "القاموس".to_string(),
            ],
        );

        let ids = settings
            .dictionary_entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec!["dict_42_entry", "dict_42_entry-2", "dict_42_entry-3"]
        );
    }

    #[test]
    fn manual_entries_deduplicate_nfc_equivalent_arabic_and_latin_without_changing_display() {
        let mut settings = get_default_settings();
        let decomposed_arabic = "\u{627}\u{654}\u{62d}\u{645}\u{62f}";
        let decomposed_latin = "Cafe\u{301}";

        let arabic = upsert_manual_entry(&mut settings, 10, decomposed_arabic.to_string(), None)
            .expect("insert decomposed Arabic display");
        let latin = upsert_manual_entry(&mut settings, 11, decomposed_latin.to_string(), None)
            .expect("insert decomposed Latin display");

        assert!(upsert_manual_entry(
            &mut settings,
            12,
            "\u{623}\u{62d}\u{645}\u{62f}".into(),
            None,
        )
        .is_err());
        assert!(upsert_manual_entry(&mut settings, 13, "Caf\u{e9}".into(), None).is_err());
        assert_eq!(settings.dictionary_entries.len(), 2);
        assert_eq!(arabic.phrase, decomposed_arabic);
        assert_eq!(latin.phrase, decomposed_latin);
        assert_eq!(
            settings.custom_words,
            vec![decomposed_arabic.to_string(), decomposed_latin.to_string()]
        );
    }

    #[test]
    fn dictionary_phrases_returns_entry_phrases() {
        let entries = vec![entry("dict_1_robyn", "Robyn")];

        assert_eq!(dictionary_phrases(&entries), vec!["Robyn"]);
    }

    #[test]
    fn dictionary_phrases_exclude_quarantined_entries() {
        // Quarantined entries must not bias ASR prompts or the custom_words mirror.
        let entries = vec![
            entry("dict_1_robyn", "Robyn"),
            DictionaryEntry {
                active: false,
                ..entry("dict_2_their", "their")
            },
        ];

        assert_eq!(dictionary_phrases(&entries), vec!["Robyn"]);
    }

    #[test]
    fn quarantine_removes_phrase_from_custom_words_mirror() {
        let mut settings = get_default_settings();
        settings.dictionary_entries.push(DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            ..entry_full("dict_1", "their", Some("there"))
        });
        sync_legacy_custom_words(&mut settings);
        assert_eq!(settings.custom_words, vec!["their"]);

        // Reversal quarantines the entry; the mirror must drop the phrase.
        let out = observe_correction(&mut settings, 5, "s1", "their", Some("there"));
        assert_eq!(out, ObserveOutcome::Routed);
        assert!(settings.custom_words.is_empty());
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
    fn sync_legacy_custom_words_removes_dangling_hyphen_manual_and_imported_entries() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![
            entry("dict_1_vow", "Vow-"),
            DictionaryEntry {
                source: DictionaryEntrySource::Imported,
                ..entry("dict_2_al", "Al-")
            },
            entry("dict_3_jean_luc", "Jean-Luc"),
        ];
        settings.custom_words = vec![
            "Vow-".to_string(),
            "Al-".to_string(),
            "Jean-Luc".to_string(),
        ];

        let changed = sync_legacy_custom_words(&mut settings);

        assert!(changed);
        assert_eq!(settings.dictionary_entries.len(), 1);
        assert_eq!(settings.dictionary_entries[0].phrase, "Jean-Luc");
        assert_eq!(settings.custom_words, vec!["Jean-Luc"]);
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

        let deleted = delete_entries(&mut settings, &["dict_1_robyn".to_string()])
            .expect("unique entry deleted");

        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].phrase, "Robyn");
        assert_eq!(settings.custom_words, vec!["Abdullah al Kulaib"]);
    }

    #[test]
    fn update_entry_rejects_ambiguous_id_without_mutating_entries() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![
            entry("dict_collision", "Robyn"),
            entry("dict_collision", "Robinette"),
        ];
        sync_legacy_custom_words(&mut settings);
        let original_entries = settings.dictionary_entries.clone();
        let original_words = settings.custom_words.clone();

        let error = update_entry(
            &mut settings,
            100,
            "dict_collision",
            Some("Changed".to_string()),
            None,
            None,
        )
        .expect_err("duplicate persisted IDs must be rejected");

        assert_eq!(error, "ambiguous_entry_id");
        assert_eq!(settings.dictionary_entries, original_entries);
        assert_eq!(settings.custom_words, original_words);
    }

    #[test]
    fn delete_entries_rejects_ambiguous_id_without_mutating_entries() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![
            entry("dict_collision", "Robyn"),
            entry("dict_collision", "Robinette"),
        ];
        sync_legacy_custom_words(&mut settings);
        let original_entries = settings.dictionary_entries.clone();
        let original_words = settings.custom_words.clone();

        let error = delete_entries(&mut settings, &["dict_collision".to_string()])
            .expect_err("duplicate persisted IDs must be rejected");

        assert_eq!(error, "ambiguous_entry_id");
        assert_eq!(settings.dictionary_entries, original_entries);
        assert_eq!(settings.custom_words, original_words);
    }

    #[test]
    fn set_entry_active_rejects_ambiguous_id_without_mutating_entries() {
        let mut settings = get_default_settings();
        settings.dictionary_entries = vec![
            entry("dict_collision", "Robyn"),
            entry("dict_collision", "Robinette"),
        ];
        let original_entries = settings.dictionary_entries.clone();

        let error = set_entry_active(&mut settings, 100, "dict_collision", false)
            .expect_err("duplicate persisted IDs must be rejected");

        assert_eq!(error, "ambiguous_entry_id");
        assert_eq!(settings.dictionary_entries, original_entries);
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
    fn deleted_entry_is_not_auto_learned_again() {
        let mut settings = get_default_settings();
        let learned = upsert_auto_learn_entry(
            &mut settings,
            42,
            "Gibbeteen".to_string(),
            Some("gibberish".to_string()),
        )
        .expect("auto learn")
        .expect("new entry");

        let deleted = delete_entries(&mut settings, &[learned.id]).expect("unique entry deleted");
        assert_eq!(deleted.len(), 1);
        assert!(settings.dictionary_entries.is_empty());
        assert_eq!(settings.dictionary_auto_learn_suppressed, vec!["gibbeteen"]);

        let learned_again = upsert_auto_learn_entry(
            &mut settings,
            43,
            "Gibbeteen".to_string(),
            Some("gibberish".to_string()),
        )
        .expect("auto learn");

        assert!(learned_again.is_none());
        assert!(settings.dictionary_entries.is_empty());
        assert!(settings.custom_words.is_empty());
    }

    #[test]
    fn manual_entry_clears_deleted_auto_learn_suppression() {
        let mut settings = get_default_settings();
        settings
            .dictionary_auto_learn_suppressed
            .push("gibbeteen".to_string());

        let manual = upsert_manual_entry(&mut settings, 42, "Gibbeteen".to_string(), None)
            .expect("manual add should override previous suppression");

        assert_eq!(manual.phrase, "Gibbeteen");
        assert!(settings.dictionary_auto_learn_suppressed.is_empty());
        assert_eq!(settings.custom_words, vec!["Gibbeteen"]);
    }

    #[test]
    fn replaced_dictionary_phrases_clear_matching_auto_learn_suppression() {
        let mut settings = get_default_settings();
        settings
            .dictionary_auto_learn_suppressed
            .extend(["robyn".to_string(), "gibbeteen".to_string()]);

        replace_dictionary_phrases(
            &mut settings,
            42,
            vec!["Robyn".to_string(), "Abdullah al Kulaib".to_string()],
        );

        assert_eq!(settings.dictionary_auto_learn_suppressed, vec!["gibbeteen"]);
        assert_eq!(
            settings.custom_words,
            vec!["Robyn".to_string(), "Abdullah al Kulaib".to_string()]
        );
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
    fn upsert_auto_learn_entry_accepts_non_cased_script_words() {
        let mut settings = get_default_settings();

        let learned = upsert_auto_learn_entry(
            &mut settings,
            42,
            "عبدالله".to_string(),
            Some("عبدالة".to_string()),
        )
        .expect("auto learn")
        .expect("new entry");

        assert_eq!(learned.phrase, "عبدالله");
        assert_eq!(learned.replacement_of, Some("عبدالة".to_string()));
        assert_eq!(settings.custom_words, vec!["عبدالله"]);
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

    #[test]
    fn first_correction_learns_provisional_candidate() {
        let mut s = get_default_settings();
        let out = observe_correction(&mut s, 10, "s1", "robin", Some("Robyn"));
        assert_eq!(out, ObserveOutcome::Learned);
        assert_eq!(s.dictionary_learn_candidates.len(), 1);
        assert_eq!(s.dictionary_learn_candidates[0].phrase, "Robyn");
        assert_eq!(s.dictionary_learn_candidates[0].occurrences, 1);
        assert!(s.dictionary_entries.is_empty()); // inert, not applied
    }

    #[test]
    fn same_pair_twice_across_sessions_promotes_to_entry() {
        let mut s = get_default_settings();
        assert_eq!(
            observe_correction(&mut s, 10, "s1", "robin", Some("Robyn")),
            ObserveOutcome::Learned
        );
        assert_eq!(
            observe_correction(&mut s, 20, "s2", "robin", Some("Robyn")),
            ObserveOutcome::Promoted
        );
        assert!(s.dictionary_learn_candidates.is_empty());
        assert_eq!(s.dictionary_entries.len(), 1);
        let e = &s.dictionary_entries[0];
        assert_eq!(e.phrase, "Robyn");
        assert_eq!(e.source, DictionaryEntrySource::AutoLearned);
        assert!(e.active);
        assert!(!e.user_confirmed);
        assert_eq!(e.replacement_of, Some("robin".to_string()));
        // legacy mirror updated on promotion
        assert_eq!(s.custom_words, vec!["Robyn".to_string()]);
    }

    #[test]
    fn none_source_corrections_share_one_candidate_and_promote() {
        // Documents intended behavior: a candidate with no usable source keys as
        // (None, phrase); repeated None-source corrections are the SAME candidate
        // (never "conflicting variants") and promote at the threshold.
        let mut s = get_default_settings();
        let out1 = observe_correction(&mut s, 10, "s1", "<<>>", Some("Robyn"));
        assert_eq!(out1, ObserveOutcome::Learned);
        assert_eq!(s.dictionary_learn_candidates.len(), 1);
        assert_eq!(s.dictionary_learn_candidates[0].replacement_of, None);

        let out2 = observe_correction(&mut s, 20, "s2", "<<>>", Some("Robyn"));
        assert_eq!(out2, ObserveOutcome::Promoted);
        assert_eq!(s.dictionary_entries.len(), 1);
        assert_eq!(s.dictionary_entries[0].replacement_of, None);
    }

    #[test]
    fn same_session_does_not_double_count() {
        let mut s = get_default_settings();
        observe_correction(&mut s, 10, "s1", "robin", Some("Robyn"));
        let out = observe_correction(&mut s, 11, "s1", "robin", Some("Robyn")); // same session
        assert_eq!(out, ObserveOutcome::NoChange);
        assert_eq!(s.dictionary_learn_candidates[0].occurrences, 1);
    }

    #[test]
    fn conflicting_targets_for_same_source_block_promotion() {
        let mut s = get_default_settings();
        observe_correction(&mut s, 10, "s1", "jon", Some("John"));
        observe_correction(&mut s, 20, "s2", "jon", Some("Jon")); // conflict on source "jon"
        let out = observe_correction(&mut s, 30, "s3", "jon", Some("John"));
        assert_eq!(out, ObserveOutcome::Reinforced); // occurrences reached 2 but conflicted -> no promotion
        assert!(s.dictionary_entries.is_empty());
    }

    #[test]
    fn suppressed_phrase_is_not_learned() {
        let mut s = get_default_settings();
        s.dictionary_auto_learn_suppressed.push("robyn".to_string());
        let out = observe_correction(&mut s, 10, "s1", "robin", Some("Robyn"));
        assert_eq!(out, ObserveOutcome::NoChange);
        assert!(s.dictionary_learn_candidates.is_empty());
    }

    #[test]
    fn existing_active_phrase_is_not_relearned() {
        let mut s = get_default_settings();
        s.dictionary_entries = vec![entry("dict_1_robyn", "Robyn")];
        let out = observe_correction(&mut s, 10, "s1", "robin", Some("Robyn"));
        assert_eq!(out, ObserveOutcome::NoChange);
        assert!(s.dictionary_learn_candidates.is_empty());
    }

    #[test]
    fn promotion_creates_auto_entry_and_merges_existing_phrase() {
        let mut s = get_default_settings();
        let cand = LearnCandidate {
            replacement_of: Some("robin".into()),
            phrase: "Robyn".into(),
            occurrences: 2,
            last_evidence_session: Some("s2".into()),
            created_at_ms: 1,
            updated_at_ms: 2,
        };
        promote_candidate_to_entry(&mut s, 42, cand, false);
        assert_eq!(s.dictionary_entries.len(), 1);
        let e = &s.dictionary_entries[0];
        assert_eq!(e.phrase, "Robyn");
        assert_eq!(e.replacement_of, Some("robin".into()));
        assert!(!e.user_confirmed);

        // Promoting a second candidate whose phrase already exists must not duplicate or clobber.
        let dup = LearnCandidate {
            replacement_of: Some("robbin".into()),
            phrase: "robyn".into(),
            occurrences: 2,
            last_evidence_session: None,
            created_at_ms: 3,
            updated_at_ms: 3,
        };
        promote_candidate_to_entry(&mut s, 43, dup, true);
        assert_eq!(s.dictionary_entries.len(), 1); // merged by phrase key
        assert_eq!(s.dictionary_entries[0].replacement_of, Some("robin".into()));
        // kept
    }

    #[test]
    fn approve_promotion_sets_user_confirmed() {
        let mut s = get_default_settings();
        let cand = LearnCandidate {
            replacement_of: Some("robin".into()),
            phrase: "Robyn".into(),
            occurrences: 1,
            last_evidence_session: None,
            created_at_ms: 1,
            updated_at_ms: 1,
        };
        promote_candidate_to_entry(&mut s, 42, cand, true);
        assert!(s.dictionary_entries[0].user_confirmed);
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
            active: true,
            user_confirmed: false,
            needs_review: false,
        }
    }

    fn entry_full(id: &str, phrase: &str, replacement_of: Option<&str>) -> DictionaryEntry {
        DictionaryEntry {
            id: id.into(),
            phrase: phrase.into(),
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
    fn reversal_of_auto_entry_quarantines_it_and_learns_nothing() {
        let mut s = get_default_settings();
        s.dictionary_entries.push(DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            ..entry_full("dict_1", "their", Some("there"))
        });
        // User edits produced "their" back to "there": dictated="their", corrected="there".
        let out = observe_correction(&mut s, 5, "s1", "their", Some("there"));
        assert_eq!(out, ObserveOutcome::Routed);
        assert!(!s.dictionary_entries[0].active); // quarantined
        assert!(s.dictionary_entries[0].needs_review);
        assert!(s.dictionary_learn_candidates.is_empty()); // no inverse learned
    }

    #[test]
    fn refinement_of_auto_entry_flags_review_without_new_rule() {
        let mut s = get_default_settings();
        s.dictionary_entries.push(DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            ..entry_full("dict_1", "ACME Corp", Some("acme"))
        });
        // User edits produced "ACME Corp" into "ACME Corporation".
        let out = observe_correction(&mut s, 5, "s1", "ACME Corp", Some("ACME Corporation"));
        assert_eq!(out, ObserveOutcome::Routed);
        assert!(s.dictionary_entries[0].needs_review);
        assert!(s.dictionary_entries[0].active); // refinement does not quarantine
        assert!(s.dictionary_learn_candidates.is_empty());
    }

    #[test]
    fn manual_entry_reversal_flags_but_stays_active() {
        let mut s = get_default_settings();
        s.dictionary_entries
            .push(entry_full("dict_1", "their", Some("there"))); // Manual
        let out = observe_correction(&mut s, 5, "s1", "their", Some("there"));
        assert_eq!(out, ObserveOutcome::Routed);
        assert!(s.dictionary_entries[0].active); // manual never auto-deactivated
        assert!(s.dictionary_entries[0].needs_review);
    }

    #[test]
    fn user_confirmed_auto_entry_reversal_flags_but_stays_active() {
        let mut s = get_default_settings();
        s.dictionary_entries.push(DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            user_confirmed: true,
            ..entry_full("dict_1", "their", Some("there"))
        });
        let out = observe_correction(&mut s, 5, "s1", "their", Some("there"));
        assert_eq!(out, ObserveOutcome::Routed);
        assert!(s.dictionary_entries[0].active); // user-confirmed = manual-tier trust
        assert!(s.dictionary_entries[0].needs_review);
    }

    #[test]
    fn inactive_entry_phrase_does_not_route_feedback() {
        let mut s = get_default_settings();
        s.dictionary_entries.push(DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            active: false,
            ..entry_full("dict_1", "their", Some("there"))
        });
        // Entry is quarantined; editing "their" is NOT feedback on it (not produced by apply).
        let out = observe_correction(&mut s, 5, "s1", "their", Some("there"));
        assert_ne!(out, ObserveOutcome::Routed);
    }

    #[test]
    fn unrelated_correction_does_not_route_feedback() {
        let mut s = get_default_settings();
        s.dictionary_entries.push(DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            ..entry_full("dict_1", "their", Some("there"))
        });
        // "robin" doesn't match any entry phrase -> normal learn path.
        let out = observe_correction(&mut s, 5, "s1", "robin", Some("Robyn"));
        assert_eq!(out, ObserveOutcome::Learned);
    }

    #[test]
    fn migration_grandfathers_auto_entries_at_manual_tier_and_is_idempotent() {
        let mut s = get_default_settings();
        s.dictionary_schema_version = 0;
        s.dictionary_entries = vec![
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry_full("dict_1", "Robyn", Some("robin"))
            },
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry_full("dict_2", "GraphQL", None)
            },
        ];

        let changed = migrate_dictionary_v1(&mut s);
        assert!(changed);
        assert_eq!(s.dictionary_schema_version, 1);
        assert!(
            s.dictionary_entries[0].user_confirmed,
            "grandfathered auto -> manual-tier"
        );
        assert!(s.dictionary_entries[1].user_confirmed);
        assert!(s.dictionary_entries.iter().all(|e| e.active));

        // Idempotent: second run is a no-op.
        let changed_again = migrate_dictionary_v1(&mut s);
        assert!(!changed_again);
    }

    #[test]
    fn migration_sanitizes_before_classifying() {
        let mut s = get_default_settings();
        s.dictionary_schema_version = 0;
        s.dictionary_entries = vec![DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            // replacement_of sanitizes to None (no alphabetic content)
            ..entry_full("dict_1", "Robyn", Some("<<>>"))
        }];

        migrate_dictionary_v1(&mut s);
        assert_eq!(s.dictionary_entries[0].replacement_of, None); // sanitized away, not kept raw
        assert!(s.dictionary_entries[0].user_confirmed);
    }

    #[test]
    fn migration_leaves_manual_entries_untouched_except_version() {
        let mut s = get_default_settings();
        s.dictionary_schema_version = 0;
        s.dictionary_entries = vec![entry_full("dict_1", "Jean-Luc", None)]; // Manual source
        migrate_dictionary_v1(&mut s);
        assert!(!s.dictionary_entries[0].user_confirmed); // manual entries NOT stamped
        assert_eq!(s.dictionary_schema_version, 1);
    }

    #[test]
    fn migration_does_not_run_on_fresh_settings_with_version_current() {
        let mut s = get_default_settings();
        s.dictionary_schema_version = 1;
        s.dictionary_entries = vec![DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            ..entry_full("dict_1", "Robyn", Some("robin"))
        }];
        assert!(!migrate_dictionary_v1(&mut s));
        assert!(!s.dictionary_entries[0].user_confirmed); // untouched: created under the new system
    }

    #[test]
    fn migration_v2_repairs_duplicate_ids_in_order_and_is_idempotent() {
        let mut settings = get_default_settings();
        settings.dictionary_schema_version = 1;
        settings.dictionary_entries = vec![
            DictionaryEntry {
                priority: DictionaryEntryPriority::Starred,
                created_at_ms: 11,
                updated_at_ms: 12,
                user_confirmed: true,
                ..entry_full("dict_42_entry", "عبدالله", Some("عبدالة"))
            },
            DictionaryEntry {
                source: DictionaryEntrySource::Imported,
                created_at_ms: 21,
                updated_at_ms: 22,
                active: false,
                needs_review: true,
                ..entry_full("dict_42_entry", "العربية", None)
            },
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                created_at_ms: 31,
                updated_at_ms: 32,
                ..entry_full("dict_42_entry", "القاموس", Some("قاموس"))
            },
        ];
        let original = settings.dictionary_entries.clone();

        assert!(migrate_dictionary_v2(&mut settings));
        assert_eq!(settings.dictionary_schema_version, 2);
        assert_eq!(
            settings
                .dictionary_entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["dict_42_entry", "dict_42_entry-2", "dict_42_entry-3"]
        );
        for (before, after) in original.iter().zip(&settings.dictionary_entries) {
            let mut expected = before.clone();
            expected.id = after.id.clone();
            assert_eq!(&expected, after, "migration may only change duplicate IDs");
        }

        let migrated = settings.clone();
        assert!(!migrate_dictionary_v2(&mut settings));
        assert_eq!(settings.dictionary_entries, migrated.dictionary_entries);
        assert_eq!(
            settings.dictionary_schema_version,
            migrated.dictionary_schema_version
        );
    }

    #[test]
    fn migration_v3_keeps_highest_trust_then_first_and_records_discarded_rules() {
        let mut settings = get_default_settings();
        settings.dictionary_schema_version = 2;
        settings.dictionary_entries = vec![
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry_full(
                    "dict_auto_ar",
                    "\u{627}\u{654}\u{62d}\u{645}\u{62f}",
                    Some("\u{627}\u{62d}\u{645}\u{62f} \u{627}\u{644}\u{642}\u{62f}\u{64a}\u{645}"),
                )
            },
            entry_full("dict_manual_ar", "\u{623}\u{62d}\u{645}\u{62f}", None),
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry_full("dict_first_latin", "Cafe\u{301}", Some("cafe old"))
            },
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry_full("dict_second_latin", "Caf\u{e9}", Some("cafe newer"))
            },
        ];
        settings.dictionary_auto_learn_suppressed = vec!["Cafe\u{301}".into(), "Caf\u{e9}".into()];

        assert!(migrate_dictionary_v3(&mut settings));
        assert_eq!(settings.dictionary_schema_version, 3);
        assert_eq!(settings.dictionary_entries.len(), 2);
        assert_eq!(settings.dictionary_entries[0].id, "dict_manual_ar");
        assert_eq!(
            settings.dictionary_entries[0].phrase,
            "\u{623}\u{62d}\u{645}\u{62f}"
        );
        assert_eq!(settings.dictionary_entries[1].id, "dict_first_latin");
        assert_eq!(settings.dictionary_entries[1].phrase, "Cafe\u{301}");
        assert_eq!(settings.dictionary_learn_candidates.len(), 2);
        assert!(settings.dictionary_learn_candidates.iter().any(|candidate| {
            candidate.phrase == "\u{627}\u{654}\u{62d}\u{645}\u{62f}"
                && candidate.replacement_of.as_deref()
                    == Some("\u{627}\u{62d}\u{645}\u{62f} \u{627}\u{644}\u{642}\u{62f}\u{64a}\u{645}")
        }));
        assert!(settings
            .dictionary_learn_candidates
            .iter()
            .any(|candidate| {
                candidate.phrase == "Caf\u{e9}"
                    && candidate.replacement_of.as_deref() == Some("cafe newer")
            }));
        assert_eq!(settings.dictionary_auto_learn_suppressed.len(), 1);
        assert_eq!(
            settings.custom_words,
            vec![
                "\u{623}\u{62d}\u{645}\u{62f}".to_string(),
                "Cafe\u{301}".to_string()
            ]
        );

        let migrated = settings.clone();
        assert!(!migrate_dictionary_v3(&mut settings));
        assert_eq!(settings.dictionary_entries, migrated.dictionary_entries);
        assert_eq!(
            settings.dictionary_learn_candidates,
            migrated.dictionary_learn_candidates
        );
        assert_eq!(
            settings.dictionary_auto_learn_suppressed,
            migrated.dictionary_auto_learn_suppressed
        );
        assert_eq!(settings.custom_words, migrated.custom_words);
    }

    #[test]
    fn repaired_ids_keep_dictionary_mutations_entry_exact() {
        let mut migrated = get_default_settings();
        migrated.dictionary_schema_version = 1;
        migrated.dictionary_entries = vec![
            entry("dict_shared", "Alpha"),
            entry("dict_shared", "Bravo"),
            entry("dict_shared", "Charlie"),
        ];
        assert!(migrate_dictionary_v2(&mut migrated));
        let repaired_ids = migrated
            .dictionary_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();

        for (target_index, target_id) in repaired_ids.iter().enumerate() {
            let mut updated = migrated.clone();
            let next_phrase = format!("Updated {target_index}");
            update_entry(
                &mut updated,
                100 + target_index as u64,
                target_id,
                Some(next_phrase.clone()),
                None,
                None,
            )
            .expect("repaired ID updates exactly one entry");
            for (index, entry) in updated.dictionary_entries.iter().enumerate() {
                let expected_phrase = if index == target_index {
                    next_phrase.as_str()
                } else {
                    migrated.dictionary_entries[index].phrase.as_str()
                };
                assert_eq!(entry.phrase, expected_phrase);
            }

            let mut toggled = migrated.clone();
            set_entry_active(&mut toggled, 200 + target_index as u64, target_id, false)
                .expect("repaired ID toggles exactly one entry");
            for (index, entry) in toggled.dictionary_entries.iter().enumerate() {
                assert_eq!(entry.active, index != target_index);
            }

            let mut deleted = migrated.clone();
            let removed = delete_entries(&mut deleted, std::slice::from_ref(target_id))
                .expect("repaired ID deletes exactly one entry");
            assert_eq!(removed.len(), 1);
            assert_eq!(removed[0].id, *target_id);
            assert_eq!(deleted.dictionary_entries.len(), repaired_ids.len() - 1);
            assert!(deleted
                .dictionary_entries
                .iter()
                .all(|entry| entry.id != *target_id));
        }
    }

    #[test]
    fn approve_candidate_promotes_with_user_confirmed() {
        let mut s = get_default_settings();
        observe_correction(&mut s, 10, "s1", "robin", Some("Robyn"));
        let entry = approve_candidate(&mut s, 20, "Robyn", Some("robin")).expect("promoted");
        assert!(entry.user_confirmed);
        assert_eq!(entry.phrase, "Robyn");
        assert!(s.dictionary_learn_candidates.is_empty());
        assert_eq!(s.dictionary_entries.len(), 1);
    }

    #[test]
    fn approving_v3_recovered_rule_merges_into_retained_entry() {
        let mut settings = get_default_settings();
        settings.dictionary_schema_version = 2;
        settings.dictionary_entries = vec![
            entry_full("dict_retained", "Caf\u{e9}", None),
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry_full("dict_discarded", "Cafe\u{301}", Some("cafe old"))
            },
        ];

        assert!(migrate_dictionary_v3(&mut settings));
        assert_eq!(settings.dictionary_learn_candidates.len(), 1);

        let approved = approve_candidate(&mut settings, 42, "Cafe\u{301}", Some("cafe old"))
            .expect("recovered rule owns the retained phrase");

        assert_eq!(approved.id, "dict_retained");
        assert_eq!(approved.replacement_of.as_deref(), Some("cafe old"));
        assert!(approved.user_confirmed);
        assert!(settings.dictionary_learn_candidates.is_empty());
    }

    #[test]
    fn approving_conflicting_v3_rule_keeps_it_reviewable() {
        let mut settings = get_default_settings();
        settings.dictionary_schema_version = 2;
        settings.dictionary_entries = vec![
            entry_full("dict_retained", "Caf\u{e9}", Some("cafe current")),
            DictionaryEntry {
                source: DictionaryEntrySource::AutoLearned,
                ..entry_full("dict_discarded", "Cafe\u{301}", Some("cafe old"))
            },
        ];

        assert!(migrate_dictionary_v3(&mut settings));
        let approved = approve_candidate(&mut settings, 42, "Cafe\u{301}", Some("cafe old"))
            .expect("existing entry keeps ownership");

        assert_eq!(approved.id, "dict_retained");
        assert_eq!(approved.replacement_of.as_deref(), Some("cafe current"));
        assert_eq!(settings.dictionary_learn_candidates.len(), 1);
        assert_eq!(
            settings.dictionary_learn_candidates[0]
                .replacement_of
                .as_deref(),
            Some("cafe old")
        );
    }

    #[test]
    fn approve_candidate_requires_matching_pair() {
        let mut s = get_default_settings();
        observe_correction(&mut s, 10, "s1", "robin", Some("Robyn"));
        assert!(approve_candidate(&mut s, 20, "Robyn", Some("different")).is_none());
        assert_eq!(s.dictionary_learn_candidates.len(), 1); // untouched
    }

    #[test]
    fn reject_candidate_drops_all_variants_and_suppresses() {
        let mut s = get_default_settings();
        observe_correction(&mut s, 10, "s1", "robin", Some("Robyn"));
        observe_correction(&mut s, 20, "s2", "rob in", Some("Robyn")); // second source variant
        assert_eq!(s.dictionary_learn_candidates.len(), 2);
        reject_candidate(&mut s, "Robyn");
        assert!(s.dictionary_learn_candidates.is_empty());
        // suppressed: next observation is NoChange
        let out = observe_correction(&mut s, 30, "s3", "robin", Some("Robyn"));
        assert_eq!(out, ObserveOutcome::NoChange);
    }

    #[test]
    fn set_entry_active_toggles_and_clears_review_flag() {
        let mut s = get_default_settings();
        s.dictionary_entries.push(DictionaryEntry {
            source: DictionaryEntrySource::AutoLearned,
            active: false,
            needs_review: true,
            ..entry_full("dict_1", "their", Some("there"))
        });
        assert!(set_entry_active(&mut s, 99, "dict_1", true).expect("unique entry"));
        assert!(s.dictionary_entries[0].active);
        assert!(!s.dictionary_entries[0].needs_review);
        assert_eq!(s.dictionary_entries[0].updated_at_ms, 99);
        assert!(!set_entry_active(&mut s, 99, "missing", true).expect("missing is unambiguous"));
    }

    #[test]
    fn diagnostics_default_is_all_zero() {
        let d = crate::settings::DictionaryDiagnostics::default();
        assert_eq!(d.learned, 0);
        assert_eq!(d.promoted, 0);
        assert_eq!(d.skip_secure_field, 0);
        assert_eq!(d.since_ms, 0);
    }

    #[test]
    fn record_learn_outcomes_accumulates_and_stamps_since() {
        let mut s = get_default_settings();
        super::record_learn_outcomes(&mut s, 100, 2, 1, 3, 0);
        assert_eq!(s.dictionary_diagnostics.learned, 2);
        assert_eq!(s.dictionary_diagnostics.promoted, 1);
        assert_eq!(s.dictionary_diagnostics.reinforced, 3);
        assert_eq!(s.dictionary_diagnostics.since_ms, 100); // stamped on first event
        super::record_learn_outcomes(&mut s, 200, 1, 0, 0, 2);
        assert_eq!(s.dictionary_diagnostics.learned, 3);
        assert_eq!(s.dictionary_diagnostics.routed, 2);
        assert_eq!(s.dictionary_diagnostics.since_ms, 100); // unchanged after first
    }

    #[test]
    fn learn_outcomes_feed_diagnostics_counts() {
        // Simulate what learn_from_text_snapshots does: observe corrections, then record outcomes.
        let mut s = get_default_settings();
        let mut learned = 0u32;
        let mut promoted = 0u32;
        let mut reinforced = 0u32;
        for (session, dictated, corrected) in [("s1", "robin", "Robyn"), ("s2", "robin", "Robyn")] {
            match observe_correction(&mut s, 10, session, dictated, Some(corrected)) {
                ObserveOutcome::Learned => learned += 1,
                ObserveOutcome::Promoted => promoted += 1,
                ObserveOutcome::Reinforced => reinforced += 1,
                _ => {}
            }
        }
        record_learn_outcomes(&mut s, 10, learned, promoted, reinforced, 0);
        assert_eq!(s.dictionary_diagnostics.learned, 1); // first correction
        assert_eq!(s.dictionary_diagnostics.promoted, 1); // second promotes
        assert!(s.dictionary_diagnostics.since_ms > 0);
    }

    #[test]
    fn record_skip_maps_known_reasons_to_fields() {
        let mut s = get_default_settings();
        super::record_skip(&mut s, 50, "skip: secure_field");
        super::record_skip(&mut s, 60, "skip: secure_field");
        super::record_skip(&mut s, 70, "skip: read_cap_exceeded");
        super::record_skip(&mut s, 80, "skip: target_changed");
        super::record_skip(&mut s, 90, "skip: no_post_paste_change");
        super::record_skip(&mut s, 95, "skip: secure_check_error");
        super::record_skip(&mut s, 96, "skip: runtime_id_missing");
        super::record_skip(&mut s, 97, "skip: runtime_id_changed");
        let d = &s.dictionary_diagnostics;
        assert_eq!(d.skip_secure_field, 2);
        assert_eq!(d.skip_read_cap_exceeded, 1);
        assert_eq!(d.skip_target_changed, 1);
        assert_eq!(d.skip_no_post_paste_change, 1);
        assert_eq!(d.skip_secure_check_error, 1);
        assert_eq!(d.skip_runtime_id, 2); // missing + changed both map here
        assert_eq!(d.since_ms, 50);
    }

    #[test]
    fn record_skip_ignores_unknown_reason() {
        let mut s = get_default_settings();
        super::record_skip(&mut s, 10, "skip: something_new");
        // unknown reasons don't panic and don't stamp since (no known counter touched)
        assert_eq!(s.dictionary_diagnostics.since_ms, 0);
    }

    #[test]
    fn reset_diagnostics_zeroes_all_and_restamps() {
        let mut s = get_default_settings();
        super::record_learn_outcomes(&mut s, 100, 5, 2, 1, 0);
        super::reset_dictionary_diagnostics(&mut s, 999);
        assert_eq!(s.dictionary_diagnostics.learned, 0);
        assert_eq!(s.dictionary_diagnostics.promoted, 0);
        assert_eq!(s.dictionary_diagnostics.since_ms, 999); // reset stamps a fresh window start
    }

    fn candidate(phrase: &str, replacement_of: Option<&str>, updated_at_ms: u64) -> LearnCandidate {
        LearnCandidate {
            replacement_of: replacement_of.map(str::to_string),
            phrase: phrase.into(),
            occurrences: 1,
            last_evidence_session: None,
            created_at_ms: updated_at_ms,
            updated_at_ms,
        }
    }

    #[test]
    fn prune_drops_candidates_older_than_age_limit() {
        let mut s = get_default_settings();
        let now = 1_000_000_000_000u64;
        s.dictionary_learn_candidates = vec![
            candidate("Fresh", Some("fresh"), now - 1000), // recent
            candidate("Stale", Some("stale"), now - MAX_CANDIDATE_AGE_MS - 1), // just past limit
            candidate("Edge", Some("edge"), now - MAX_CANDIDATE_AGE_MS), // exactly at limit -> kept
        ];
        let removed = prune_learn_candidates(&mut s, now);
        assert_eq!(removed, 1);
        let phrases: Vec<&str> = s
            .dictionary_learn_candidates
            .iter()
            .map(|c| c.phrase.as_str())
            .collect();
        assert!(phrases.contains(&"Fresh") && phrases.contains(&"Edge"));
        assert!(!phrases.contains(&"Stale"));
    }

    #[test]
    fn prune_caps_store_evicting_oldest() {
        let mut s = get_default_settings();
        let now = 1_000_000_000_000u64;
        // MAX_LEARN_CANDIDATES + 3 candidates, all within age, distinct updated_at.
        s.dictionary_learn_candidates = (0..(MAX_LEARN_CANDIDATES as u64 + 3))
            .map(|i| candidate(&format!("p{i}"), Some(&format!("s{i}")), now - i * 10))
            .collect();
        let removed = prune_learn_candidates(&mut s, now);
        assert_eq!(removed, 3);
        assert_eq!(s.dictionary_learn_candidates.len(), MAX_LEARN_CANDIDATES);
        // The 3 evicted are the oldest (largest i => smallest updated_at). Newest (p0) survives.
        let phrases: Vec<String> = s
            .dictionary_learn_candidates
            .iter()
            .map(|c| c.phrase.clone())
            .collect();
        assert!(phrases.contains(&"p0".to_string()));
        assert!(!phrases.contains(&format!("p{}", MAX_LEARN_CANDIDATES as u64 + 2)));
    }

    #[test]
    fn prune_noop_when_within_limits() {
        let mut s = get_default_settings();
        let now = 1_000_000_000_000u64;
        s.dictionary_learn_candidates = vec![
            candidate("A", Some("a"), now),
            candidate("B", Some("b"), now),
        ];
        assert_eq!(prune_learn_candidates(&mut s, now), 0);
        assert_eq!(s.dictionary_learn_candidates.len(), 2);
    }

    #[test]
    fn observe_correction_prunes_stale_candidates() {
        let mut s = get_default_settings();
        let now = 1_000_000_000_000u64;
        s.dictionary_learn_candidates = vec![candidate(
            "Stale",
            Some("stale"),
            now - MAX_CANDIDATE_AGE_MS - 1,
        )];
        // A brand-new correction triggers pruning; the stale candidate is dropped and the new one learned.
        let out = observe_correction(&mut s, now, "s1", "robin", Some("Robyn"));
        assert_eq!(out, ObserveOutcome::Learned);
        let phrases: Vec<&str> = s
            .dictionary_learn_candidates
            .iter()
            .map(|c| c.phrase.as_str())
            .collect();
        assert!(phrases.contains(&"Robyn"));
        assert!(!phrases.contains(&"Stale"));
    }
}
