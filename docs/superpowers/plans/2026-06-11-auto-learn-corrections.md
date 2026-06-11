# Auto-Learn Corrections Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Wispr Flow-style auto-learn dictionary feature that watches text Verbatim just pasted, detects user corrections in the target field, adds plausible corrected words to `custom_words`, and exposes an undo path.

**Architecture:** Add a small `auto_learn` backend module with three responsibilities: pure correction extraction, short-lived focused-field monitoring, and dictionary updates. Start with Windows UI Automation because the current workspace is Windows and the repo already depends on the `windows` crate; macOS/Linux stay behind an unsupported no-op boundary and need separate future plans.

**Tech Stack:** Tauri 2, Rust, React/TypeScript, i18next, `windows` crate UI Automation, existing Tauri event and settings infrastructure.

---

## Assumptions

- Auto-learn is opt-in through settings. Default it to `false` unless the product decision is to match Wispr Flow and enable it by default.
- The feature never monitors general keyboard activity. It only reads the focused text field for a short window after Verbatim successfully pastes text.
- The first shippable target is Windows. macOS and Linux are not part of this executable plan; keep a no-op fallback boundary so the repo can compile on those platforms.
- Use `243ff052 refactor: rebrand Handy -> Verbatim across codebase` as the baseline rebrand commit. Preserve unrelated untracked `.superpowers/` and `docs/` files unless explicitly including plan artifacts.
- Process review fixes are incorporated in this revision: module declarations grow only when files exist, case-only corrections are detected, learned events use raw Tauri `emit` consistently, monitor changes are debounced, password fields are skipped, and rapid successive pastes cancel older monitor sessions.

## File Map

- Create `src-tauri/src/auto_learn/mod.rs`: public module exports and command wiring.
- Create `src-tauri/src/auto_learn/correction_learner.rs`: pure, testable diff and filtering logic.
- Create `src-tauri/src/auto_learn/manager.rs`: starts monitors, updates settings, emits learned-correction events, handles undo.
- Create `src-tauri/src/auto_learn/monitor.rs`: platform-neutral monitor trait and event type.
- Create `src-tauri/src/auto_learn/windows_monitor.rs`: Windows focused-field polling using UI Automation.
- Create `src-tauri/src/auto_learn/unsupported_monitor.rs`: no-op monitor for platforms not implemented in the current phase.
- macOS/Linux parity is intentionally a later architecture phase after the current Verbatim changes and core development are complete.
- Modify `src-tauri/src/settings.rs`: add persisted `auto_learn_corrections`.
- Modify `src-tauri/src/clipboard.rs`: start monitoring after successful paste.
- Modify `src-tauri/src/lib.rs`: register module and commands. Do not register the raw learned-corrections event in `collect_events!`.
- Modify `src-tauri/Cargo.toml`: add `Win32_UI_Accessibility` to existing `windows` crate features.
- Create `src/components/settings/AutoLearnCorrections.tsx`: settings toggle.
- Modify `src/components/settings/advanced/AdvancedSettings.tsx`: place toggle near custom words.
- Modify `src/stores/settingsStore.ts`: add settings updater.
- Modify `src/App.tsx`: listen for learned corrections and show undo toast.
- Modify `src/i18n/locales/en/translation.json`: add English strings.
- Regenerate `src/bindings.ts`: Tauri/Specta command bindings, or hand-edit the narrow command/settings shape if native dependency startup blocks `tauri dev`, then regenerate before release.

## Task 0: Preflight And Worktree Guard

**Files:**
- Inspect only.

- [ ] **Step 1: Confirm branch and dirty tree**

Run:

```powershell
git status --short --branch
```

Expected: current branch is `fix/windows-tray-first-shortcut`, ahead by `243ff052 refactor: rebrand Handy -> Verbatim across codebase`. Do not stage unrelated untracked `.superpowers/` or `docs/` files as part of this feature unless the plan artifact itself is intentionally being committed.

- [ ] **Step 2: Confirm dependency baseline**

Run:

```powershell
bun run build
```

Expected: frontend build succeeds. The rebrand commit already passed `bun run build` with only the existing Vite chunk-size warning.

- [ ] **Step 3: Confirm Rust baseline**

Run:

```powershell
Set-Location src-tauri
cargo test clipboard::tests --lib
Set-Location ..
```

Expected: existing focused Rust tests pass. If the native Whisper CMake build fails before Rust tests run, record the exact error and continue only with pure frontend or pure Rust tests that do not require that dependency.

## Task 1: Pure Correction Learner

**Files:**
- Create: `src-tauri/src/auto_learn/mod.rs`
- Create: `src-tauri/src/auto_learn/correction_learner.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add failing learner tests**

Create `src-tauri/src/auto_learn/mod.rs` with:

```rust
pub mod correction_learner;
```

Create `src-tauri/src/auto_learn/correction_learner.rs` with the tests first:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedCorrection {
    pub original: String,
    pub word: String,
}

pub fn extract_corrections(
    _original_text: &str,
    _field_value: &str,
    _existing_words: &[String],
) -> Vec<LearnedCorrection> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn learns_titlecase_name_correction() {
        let learned = extract_corrections(
            "I spoke with Shunade yesterday",
            "I spoke with Sinead yesterday",
            &[],
        );

        assert_eq!(
            learned,
            vec![LearnedCorrection {
                original: "Shunade".into(),
                word: "Sinead".into(),
            }]
        );
    }

    #[test]
    fn learns_camelcase_product_correction_from_split_words() {
        let learned = extract_corrections(
            "We use Charge B for invoices",
            "We use ChargeBee for invoices",
            &[],
        );

        assert_eq!(
            learned,
            vec![LearnedCorrection {
                original: "Charge B".into(),
                word: "ChargeBee".into(),
            }]
        );
    }

    #[test]
    fn learns_acronym_case_correction() {
        let learned = extract_corrections(
            "Send it through api gateway",
            "Send it through API gateway",
            &[],
        );

        assert_eq!(
            learned,
            vec![LearnedCorrection {
                original: "api".into(),
                word: "API".into(),
            }]
        );
    }

    #[test]
    fn ignores_existing_dictionary_words_case_insensitively() {
        let learned = extract_corrections(
            "I spoke with Shunade yesterday",
            "I spoke with Sinead yesterday",
            &words(&["sinead"]),
        );

        assert!(learned.is_empty());
    }

    #[test]
    fn ignores_punctuation_only_changes() {
        let learned = extract_corrections("Hello world", "Hello, world.", &[]);

        assert!(learned.is_empty());
    }

    #[test]
    fn ignores_large_rewrites() {
        let learned = extract_corrections(
            "Please send the report today",
            "The customer asked for a completely different reply",
            &[],
        );

        assert!(learned.is_empty());
    }

    #[test]
    fn ignores_common_everyday_word_replacements() {
        let learned = extract_corrections(
            "I will walk to the store",
            "I will drive to the store",
            &[],
        );

        assert!(learned.is_empty());
    }

    #[test]
    fn supports_unicode_names() {
        let learned = extract_corrections(
            "Talk to José tomorrow",
            "Talk to Josée tomorrow",
            &[],
        );

        assert_eq!(
            learned,
            vec![LearnedCorrection {
                original: "José".into(),
                word: "Josée".into(),
            }]
        );
    }
}
```

Modify `src-tauri/src/lib.rs` near the other `mod` declarations:

```rust
mod auto_learn;
```

- [ ] **Step 2: Run failing tests**

Run:

```powershell
Set-Location src-tauri
cargo test auto_learn::correction_learner --lib
Set-Location ..
```

Expected: tests fail because `extract_corrections` returns an empty vector.

- [ ] **Step 3: Implement learner**

Replace `src-tauri/src/auto_learn/correction_learner.rs` with:

```rust
use std::collections::HashSet;

use strsim::levenshtein;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearnedCorrection {
    pub original: String,
    pub word: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    normalized: String,
}

const MAX_CHANGED_RATIO: f64 = 0.5;
const MAX_DISTANCE_RATIO: f64 = 0.65;
const MAX_CANDIDATE_CHARS: usize = 60;

pub fn extract_corrections(
    original_text: &str,
    field_value: &str,
    existing_words: &[String],
) -> Vec<LearnedCorrection> {
    if original_text.trim().is_empty() || field_value.trim().is_empty() {
        return Vec::new();
    }

    let original_tokens = tokenize(original_text);
    let edited_tokens = tokenize(&find_edited_region(original_text, field_value));

    if original_tokens.is_empty() || edited_tokens.is_empty() {
        return Vec::new();
    }

    let diff_groups = diff_token_groups(&original_tokens, &edited_tokens);
    let changed_token_count: usize = diff_groups
        .iter()
        .map(|(original, edited)| original.len().max(edited.len()))
        .sum();
    if changed_token_count as f64 > original_tokens.len() as f64 * MAX_CHANGED_RATIO {
        return Vec::new();
    }

    let existing: HashSet<String> = existing_words
        .iter()
        .map(|word| normalize_for_compare(word))
        .collect();
    let mut seen = HashSet::new();
    let mut learned = Vec::new();

    for (original_group, edited_group) in diff_groups {
        if original_group.is_empty() || edited_group.is_empty() {
            continue;
        }

        let candidate = edited_group
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        if !is_candidate_dictionary_worthy(&candidate) {
            continue;
        }

        let normalized_candidate = normalize_for_compare(&candidate);
        if existing.contains(&normalized_candidate) || seen.contains(&normalized_candidate) {
            continue;
        }

        let original_text = original_group
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        if original_text == candidate {
            continue;
        }

        let original_joined = original_group
            .iter()
            .map(|token| token.normalized.as_str())
            .collect::<String>();
        let candidate_joined = edited_group
            .iter()
            .map(|token| token.normalized.as_str())
            .collect::<String>();

        let max_len = original_joined.chars().count().max(candidate_joined.chars().count());
        if max_len == 0 {
            continue;
        }

        let distance_ratio =
            levenshtein(&original_joined, &candidate_joined) as f64 / max_len as f64;
        if distance_ratio > MAX_DISTANCE_RATIO && !has_strong_shape_signal(&candidate) {
            continue;
        }

        seen.insert(normalized_candidate);
        learned.push(LearnedCorrection {
            original: original_text,
            word: candidate,
        });
    }

    learned
}

fn tokenize(text: &str) -> Vec<Token> {
    text.split_whitespace()
        .filter_map(|part| {
            let cleaned = trim_token(part);
            if cleaned.is_empty() {
                return None;
            }

            Some(Token {
                normalized: normalize_for_compare(cleaned),
                text: cleaned.to_string(),
            })
        })
        .collect()
}

fn trim_token(token: &str) -> &str {
    token.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '_' && ch != '-' && ch != '\'')
}

fn normalize_for_compare(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_alphanumeric() || *ch == '_' || *ch == '-')
        .flat_map(char::to_lowercase)
        .collect()
}

fn find_edited_region(original_text: &str, field_value: &str) -> String {
    if field_value.chars().count() <= original_text.chars().count() * 3 / 2 {
        return field_value.to_string();
    }

    if field_value.contains(original_text) {
        return original_text.to_string();
    }

    let original_tokens = tokenize(original_text);
    let field_tokens = tokenize(field_value);
    let window_size = original_tokens.len();

    if window_size == 0 || field_tokens.len() <= window_size {
        return field_value.to_string();
    }

    let mut best_start = 0;
    let mut best_score = 0;

    for start in 0..=(field_tokens.len() - window_size) {
        let score = original_tokens
            .iter()
            .enumerate()
            .filter(|(offset, token)| token.normalized == field_tokens[start + offset].normalized)
            .count();

        if score > best_score {
            best_score = score;
            best_start = start;
        }
    }

    if best_score * 10 < window_size * 3 {
        return field_value.to_string();
    }

    field_tokens[best_start..best_start + window_size]
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn diff_token_groups(original: &[Token], edited: &[Token]) -> Vec<(Vec<Token>, Vec<Token>)> {
    let lcs = lcs_table(original, edited);
    let mut aligned = Vec::new();
    let mut i = original.len();
    let mut j = edited.len();

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && original[i - 1].normalized == edited[j - 1].normalized {
            aligned.push((Some(original[i - 1].clone()), Some(edited[j - 1].clone())));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || lcs[i][j - 1] >= lcs[i - 1][j]) {
            aligned.push((None, Some(edited[j - 1].clone())));
            j -= 1;
        } else {
            aligned.push((Some(original[i - 1].clone()), None));
            i -= 1;
        }
    }

    aligned.reverse();

    let mut groups = Vec::new();
    let mut current_original = Vec::new();
    let mut current_edited = Vec::new();

    for (orig, edit) in aligned {
        if let (Some(orig_token), Some(edit_token)) = (&orig, &edit) {
            if orig_token.normalized == edit_token.normalized && orig_token.text == edit_token.text
            {
                if !current_original.is_empty() || !current_edited.is_empty() {
                    groups.push((current_original, current_edited));
                    current_original = Vec::new();
                    current_edited = Vec::new();
                }
                continue;
            }
        }

        if let Some(token) = orig {
            current_original.push(token);
        }
        if let Some(token) = edit {
            current_edited.push(token);
        }
    }

    if !current_original.is_empty() || !current_edited.is_empty() {
        groups.push((current_original, current_edited));
    }

    groups
}

fn lcs_table(original: &[Token], edited: &[Token]) -> Vec<Vec<usize>> {
    let mut dp = vec![vec![0; edited.len() + 1]; original.len() + 1];

    for i in 1..=original.len() {
        for j in 1..=edited.len() {
            dp[i][j] = if original[i - 1].normalized == edited[j - 1].normalized {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    dp
}

fn is_candidate_dictionary_worthy(candidate: &str) -> bool {
    let char_count = candidate.chars().count();
    if !(3..=MAX_CANDIDATE_CHARS).contains(&char_count) {
        return false;
    }
    if candidate.chars().any(|ch| matches!(ch, '<' | '>' | '"' | '&')) {
        return false;
    }

    let normalized = normalize_for_compare(candidate);
    if normalized.is_empty() {
        return false;
    }

    // Auto-learn should bias toward proper names, acronyms, product terms,
    // identifiers, and non-ASCII names. Lowercase everyday replacements are too
    // noisy for an automatic dictionary.
    has_strong_shape_signal(candidate) || has_non_ascii_letter(candidate)
}

fn has_strong_shape_signal(candidate: &str) -> bool {
    let letters: Vec<char> = candidate.chars().filter(|ch| ch.is_alphabetic()).collect();
    let uppercase_count = letters.iter().filter(|ch| ch.is_uppercase()).count();
    let lowercase_count = letters.iter().filter(|ch| ch.is_lowercase()).count();

    uppercase_count >= 2
        || (uppercase_count >= 1 && lowercase_count >= 1)
        || candidate.chars().any(|ch| ch.is_ascii_digit())
        || candidate.contains('-')
        || candidate.contains('_')
}

fn has_non_ascii_letter(candidate: &str) -> bool {
    candidate
        .chars()
        .any(|ch| ch.is_alphabetic() && !ch.is_ascii())
}
```

Keep the tests from Step 1 at the bottom of the file.

- [ ] **Step 4: Run learner tests**

Run:

```powershell
Set-Location src-tauri
cargo test auto_learn::correction_learner --lib
Set-Location ..
```

Expected: all `auto_learn::correction_learner` tests pass.

- [ ] **Step 5: Commit learner**

Run:

```powershell
git add src-tauri/src/auto_learn src-tauri/src/lib.rs
git commit -m "feat: extract learned dictionary corrections"
```

Expected: commit contains only the new learner module and `mod auto_learn;`.

## Task 2: Settings, Commands, And Undo

**Files:**
- Modify: `src-tauri/src/auto_learn/mod.rs`
- Modify: `src-tauri/src/settings.rs`
- Create: `src-tauri/src/auto_learn/manager.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/stores/settingsStore.ts`
- Regenerate: `src/bindings.ts`

- [ ] **Step 1: Write backend tests for merge and undo behavior**

First grow `src-tauri/src/auto_learn/mod.rs` now that `manager.rs` exists:

```rust
pub mod correction_learner;
pub mod manager;
```

Create `src-tauri/src/auto_learn/manager.rs` with:

```rust
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize, Type)]
pub struct LearnedCorrectionsPayload {
    pub words: Vec<String>,
}

pub fn merge_learned_words(existing: &[String], learned: &[String]) -> Vec<String> {
    let mut merged = existing.to_vec();
    for word in learned {
        if !merged.iter().any(|existing| existing.eq_ignore_ascii_case(word)) {
            merged.push(word.clone());
        }
    }
    merged
}

pub fn remove_learned_words(existing: &[String], words: &[String]) -> Vec<String> {
    existing
        .iter()
        .filter(|existing| !words.iter().any(|word| word.eq_ignore_ascii_case(existing)))
        .cloned()
        .collect()
}

pub fn emit_learned_corrections(app: &AppHandle, words: Vec<String>) {
    if words.is_empty() {
        return;
    }

    let _ = app.emit("auto-learn-corrections-learned", LearnedCorrectionsPayload { words });
}

#[tauri::command]
#[specta::specta]
pub fn undo_learned_corrections(app: AppHandle, words: Vec<String>) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.custom_words = remove_learned_words(&settings.custom_words, &words);
    crate::settings::write_settings(&app, settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_keeps_existing_order_and_appends_new_words() {
        let existing = vec!["Sinead".to_string()];
        let learned = vec!["ChargeBee".to_string(), "API".to_string()];

        assert_eq!(
            merge_learned_words(&existing, &learned),
            vec!["Sinead".to_string(), "ChargeBee".to_string(), "API".to_string()]
        );
    }

    #[test]
    fn merge_ignores_case_insensitive_duplicates() {
        let existing = vec!["Sinead".to_string()];
        let learned = vec!["sinead".to_string(), "ChargeBee".to_string()];

        assert_eq!(
            merge_learned_words(&existing, &learned),
            vec!["Sinead".to_string(), "ChargeBee".to_string()]
        );
    }

    #[test]
    fn undo_removes_case_insensitive_words() {
        let existing = vec!["Sinead".to_string(), "ChargeBee".to_string(), "API".to_string()];
        let words = vec!["chargebee".to_string()];

        assert_eq!(
            remove_learned_words(&existing, &words),
            vec!["Sinead".to_string(), "API".to_string()]
        );
    }
}
```

- [ ] **Step 2: Add settings field and command**

In `src-tauri/src/settings.rs`, add to `AppSettings` after `custom_words`:

```rust
    #[serde(default = "default_auto_learn_corrections")]
    pub auto_learn_corrections: bool,
```

Add near the other default functions:

```rust
fn default_auto_learn_corrections() -> bool {
    false
}
```

In `src-tauri/src/shortcut/mod.rs`, add:

```rust
#[tauri::command]
#[specta::specta]
pub fn change_auto_learn_corrections_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.auto_learn_corrections = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}
```

In `src-tauri/src/lib.rs`, add commands to `collect_commands!`:

```rust
            shortcut::change_auto_learn_corrections_setting,
            auto_learn::manager::undo_learned_corrections,
```

Do not add `LearnedCorrectionsPayload` to `collect_events!`. This plan uses raw `app.emit("auto-learn-corrections-learned", ...)` plus frontend `listen(...)`; registering the payload without deriving `tauri_specta::Event` will not compile.

- [ ] **Step 3: Add frontend updater**

In `src/stores/settingsStore.ts`, add to `settingUpdaters` after `custom_words`:

```ts
  auto_learn_corrections: (value) =>
    commands.changeAutoLearnCorrectionsSetting(value as boolean),
```

- [ ] **Step 4: Run backend tests**

Run:

```powershell
Set-Location src-tauri
cargo test auto_learn::manager --lib
Set-Location ..
```

Expected: merge and undo tests pass.

- [ ] **Step 5: Regenerate TypeScript bindings**

Run:

```powershell
bun run tauri dev
```

Expected: the app starts in development mode and `src/bindings.ts` gains:

```ts
changeAutoLearnCorrectionsSetting(enabled: boolean)
undoLearnedCorrections(words: string[])
auto_learn_corrections?: boolean
```

Stop the dev process after confirming bindings changed.

If `bun run tauri dev` is blocked by the native Whisper/CMake failure before Specta export runs, hand-edit only the narrow generated shape needed for this feature:

```ts
async changeAutoLearnCorrectionsSetting(enabled: boolean) : Promise<Result<null, string>> {
    return { status: "ok", data: await TAURI_INVOKE("change_auto_learn_corrections_setting", { enabled }) };
},

async undoLearnedCorrections(words: string[]) : Promise<Result<null, string>> {
    return { status: "ok", data: await TAURI_INVOKE("undo_learned_corrections", { words }) };
},
```

Also add `auto_learn_corrections?: boolean` to `AppSettings`. Mark the commit message or implementation note with `bindings hand-edited; regenerate before release`, and keep Task 8 as the final required regeneration/verification gate.

- [ ] **Step 6: Commit settings and commands**

Run:

```powershell
git add src-tauri/src/auto_learn/mod.rs src-tauri/src/settings.rs src-tauri/src/shortcut/mod.rs src-tauri/src/lib.rs src-tauri/src/auto_learn/manager.rs src/stores/settingsStore.ts src/bindings.ts
git commit -m "feat: add auto-learn settings and undo command"
```

Expected: commit contains settings, commands, manager helper tests, and regenerated bindings.

## Task 3: Windows Focused-Field Monitor

**Files:**
- Modify: `src-tauri/src/auto_learn/mod.rs`
- Create: `src-tauri/src/auto_learn/monitor.rs`
- Create: `src-tauri/src/auto_learn/windows_monitor.rs`
- Create: `src-tauri/src/auto_learn/unsupported_monitor.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Grow module declarations and add monitor trait**

First update `src-tauri/src/auto_learn/mod.rs`. Add only the modules that exist in this task:

```rust
pub mod correction_learner;
pub mod manager;
pub mod monitor;

#[cfg(target_os = "windows")]
pub mod windows_monitor;

#[cfg(not(target_os = "windows"))]
pub mod unsupported_monitor;
```

Create `src-tauri/src/auto_learn/monitor.rs`:

```rust
use std::{
    sync::{
        atomic::AtomicBool,
        Arc,
    },
    time::Duration,
};

#[derive(Debug, Clone)]
pub struct TextFieldChange {
    pub value: String,
}

pub type CancelFlag = Arc<AtomicBool>;

pub trait TextMonitor: Send + Sync + 'static {
    fn monitor_for_changes(
        &self,
        original_text: String,
        timeout: Duration,
        cancel: CancelFlag,
        on_change: Box<dyn Fn(TextFieldChange) + Send>,
    ) -> Result<(), String>;
}
```

Create `src-tauri/src/auto_learn/unsupported_monitor.rs`:

```rust
use std::time::Duration;

use super::monitor::{CancelFlag, TextFieldChange, TextMonitor};

pub struct UnsupportedTextMonitor;

impl TextMonitor for UnsupportedTextMonitor {
    fn monitor_for_changes(
        &self,
        _original_text: String,
        _timeout: Duration,
        _cancel: CancelFlag,
        _on_change: Box<dyn Fn(TextFieldChange) + Send>,
    ) -> Result<(), String> {
        Ok(())
    }
}
```

- [ ] **Step 2: Add Windows UI Automation feature**

Modify the existing Windows `windows` dependency in `src-tauri/Cargo.toml` to include:

```toml
  "Win32_UI_Accessibility",
```

- [ ] **Step 3: Implement Windows polling monitor**

Create `src-tauri/src/auto_learn/windows_monitor.rs`:

```rust
use std::{
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};

use windows::{
    core::{Interface, BSTR},
    Win32::{
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        System::Variant::VT_BOOL,
        UI::Accessibility::{
            CUIAutomation, IUIAutomation, IUIAutomationTextPattern,
            IUIAutomationValuePattern, UIA_IsPasswordPropertyId, UIA_TextPatternId,
            UIA_ValuePatternId,
        },
    },
};

use super::monitor::{CancelFlag, TextFieldChange, TextMonitor};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const MAX_VALUE_CHARS: usize = 10_000;

pub struct WindowsTextMonitor;

impl TextMonitor for WindowsTextMonitor {
    fn monitor_for_changes(
        &self,
        _original_text: String,
        timeout: Duration,
        cancel: CancelFlag,
        on_change: Box<dyn Fn(TextFieldChange) + Send>,
    ) -> Result<(), String> {
        thread::spawn(move || {
            if let Err(error) = monitor_focused_element(timeout, cancel, on_change) {
                log::debug!("Windows auto-learn monitor stopped: {error}");
            }
        });

        Ok(())
    }
}

fn monitor_focused_element(
    timeout: Duration,
    cancel: CancelFlag,
    on_change: Box<dyn Fn(TextFieldChange) + Send>,
) -> Result<(), String> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .map_err(|error| format!("CoInitializeEx failed: {error}"))?;
    }

    let result = unsafe { monitor_focused_element_inner(timeout, cancel, on_change) };

    unsafe {
        CoUninitialize();
    }

    result
}

unsafe fn monitor_focused_element_inner(
    timeout: Duration,
    cancel: CancelFlag,
    on_change: Box<dyn Fn(TextFieldChange) + Send>,
) -> Result<(), String> {
    let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
        .map_err(|error| format!("Failed to create UI Automation: {error}"))?;
    let focused = automation
        .GetFocusedElement()
        .map_err(|error| format!("Failed to get focused element: {error}"))?;
    if element_is_password(&focused) {
        return Err("focused element is a password field".to_string());
    }

    let mut last_value = read_element_value(&focused)?;
    let start = Instant::now();

    while !cancel.load(Ordering::Relaxed) && start.elapsed() < timeout {
        thread::sleep(POLL_INTERVAL);
        let current_value = match read_element_value(&focused) {
            Ok(value) => value,
            Err(error) => {
                log::debug!("Failed to read focused element value: {error}");
                continue;
            }
        };

        if current_value != last_value {
            last_value = current_value.clone();
            on_change(TextFieldChange {
                value: truncate_value(current_value),
            });
        }
    }

    Ok(())
}

unsafe fn element_is_password(
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
) -> bool {
    match element.GetCurrentPropertyValue(UIA_IsPasswordPropertyId) {
        Ok(value) => {
            value.Anonymous.Anonymous.vt == VT_BOOL
                && value.Anonymous.Anonymous.Anonymous.boolVal.0 != 0
        }
        Err(_) => false,
    }
}

unsafe fn read_element_value(
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
) -> Result<String, String> {
    if let Ok(pattern) = element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
    {
        let value = pattern
            .CurrentValue()
            .map_err(|error| format!("Failed to read ValuePattern: {error}"))?;
        return Ok(bstr_to_string(value));
    }

    let pattern = element
        .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        .map_err(|error| format!("Focused element has no readable text pattern: {error}"))?;
    let range = pattern
        .DocumentRange()
        .map_err(|error| format!("Failed to get document range: {error}"))?;
    let text = range
        .GetText(-1)
        .map_err(|error| format!("Failed to read TextPattern range: {error}"))?;

    Ok(bstr_to_string(text))
}

fn bstr_to_string(value: BSTR) -> String {
    value.to_string()
}

fn truncate_value(value: String) -> String {
    value.chars().take(MAX_VALUE_CHARS).collect()
}
```

If the `windows` crate method names or `VARIANT` field access differ on this version, use `cargo check` diagnostics to adjust only the UI Automation calls while preserving this public interface and the explicit password-field bail-out.

- [ ] **Step 4: Run Windows monitor typecheck**

Run:

```powershell
Set-Location src-tauri
cargo check --lib
Set-Location ..
```

Expected: crate typechecks, or the known native Whisper build error occurs before typechecking. If UI Automation method names fail, fix imports and method names until this module typechecks.

- [ ] **Step 5: Commit monitor interface**

Run:

```powershell
git add src-tauri/Cargo.toml src-tauri/src/auto_learn/mod.rs src-tauri/src/auto_learn/monitor.rs src-tauri/src/auto_learn/windows_monitor.rs src-tauri/src/auto_learn/unsupported_monitor.rs
git commit -m "feat: monitor pasted text corrections on Windows"
```

Expected: commit contains the monitor interface and Windows implementation.

## Task 4: Wire Monitor To Paste And Dictionary Updates

**Files:**
- Modify: `src-tauri/src/auto_learn/manager.rs`
- Modify: `src-tauri/src/clipboard.rs`

- [ ] **Step 1: Extend manager with start-after-paste flow**

Add to `src-tauri/src/auto_learn/manager.rs`:

```rust
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use once_cell::sync::Lazy;
use tauri::AppHandle;

use crate::auto_learn::correction_learner::extract_corrections;
use crate::auto_learn::monitor::{CancelFlag, TextFieldChange, TextMonitor};

const MONITOR_TIMEOUT: Duration = Duration::from_secs(30);
const DEBOUNCE_DELAY: Duration = Duration::from_secs(2);

struct ActiveMonitor {
    generation: u64,
    cancel: CancelFlag,
}

static NEXT_MONITOR_GENERATION: AtomicU64 = AtomicU64::new(1);
static ACTIVE_MONITOR: Lazy<Mutex<Option<ActiveMonitor>>> = Lazy::new(|| Mutex::new(None));

#[cfg(target_os = "windows")]
fn platform_monitor() -> impl TextMonitor {
    crate::auto_learn::windows_monitor::WindowsTextMonitor
}

#[cfg(not(target_os = "windows"))]
fn platform_monitor() -> impl TextMonitor {
    crate::auto_learn::unsupported_monitor::UnsupportedTextMonitor
}

pub fn start_after_successful_paste(app: AppHandle, pasted_text: String) {
    start_after_successful_paste_with_monitor(app, pasted_text, platform_monitor());
}

pub fn start_after_successful_paste_with_monitor<M>(
    app: AppHandle,
    pasted_text: String,
    monitor: M,
) where
    M: TextMonitor,
{
    let settings = crate::settings::get_settings(&app);
    if !settings.auto_learn_corrections || pasted_text.trim().is_empty() {
        return;
    }

    let generation = NEXT_MONITOR_GENERATION.fetch_add(1, Ordering::Relaxed);
    let cancel = Arc::new(AtomicBool::new(false));
    replace_active_monitor(generation, Arc::clone(&cancel));

    let latest_change: Arc<Mutex<Option<(u64, String)>>> = Arc::new(Mutex::new(None));
    let change_counter = Arc::new(AtomicU64::new(0));

    let monitor_result = monitor.monitor_for_changes(
        pasted_text.clone(),
        MONITOR_TIMEOUT,
        Arc::clone(&cancel),
        Box::new(move |change: TextFieldChange| {
            let change_id = change_counter.fetch_add(1, Ordering::Relaxed) + 1;
            if let Ok(mut latest) = latest_change.lock() {
                *latest = Some((change_id, change.value));
            }

            let app_for_debounce = app.clone();
            let original_for_debounce = pasted_text.clone();
            let latest_for_debounce = Arc::clone(&latest_change);
            let cancel_for_debounce = Arc::clone(&cancel);

            thread::spawn(move || {
                thread::sleep(DEBOUNCE_DELAY);

                if cancel_for_debounce.load(Ordering::Relaxed) || !is_active_generation(generation)
                {
                    return;
                }

                let field_value = latest_for_debounce.lock().ok().and_then(|latest| {
                    latest.as_ref().and_then(|(latest_id, value)| {
                        (*latest_id == change_id).then(|| value.clone())
                    })
                });

                if let Some(field_value) = field_value {
                    process_field_value(app_for_debounce, original_for_debounce, field_value);
                }
            });
        }),
    );

    if let Err(error) = monitor_result {
        log::debug!("Auto-learn monitor did not start: {error}");
    }
}

fn replace_active_monitor(generation: u64, cancel: CancelFlag) {
    if let Ok(mut active) = ACTIVE_MONITOR.lock() {
        if let Some(previous) = active.replace(ActiveMonitor { generation, cancel }) {
            previous.cancel.store(true, Ordering::Relaxed);
        }
    }
}

fn is_active_generation(generation: u64) -> bool {
    ACTIVE_MONITOR
        .lock()
        .ok()
        .and_then(|active| active.as_ref().map(|active| active.generation == generation))
        .unwrap_or(false)
}

fn process_field_value(app: AppHandle, original_text: String, field_value: String) {
    let mut settings = crate::settings::get_settings(&app);
    let corrections = extract_corrections(&original_text, &field_value, &settings.custom_words);
    if corrections.is_empty() {
        return;
    }

    let words: Vec<String> = corrections.into_iter().map(|correction| correction.word).collect();
    settings.custom_words = merge_learned_words(&settings.custom_words, &words);
    crate::settings::write_settings(&app, settings);
    emit_learned_corrections(&app, words);
}
```

Add manager tests using `start_after_successful_paste_with_monitor` and a fake `TextMonitor`:

- one fake monitor emits `Sine`, `Sinea`, `Sinead`; assert only the final debounced value is processed
- two rapid `start_after_successful_paste_with_monitor` calls cancel the first monitor; assert the first monitor's later emission is ignored
- disabled setting or empty pasted text never calls the fake monitor

- [ ] **Step 2: Start monitor after successful paste**

In `src-tauri/src/clipboard.rs`, import:

```rust
use crate::auto_learn;
```

After the paste method `match` succeeds and before auto-submit, add:

```rust
    if paste_method != PasteMethod::None {
        auto_learn::manager::start_after_successful_paste(app_handle.clone(), text.clone());
    }
```

- [ ] **Step 3: Run focused tests**

Run:

```powershell
Set-Location src-tauri
cargo test auto_learn --lib
cargo test clipboard::tests --lib
Set-Location ..
```

Expected: auto-learn tests and existing clipboard tests pass.

- [ ] **Step 4: Commit paste wiring**

Run:

```powershell
git add src-tauri/src/auto_learn/manager.rs src-tauri/src/clipboard.rs
git commit -m "feat: start correction learning after paste"
```

Expected: commit contains only manager and paste integration changes.

## Task 5: Settings UI Toggle

**Files:**
- Create: `src/components/settings/AutoLearnCorrections.tsx`
- Modify: `src/components/settings/advanced/AdvancedSettings.tsx`
- Modify: `src/i18n/locales/en/translation.json`

- [ ] **Step 1: Create toggle component**

Create `src/components/settings/AutoLearnCorrections.tsx`:

```tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface AutoLearnCorrectionsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const AutoLearnCorrections: React.FC<AutoLearnCorrectionsProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled = getSetting("auto_learn_corrections") ?? false;

    return (
      <ToggleSwitch
        checked={enabled}
        onChange={(enabled) => updateSetting("auto_learn_corrections", enabled)}
        isUpdating={isUpdating("auto_learn_corrections")}
        label={t("settings.advanced.autoLearnCorrections.label")}
        description={t("settings.advanced.autoLearnCorrections.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  });
```

- [ ] **Step 2: Add toggle near custom words**

In `src/components/settings/advanced/AdvancedSettings.tsx`, add import:

```tsx
import { AutoLearnCorrections } from "../AutoLearnCorrections";
```

Inside the transcription `SettingsGroup`, directly after `CustomWords`:

```tsx
        <AutoLearnCorrections descriptionMode="tooltip" grouped={true} />
```

- [ ] **Step 3: Add English translations**

In `src/i18n/locales/en/translation.json`, under `settings.advanced`, add:

```json
"autoLearnCorrections": {
  "label": "Auto-learn from corrections",
  "description": "When enabled, Verbatim watches text it just pasted for a short time and adds corrected names, acronyms, and uncommon terms to your custom dictionary."
}
```

Keep valid JSON commas around the inserted object.

- [ ] **Step 4: Run frontend build**

Run:

```powershell
bun run build
```

Expected: TypeScript and Vite build pass.

- [ ] **Step 5: Commit UI toggle**

Run:

```powershell
git add src/components/settings/AutoLearnCorrections.tsx src/components/settings/advanced/AdvancedSettings.tsx src/i18n/locales/en/translation.json
git commit -m "feat: add auto-learn corrections setting"
```

Expected: commit contains only UI toggle and English source text.

## Task 6: Learned Toast And Undo Action

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/i18n/locales/en/translation.json`

- [ ] **Step 1: Add event listener and toast**

In `src/App.tsx`, add a payload type near the existing event types:

```tsx
type LearnedCorrectionsPayload = {
  words: string[];
};
```

Add a `useEffect` after the paste-error listener:

```tsx
  useEffect(() => {
    const unlisten = listen<LearnedCorrectionsPayload>(
      "auto-learn-corrections-learned",
      (event) => {
        const words = event.payload.words;
        if (!words.length) {
          return;
        }

        const wordList = words.join(", ");
        toast.success(t("autoLearn.addedTitle", { words: wordList }), {
          description: t("autoLearn.addedDescription"),
          action: {
            label: t("autoLearn.undo"),
            onClick: () => {
              commands.undoLearnedCorrections(words);
            },
          },
        });
      },
    );

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [t]);
```

If `commands` is not currently imported in `App.tsx`, import it from `./bindings`.

- [ ] **Step 2: Add toast translations**

In `src/i18n/locales/en/translation.json`, add a top-level object:

```json
"autoLearn": {
  "addedTitle": "Added {{words}} to your dictionary",
  "addedDescription": "Verbatim learned this from a correction you made after dictation.",
  "undo": "Undo"
}
```

- [ ] **Step 3: Run frontend build**

Run:

```powershell
bun run build
```

Expected: build passes.

- [ ] **Step 4: Commit toast and undo UI**

Run:

```powershell
git add src/App.tsx src/i18n/locales/en/translation.json
git commit -m "feat: show learned correction undo toast"
```

Expected: commit contains only toast listener and English source strings.

## Task 7: Manual Windows Verification

**Files:**
- Inspect runtime behavior only.

- [ ] **Step 1: Run the app**

Run:

```powershell
bun run tauri dev
```

Expected: Verbatim launches.

- [ ] **Step 2: Enable the feature**

In the app:

1. Open Settings.
2. Open Advanced.
3. Enable `Auto-learn from corrections`.

Expected: no error toast; setting remains enabled after closing and reopening settings.

- [ ] **Step 3: Verify a learned name**

In Notepad or another plain text field:

1. Dictate text that produces `I spoke with Shunade yesterday`.
2. After Verbatim pastes it, edit `Shunade` to `Sinead`.

Expected:

- Verbatim shows `Added Sinead to your dictionary`.
- `Sinead` appears in Custom Words.

- [ ] **Step 4: Verify undo**

Click the toast Undo action.

Expected:

- `Sinead` is removed from Custom Words.
- No unrelated dictionary words are removed.

- [ ] **Step 5: Verify common-word filter**

In Notepad:

1. Dictate `I will walk to the store`.
2. Edit `walk` to `drive`.

Expected: no learned correction toast and no `drive` entry in Custom Words.

- [ ] **Step 6: Verify password fields are skipped**

In a password field:

1. Paste or dictate text while the feature is enabled.
2. Edit the field contents within the monitor timeout.

Expected: no learned correction toast, no custom word change, and debug logs show the monitor stopped because the focused element is a password field.

- [ ] **Step 7: Verify debounce prevents partial words**

In Notepad:

1. Dictate text that produces `I spoke with Shunade`.
2. Edit `Shunade` toward `Sinead` slowly enough to create intermediate values like `Sine` and `Sinea`, but finish within the debounce window.

Expected: only `Sinead` is learned after the field is quiet; no partial values are added.

- [ ] **Step 8: Verify rapid paste cancellation**

In Notepad:

1. Trigger a dictation paste.
2. Before editing it, trigger another dictation paste into another text field.
3. Edit the first field after the second paste starts monitoring.

Expected: edits to the first field are ignored; only the latest paste session can learn corrections.

- [ ] **Step 9: Stop dev server**

Stop the `bun run tauri dev` process with `Ctrl+C`.

Expected: no orphaned monitor process remains. On Windows, confirm with:

```powershell
Get-Process | Where-Object { $_.ProcessName -like '*verbatim*' -or $_.ProcessName -like '*text*monitor*' }
```

## Future Platform Plans

macOS and Linux monitor parity are intentionally out of scope for this execution plan. For this feature slice, non-Windows platforms use `unsupported_monitor.rs`, return `Ok(())`, and never block paste or show user-facing errors when focused-field monitoring is unavailable.

After the current Verbatim changes and core development are complete, run a dedicated parity phase for:

- macOS AXObserver monitoring with explicit Accessibility permission behavior.
- Linux AT-SPI monitoring with Wayland limitations documented and verified.

## Task 8: Final Verification

**Files:**
- Inspect full working tree.

- [ ] **Step 1: Format Rust**

Run:

```powershell
bun run format:backend
```

Expected: Rust formatting completes.

- [ ] **Step 2: Format touched frontend files**

Run:

```powershell
bunx prettier --write src/App.tsx src/components/settings/AutoLearnCorrections.tsx src/components/settings/advanced/AdvancedSettings.tsx src/i18n/locales/en/translation.json src/stores/settingsStore.ts src/bindings.ts
```

Expected: Prettier formats only touched frontend files.

- [ ] **Step 3: Run focused Rust tests**

Run:

```powershell
Set-Location src-tauri
cargo test auto_learn --lib
cargo test clipboard::tests --lib
Set-Location ..
```

Expected: focused Rust tests pass, or native Whisper build failure is recorded if it blocks test startup before compiling these modules.

- [ ] **Step 4: Run frontend build**

Run:

```powershell
bun run build
```

Expected: TypeScript and Vite build pass.

- [ ] **Step 5: Run lint**

Run:

```powershell
bun run lint
```

Expected: ESLint passes for `src`.

- [ ] **Step 6: Review final diff**

Run:

```powershell
git diff --stat
git diff -- src-tauri/src/auto_learn src-tauri/src/settings.rs src-tauri/src/clipboard.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src/App.tsx src/components/settings src/stores/settingsStore.ts src/i18n/locales/en/translation.json src/bindings.ts
```

Expected: diff contains only auto-learn feature changes and does not include unrelated rename cleanup.

- [ ] **Step 7: Verify no executable placeholders remain**

Run:

```powershell
rg -n '^## Task (9|10):' docs/superpowers/plans/2026-06-11-auto-learn-corrections.md
rg -n "TODO placeholder|macOS Monitor Parity|Linux Monitor Parity|collect_events!.*LearnedCorrectionsPayload" src-tauri/src src
```

Expected: no stale plan task numbers, no platform-parity placeholder tasks, and no learned-corrections payload registered through `collect_events!`.

- [ ] **Step 8: Commit final verification fixes**

If formatting or small fixes changed files, run:

```powershell
git add src-tauri/src/auto_learn src-tauri/src/settings.rs src-tauri/src/clipboard.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock src/App.tsx src/components/settings src/stores/settingsStore.ts src/i18n/locales/en/translation.json src/bindings.ts
git commit -m "chore: verify auto-learn corrections"
```

Expected: no commit is created if there were no verification fixes.

## Acceptance Criteria

- Auto-learn setting persists across restarts.
- When disabled, no focused-field monitoring starts after paste.
- When enabled on Windows, correcting a pasted proper noun adds the corrected word to `custom_words`.
- Password fields are detected through UI Automation and skipped before any text value is read.
- Intermediate typing states inside the debounce window are ignored; only the final quiescent field value is processed.
- A new paste cancels the previous monitor session, so late events from older paste sessions cannot add dictionary words.
- Learned words are deduplicated case-insensitively.
- Common word replacements and large rewrites are ignored.
- Learned corrections emit a toast with Undo.
- Undo removes only the words learned from that toast.
- Paste still works when the monitor cannot read the focused element.
- No general keyboard logging is introduced.
- `bun run build`, `bun run lint`, and focused Rust tests pass or have a recorded native dependency blocker unrelated to the feature.

## Self-Review

- Spec coverage: the plan covers opt-in settings, post-paste monitoring, diff extraction, custom dictionary updates, undo, frontend feedback, and Windows runtime rollout.
- Placeholder scan: macOS/Linux monitor parity is moved to future plans, and no executable task depends on undefined platform work.
- Type consistency: `auto_learn_corrections`, `LearnedCorrectionsPayload`, `auto-learn-corrections-learned`, and `undo_learned_corrections` are used consistently across backend, bindings, and frontend.
