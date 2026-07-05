use log::{debug, info};
use std::fmt;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const POST_PASTE_SETTLE_DELAY: Duration = Duration::from_millis(120);
const POST_PASTE_POLL_INTERVAL: Duration = Duration::from_millis(150);
const POST_PASTE_STABLE_EDIT_DELAY: Duration = Duration::from_millis(300);
const POST_PASTE_LEARNING_WINDOW: Duration = Duration::from_secs(6);
/// Cross-platform cap on focused-text size read/diffed by the learning watcher.
/// Aligned with the Windows UIA read cap (MAX_UIA_TEXT_CHARS). Documents/fields larger
/// than this abort learning (better no learning than an unbounded diff loop).
pub const MAX_FOCUSED_TEXT_CHARS: usize = 20_000;
#[cfg(any(target_os = "macos", target_os = "linux", test))]
const SELECTION_FIELD_SEPARATOR: &str = "\n--VERBATIM-SELECTED-TEXT--\n";
#[cfg(any(target_os = "macos", target_os = "linux", test))]
const TEXT_FIELD_SEPARATOR: &str = "\n--VERBATIM-FOCUSED-TEXT--\n";

/// Returns false when `text` exceeds [`MAX_FOCUSED_TEXT_CHARS`]. Callers should treat
/// an over-cap snapshot as a skip (fail closed) rather than diffing it.
pub fn focused_text_within_cap(text: &str) -> bool {
    text.chars().count() <= MAX_FOCUSED_TEXT_CHARS
}

#[derive(Clone, PartialEq, Eq)]
pub struct FocusedTextSnapshot {
    pub target_id: String,
    pub text: String,
}

#[derive(Clone, PartialEq, Eq)]
pub enum FocusedTextSelection {
    Selected(String),
    Empty,
    Unsupported(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct FocusedTextSelectionSnapshot {
    pub target_id: String,
    pub text: String,
    pub selection: FocusedTextSelection,
}

impl fmt::Debug for FocusedTextSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FocusedTextSnapshot")
            .field("target_id", &self.target_id)
            .field("text_len", &self.text.chars().count())
            .finish()
    }
}

impl fmt::Debug for FocusedTextSelection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Selected(selected_text) => formatter
                .debug_tuple("Selected")
                .field(&format_args!("{} chars", selected_text.chars().count()))
                .finish(),
            Self::Empty => formatter.write_str("Empty"),
            Self::Unsupported(reason) => {
                formatter.debug_tuple("Unsupported").field(reason).finish()
            }
        }
    }
}

impl fmt::Debug for FocusedTextSelectionSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FocusedTextSelectionSnapshot")
            .field("target_id", &self.target_id)
            .field("text_len", &self.text.chars().count())
            .field("selection", &self.selection)
            .finish()
    }
}

pub fn maybe_spawn_auto_add_watcher(
    app: AppHandle,
    inserted_text: String,
    before_paste: Option<FocusedTextSnapshot>,
) {
    if crate::private_session::is_enabled(&app) {
        return;
    }

    let Some(before_paste) = before_paste else {
        return;
    };

    if inserted_text.trim().is_empty() {
        return;
    }

    // Stable per-paste session id for the state machine's per-session dedup. `target_id`
    // can contain window titles, so only its length (a little entropy) is folded in here
    // rather than the id itself, keeping window titles out of logs.
    let session_id = format!(
        "paste_{}_{}",
        crate::dictionary::current_unix_ms(),
        before_paste.target_id.len()
    );

    std::thread::spawn(move || {
        if let Err(error) =
            watch_for_post_paste_correction(app, session_id, inserted_text, before_paste)
        {
            debug!("Post-paste dictionary watcher skipped: {}", error);
        }
    });
}

pub fn capture_focused_text_snapshot() -> Option<FocusedTextSnapshot> {
    capture_platform_focused_text_snapshot()
        .map_err(|error| {
            debug!("Focused text snapshot unavailable: {}", error);
            error
        })
        .ok()
}

#[allow(dead_code)]
pub fn capture_focused_text_selection_snapshot() -> Result<FocusedTextSelectionSnapshot, String> {
    capture_platform_focused_text_selection_snapshot()
}

fn watch_for_post_paste_correction(
    app: AppHandle,
    session_id: String,
    inserted_text: String,
    before_paste: FocusedTextSnapshot,
) -> Result<(), String> {
    if !focused_text_within_cap(&before_paste.text) {
        return Err("skip: read_cap_exceeded".to_string());
    }

    std::thread::sleep(POST_PASTE_SETTLE_DELAY);

    let after_paste = capture_matching_snapshot(&before_paste.target_id)?
        .ok_or_else(|| "skip: target_changed".to_string())?;

    if !focused_text_within_cap(&after_paste.text) {
        return Err("skip: read_cap_exceeded".to_string());
    }

    if after_paste.text == before_paste.text {
        return Err("skip: no_post_paste_change".to_string());
    }

    let deadline = Instant::now() + POST_PASTE_LEARNING_WINDOW;
    let mut last_seen_text = after_paste.text.clone();
    let mut changed_text: Option<String> = None;
    let mut stable_since = Instant::now();

    while Instant::now() < deadline {
        std::thread::sleep(POST_PASTE_POLL_INTERVAL);

        let Some(current) = capture_matching_snapshot(&before_paste.target_id)? else {
            return Err("skip: target_changed".to_string());
        };

        if !focused_text_within_cap(&current.text) {
            return Err("skip: read_cap_exceeded".to_string());
        }

        if current.text != last_seen_text {
            last_seen_text = current.text.clone();
            changed_text = Some(current.text);
            stable_since = Instant::now();
            continue;
        }

        if let Some(candidate_text) = changed_text.as_deref() {
            if stable_since.elapsed() >= POST_PASTE_STABLE_EDIT_DELAY {
                learn_from_text_snapshots(
                    &app,
                    &session_id,
                    &inserted_text,
                    &before_paste.text,
                    &after_paste.text,
                    candidate_text,
                )?;
                return Ok(());
            }
        }
    }

    if let Some(candidate_text) = changed_text {
        learn_from_text_snapshots(
            &app,
            &session_id,
            &inserted_text,
            &before_paste.text,
            &after_paste.text,
            &candidate_text,
        )?;
    }

    Ok(())
}

fn capture_matching_snapshot(target_id: &str) -> Result<Option<FocusedTextSnapshot>, String> {
    let snapshot = capture_platform_focused_text_snapshot()?;
    if snapshot.target_id == target_id {
        Ok(Some(snapshot))
    } else {
        Ok(None)
    }
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
fn focused_text_snapshot_from_parts(
    target_id: String,
    text: String,
) -> Result<FocusedTextSnapshot, String> {
    if target_id.trim().is_empty() {
        return Err("focused text snapshot command returned an empty target id".to_string());
    }

    Ok(FocusedTextSnapshot { target_id, text })
}

fn learn_from_text_snapshots(
    app: &AppHandle,
    session_id: &str,
    inserted_text: &str,
    before_paste_text: &str,
    after_paste_text: &str,
    after_edit_text: &str,
) -> Result<(), String> {
    if crate::private_session::is_enabled(app) {
        return Ok(());
    }

    let Some(corrected_text) =
        extract_corrected_inserted_text(before_paste_text, after_paste_text, after_edit_text)
    else {
        return Ok(());
    };

    let inserted_text = strip_paste_direction_marks(inserted_text);
    let corrected_text = strip_paste_direction_marks(&corrected_text);

    let now_ms = crate::dictionary::current_unix_ms();
    let (promoted_entries, learned_count, routed_count) =
        crate::settings::mutate_settings_locked(app, |settings| {
            let candidates = crate::dictionary_learning::infer_auto_learn_candidates(
                &inserted_text,
                &corrected_text,
                &settings.custom_words,
            );

            let mut promoted = Vec::new();
            let mut learned = 0usize;
            let mut routed = 0usize;
            for candidate in candidates {
                let dictated = candidate.replacement_of.as_deref().unwrap_or("");
                match crate::dictionary::observe_correction(
                    settings,
                    now_ms,
                    session_id,
                    dictated,
                    Some(&candidate.phrase),
                ) {
                    crate::dictionary::ObserveOutcome::Promoted => {
                        // Promotion pushes the new entry last onto `dictionary_entries`
                        // (see `promote_candidate_to_entry`), so grabbing `.last()` here,
                        // within the same closure iteration, is safe.
                        if let Some(entry) = settings.dictionary_entries.last() {
                            promoted.push(entry.clone());
                        }
                    }
                    crate::dictionary::ObserveOutcome::Learned => learned += 1,
                    crate::dictionary::ObserveOutcome::Routed => routed += 1,
                    _ => {}
                }
            }
            (promoted, learned, routed)
        });

    // Emit AFTER the lock is released.
    if !promoted_entries.is_empty() {
        let promoted_words = promoted_entries
            .iter()
            .map(|entry| entry.phrase.clone())
            .collect::<Vec<_>>();
        let _ = app.emit("dictionary-entries-learned", promoted_entries.clone());
        let _ = app.emit("custom-words-learned", promoted_words);
    }
    if learned_count > 0 {
        // New event for the (future) review-queue UI; payload is intentionally phrase-free
        // (the store re-fetches candidates via command).
        let _ = app.emit("dictionary-candidates-learned", learned_count);
    }

    info!(
        "{}",
        auto_learn_outcome_log_message(promoted_entries.len(), learned_count, routed_count)
    );

    Ok(())
}

fn auto_learn_outcome_log_message(promoted: usize, learned: usize, routed: usize) -> String {
    format!("Auto-learn pass: {promoted} promoted, {learned} new candidates, {routed} routed as feedback")
}

pub fn extract_corrected_inserted_text(
    before_paste_text: &str,
    after_paste_text: &str,
    after_edit_text: &str,
) -> Option<String> {
    if before_paste_text == after_paste_text || after_paste_text == after_edit_text {
        return None;
    }

    let before_chars: Vec<char> = before_paste_text.chars().collect();
    let after_paste_chars: Vec<char> = after_paste_text.chars().collect();
    let after_edit_chars: Vec<char> = after_edit_text.chars().collect();

    let prefix_len = common_prefix_len(&before_chars, &after_paste_chars);
    let suffix_len = common_suffix_len(
        &before_chars[prefix_len..],
        &after_paste_chars[prefix_len..],
    );

    if prefix_len + suffix_len < before_chars.len() {
        return None;
    }

    if prefix_len + suffix_len > after_paste_chars.len()
        || prefix_len + suffix_len > after_edit_chars.len()
    {
        return None;
    }

    if !same_chars(&before_chars[..prefix_len], &after_edit_chars[..prefix_len]) {
        return None;
    }

    if suffix_len > 0 {
        let before_suffix = &before_chars[before_chars.len() - suffix_len..];
        let edit_suffix = &after_edit_chars[after_edit_chars.len() - suffix_len..];
        if !same_chars(before_suffix, edit_suffix) {
            return None;
        }
    }

    let corrected_end = after_edit_chars.len() - suffix_len;
    if corrected_end <= prefix_len {
        return None;
    }

    let corrected: String = after_edit_chars[prefix_len..corrected_end].iter().collect();
    let corrected = strip_paste_direction_marks(corrected.trim());
    if corrected.is_empty() {
        return None;
    }

    Some(corrected)
}

fn strip_paste_direction_marks(text: &str) -> String {
    text.chars()
        .filter(|ch| {
            !matches!(
                ch,
                '\u{200E}'
                    | '\u{200F}'
                    | '\u{202A}'
                    | '\u{202B}'
                    | '\u{202C}'
                    | '\u{202D}'
                    | '\u{202E}'
                    | '\u{2066}'
                    | '\u{2067}'
                    | '\u{2068}'
                    | '\u{2069}'
            )
        })
        .collect()
}

fn common_prefix_len(left: &[char], right: &[char]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(left, right)| left == right)
        .count()
}

fn common_suffix_len(left: &[char], right: &[char]) -> usize {
    left.iter()
        .rev()
        .zip(right.iter().rev())
        .take_while(|(left, right)| left == right)
        .count()
}

fn same_chars(left: &[char], right: &[char]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| left == right)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn capture_platform_focused_text_snapshot() -> Result<FocusedTextSnapshot, String> {
    Err("post-paste dictionary learning is not implemented on this platform yet".to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn capture_platform_focused_text_selection_snapshot() -> Result<FocusedTextSelectionSnapshot, String>
{
    Err("selected-text capture is not implemented on this platform yet".to_string())
}

#[cfg(target_os = "macos")]
fn capture_platform_focused_text_snapshot() -> Result<FocusedTextSnapshot, String> {
    macos_focused_text::capture()
}

#[cfg(target_os = "macos")]
fn capture_platform_focused_text_selection_snapshot() -> Result<FocusedTextSelectionSnapshot, String>
{
    macos_focused_text::capture_with_selection()
}

#[cfg(target_os = "linux")]
fn capture_platform_focused_text_snapshot() -> Result<FocusedTextSnapshot, String> {
    linux_focused_text::capture()
}

#[cfg(target_os = "linux")]
fn capture_platform_focused_text_selection_snapshot() -> Result<FocusedTextSelectionSnapshot, String>
{
    linux_focused_text::capture_with_selection()
}

#[cfg(target_os = "windows")]
fn capture_platform_focused_text_snapshot() -> Result<FocusedTextSnapshot, String> {
    windows_focused_text::capture()
}

#[cfg(target_os = "windows")]
fn capture_platform_focused_text_selection_snapshot() -> Result<FocusedTextSelectionSnapshot, String>
{
    windows_focused_text::capture_with_selection()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn parse_snapshot_output(output: &[u8]) -> Result<FocusedTextSnapshot, String> {
    let stdout = String::from_utf8_lossy(output);
    let Some((target_id, text)) = stdout.split_once('\n') else {
        return Err("focused text snapshot command returned malformed output".to_string());
    };

    focused_text_snapshot_from_parts(
        target_id.trim().to_string(),
        text.trim_end_matches(['\r', '\n']).to_string(),
    )
}

#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn parse_selection_snapshot_output(output: &[u8]) -> Result<FocusedTextSelectionSnapshot, String> {
    let stdout = String::from_utf8_lossy(output);
    let Some((target_id, rest)) = stdout.split_once(SELECTION_FIELD_SEPARATOR) else {
        return Err("focused text selection command returned malformed output".to_string());
    };
    let Some((selected_text, focused_text)) = rest.split_once(TEXT_FIELD_SEPARATOR) else {
        return Err("focused text selection command returned malformed output".to_string());
    };

    let target_id = target_id.trim().to_string();
    if target_id.is_empty() {
        return Err("focused text selection command returned an empty target id".to_string());
    }

    let selected_text = selected_text.trim_end_matches(['\r', '\n']).to_string();
    let focused_text = focused_text.trim_end_matches(['\r', '\n']).to_string();
    let selection = if selected_text.trim().is_empty() {
        FocusedTextSelection::Empty
    } else {
        FocusedTextSelection::Selected(selected_text)
    };

    Ok(FocusedTextSelectionSnapshot {
        target_id,
        text: focused_text,
        selection,
    })
}

/// Classifies the "secure field" sentinels emitted by the macOS/Linux focused-text
/// capture scripts. Single-sourced so the mac (stdout) and Linux (stderr) capture
/// paths share identical fail-closed classification logic, and so the logic is
/// exercised by unit tests on every platform even though the capture code itself
/// only compiles on macOS/Linux.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn classify_secure_sentinel(stdout: &str, stderr: &str) -> Option<&'static str> {
    if stdout.trim() == "__VERBATIM_SECURE__" || stderr.contains("__VERBATIM_SECURE__") {
        return Some("skip: secure_field");
    }
    if stderr.contains("__VERBATIM_SECURE_CHECK_ERROR__") {
        return Some("skip: secure_check_error");
    }
    None
}

#[cfg(target_os = "macos")]
mod macos_focused_text {
    use super::{
        parse_selection_snapshot_output, parse_snapshot_output, Command,
        FocusedTextSelectionSnapshot, FocusedTextSnapshot,
    };

    pub fn capture() -> Result<FocusedTextSnapshot, String> {
        let output = Command::new("osascript")
            .args(["-e", MACOS_FOCUSED_TEXT_SCRIPT])
            .output()
            .map_err(|error| format!("failed to run osascript: {}", error))?;

        if !output.status.success() {
            return Err(format!(
                "macOS Accessibility snapshot failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Some(skip) = super::classify_secure_sentinel(&stdout, "") {
            return Err(skip.to_string());
        }

        parse_snapshot_output(&output.stdout)
    }

    pub fn capture_with_selection() -> Result<FocusedTextSelectionSnapshot, String> {
        let output = Command::new("osascript")
            .args(["-e", MACOS_FOCUSED_TEXT_SELECTION_SCRIPT])
            .output()
            .map_err(|error| format!("failed to run osascript: {}", error))?;

        if !output.status.success() {
            return Err(format!(
                "macOS Accessibility selection snapshot failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        parse_selection_snapshot_output(&output.stdout)
    }

    const MACOS_FOCUSED_TEXT_SCRIPT: &str = r#"
on attrText(elementRef, attrName)
  try
    set attrValue to value of attribute attrName of elementRef
    if attrValue is missing value then return ""
    return attrValue as text
  on error
    return ""
  end try
end attrText

tell application "System Events"
  set frontApp to first application process whose frontmost is true
  set focusedElement to value of attribute "AXFocusedUIElement" of frontApp

  set subroleValue to ""
  try
    set subroleRaw to value of attribute "AXSubrole" of focusedElement
    if subroleRaw is not missing value then set subroleValue to subroleRaw as text
  on error errMsg
    error "secure_check_error: " & errMsg
  end try
  if subroleValue is "AXSecureTextField" then return "__VERBATIM_SECURE__"

  set textValue to my attrText(focusedElement, "AXValue")

  set targetParts to {unix id of frontApp as text, my attrText(focusedElement, "AXRole"), my attrText(focusedElement, "AXSubrole"), my attrText(focusedElement, "AXIdentifier"), my attrText(focusedElement, "AXTitle")}
  set AppleScript's text item delimiters to "|"
  set targetId to targetParts as text
  set AppleScript's text item delimiters to ""
  return targetId & linefeed & textValue
end tell
"#;

    const MACOS_FOCUSED_TEXT_SELECTION_SCRIPT: &str = r#"
on attrText(elementRef, attrName)
  try
    set attrValue to value of attribute attrName of elementRef
    if attrValue is missing value then return ""
    return attrValue as text
  on error
    return ""
  end try
end attrText

tell application "System Events"
  set frontApp to first application process whose frontmost is true
  set focusedElement to value of attribute "AXFocusedUIElement" of frontApp
  set textValue to my attrText(focusedElement, "AXValue")
  set selectedText to my attrText(focusedElement, "AXSelectedText")

  set targetParts to {unix id of frontApp as text, my attrText(focusedElement, "AXRole"), my attrText(focusedElement, "AXSubrole"), my attrText(focusedElement, "AXIdentifier"), my attrText(focusedElement, "AXTitle")}
  set AppleScript's text item delimiters to "|"
  set targetId to targetParts as text
  set AppleScript's text item delimiters to ""
  return targetId & linefeed & "--VERBATIM-SELECTED-TEXT--" & linefeed & selectedText & linefeed & "--VERBATIM-FOCUSED-TEXT--" & linefeed & textValue
end tell
"#;
}

#[cfg(target_os = "linux")]
mod linux_focused_text {
    use super::{
        parse_selection_snapshot_output, parse_snapshot_output, Command,
        FocusedTextSelectionSnapshot, FocusedTextSnapshot,
    };

    pub fn capture() -> Result<FocusedTextSnapshot, String> {
        run_python("python3").or_else(|python3_error| {
            run_python("python").map_err(|python_error| {
                format!(
                    "Linux AT-SPI snapshot failed with python3 ({}) and python ({})",
                    python3_error, python_error
                )
            })
        })
    }

    fn run_python(binary: &str) -> Result<FocusedTextSnapshot, String> {
        let output = Command::new(binary)
            .args(["-c", LINUX_FOCUSED_TEXT_SCRIPT])
            .output()
            .map_err(|error| format!("failed to run {binary}: {error}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if let Some(skip) = super::classify_secure_sentinel("", &stderr) {
                return Err(skip.to_string());
            }
            return Err(format!(
                "{} AT-SPI snapshot failed: {}",
                binary,
                stderr.trim()
            ));
        }

        parse_snapshot_output(&output.stdout)
    }

    pub fn capture_with_selection() -> Result<FocusedTextSelectionSnapshot, String> {
        run_selection_python("python3").or_else(|python3_error| {
            run_selection_python("python").map_err(|python_error| {
                format!(
                    "Linux AT-SPI selection snapshot failed with python3 ({}) and python ({})",
                    python3_error, python_error
                )
            })
        })
    }

    fn run_selection_python(binary: &str) -> Result<FocusedTextSelectionSnapshot, String> {
        let output = Command::new(binary)
            .args(["-c", LINUX_FOCUSED_TEXT_SELECTION_SCRIPT])
            .output()
            .map_err(|error| format!("failed to run {binary}: {error}"))?;

        if !output.status.success() {
            return Err(format!(
                "{} AT-SPI selection snapshot failed: {}",
                binary,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        parse_selection_snapshot_output(&output.stdout)
    }

    const LINUX_FOCUSED_TEXT_SCRIPT: &str = r#"
import sys

try:
    import pyatspi
except Exception as exc:
    raise SystemExit(f"pyatspi is unavailable: {exc}")

def active_window():
    desktop = pyatspi.Registry.getDesktop(0)
    for app in desktop:
        for window in app:
            try:
                if window.getState().contains(pyatspi.STATE_ACTIVE):
                    return app, window
            except Exception:
                pass
    return None, None

def find_focused(root, path=""):
    try:
        if root.getState().contains(pyatspi.STATE_FOCUSED):
            return root, path
    except Exception:
        pass

    try:
        child_count = root.childCount
    except Exception:
        child_count = 0

    for index in range(child_count):
        try:
            child = root.getChildAtIndex(index)
        except Exception:
            continue
        found, found_path = find_focused(child, f"{path}/{index}")
        if found is not None:
            return found, found_path

    return None, ""

def accessible_text(obj):
    try:
        text = obj.queryText()
        return text.getText(0, text.characterCount)
    except Exception:
        return None

app, window = active_window()
if window is None:
    raise SystemExit("no active AT-SPI window")

focused, path = find_focused(window)
if focused is None:
    raise SystemExit("no focused AT-SPI element")

try:
    role = focused.getRoleName() if hasattr(focused, "getRoleName") else ""
except Exception as exc:
    raise SystemExit(f"__VERBATIM_SECURE_CHECK_ERROR__: {exc}")
if isinstance(role, str) and role.strip().lower() in ("password text", "password"):
    raise SystemExit("__VERBATIM_SECURE__")

text = accessible_text(focused)
if text is None:
    raise SystemExit("focused AT-SPI element has no readable text")

parts = [
    getattr(app, "name", "") or "",
    getattr(window, "name", "") or "",
    focused.getRoleName() if hasattr(focused, "getRoleName") else "",
    path,
]
print("|".join(parts))
print(text, end="")
"#;

    const LINUX_FOCUSED_TEXT_SELECTION_SCRIPT: &str = r#"
import sys

try:
    import pyatspi
except Exception as exc:
    raise SystemExit(f"pyatspi is unavailable: {exc}")

def active_window():
    desktop = pyatspi.Registry.getDesktop(0)
    for app in desktop:
        for window in app:
            try:
                if window.getState().contains(pyatspi.STATE_ACTIVE):
                    return app, window
            except Exception:
                pass
    return None, None

def find_focused(root, path=""):
    try:
        if root.getState().contains(pyatspi.STATE_FOCUSED):
            return root, path
    except Exception:
        pass

    try:
        child_count = root.childCount
    except Exception:
        child_count = 0

    for index in range(child_count):
        try:
            child = root.getChildAtIndex(index)
        except Exception:
            continue
        found, found_path = find_focused(child, f"{path}/{index}")
        if found is not None:
            return found, found_path

    return None, ""

def accessible_text(obj):
    try:
        text = obj.queryText()
        return text, text.getText(0, text.characterCount)
    except Exception:
        return None, None

def selected_text(text):
    try:
        count = text.getNSelections()
    except Exception:
        return ""

    selections = []
    for index in range(count):
        try:
            start, end = text.getSelection(index)
            if start != end:
                selections.append(text.getText(start, end))
        except Exception:
            pass
    return "".join(selections)

app, window = active_window()
if window is None:
    raise SystemExit("no active AT-SPI window")

focused, path = find_focused(window)
if focused is None:
    raise SystemExit("no focused AT-SPI element")

text_iface, text_value = accessible_text(focused)
if text_iface is None:
    raise SystemExit("focused AT-SPI element has no readable text")

parts = [
    getattr(app, "name", "") or "",
    getattr(window, "name", "") or "",
    focused.getRoleName() if hasattr(focused, "getRoleName") else "",
    path,
]
print("|".join(parts))
print("--VERBATIM-SELECTED-TEXT--")
print(selected_text(text_iface))
print("--VERBATIM-FOCUSED-TEXT--")
print(text_value, end="")
"#;
}

#[cfg(target_os = "windows")]
mod windows_focused_text {
    use super::{FocusedTextSelection, FocusedTextSelectionSnapshot, FocusedTextSnapshot};
    use std::ffi::c_void;
    use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED, SAFEARRAY,
    };
    use windows::Win32::System::Ole::{
        SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
        IUIAutomationValuePattern, UIA_TextPatternId, UIA_ValuePatternId,
    };

    const MAX_UIA_TEXT_CHARS: i32 = 20_000;

    pub fn capture() -> Result<FocusedTextSnapshot, String> {
        let _com = ComApartment::initialize()?;

        unsafe {
            let automation: IUIAutomation = CoCreateInstance(
                &CUIAutomation,
                None::<&windows::core::IUnknown>,
                CLSCTX_INPROC_SERVER,
            )
            .map_err(|error| format!("failed to create UI Automation client: {}", error))?;
            let element = automation
                .GetFocusedElement()
                .map_err(|error| format!("failed to get focused element: {}", error))?;

            // Skip protected fields BEFORE reading any text (fail closed on error).
            let is_password = element
                .CurrentIsPassword()
                .map(|value| value.as_bool())
                .unwrap_or(true);
            if is_password {
                return Err("skip: secure_field".to_string());
            }

            let text = read_element_text(&element)
                .ok_or_else(|| "focused element has no readable text pattern".to_string())?;

            Ok(FocusedTextSnapshot {
                target_id: strict_target_id(&element)?,
                text,
            })
        }
    }

    pub fn capture_with_selection() -> Result<FocusedTextSelectionSnapshot, String> {
        let _com = ComApartment::initialize()?;

        unsafe {
            let automation: IUIAutomation = CoCreateInstance(
                &CUIAutomation,
                None::<&windows::core::IUnknown>,
                CLSCTX_INPROC_SERVER,
            )
            .map_err(|error| format!("failed to create UI Automation client: {}", error))?;
            let element = automation
                .GetFocusedElement()
                .map_err(|error| format!("failed to get focused element: {}", error))?;
            let text = read_element_text(&element)
                .ok_or_else(|| "focused element has no readable text pattern".to_string())?;
            let selection = read_element_selection(&element);

            Ok(FocusedTextSelectionSnapshot {
                target_id: strict_target_id(&element)?,
                text,
                selection,
            })
        }
    }

    struct ComApartment {
        should_uninitialize: bool,
    }

    impl ComApartment {
        fn initialize() -> Result<Self, String> {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if hr == S_OK || hr == S_FALSE {
                return Ok(Self {
                    should_uninitialize: true,
                });
            }
            if hr == RPC_E_CHANGED_MODE {
                return Ok(Self {
                    should_uninitialize: false,
                });
            }

            Err(format!("failed to initialize COM: {:?}", hr))
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.should_uninitialize {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    unsafe fn read_element_text(element: &IUIAutomationElement) -> Option<String> {
        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        {
            if let Ok(range) = pattern.DocumentRange() {
                if let Ok(text) = range.GetText(MAX_UIA_TEXT_CHARS) {
                    return Some(text.to_string());
                }
            }
        }

        if let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        {
            if let Ok(text) = pattern.CurrentValue() {
                return Some(text.to_string());
            }
        }

        None
    }

    unsafe fn read_element_selection(element: &IUIAutomationElement) -> FocusedTextSelection {
        let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        else {
            return FocusedTextSelection::Unsupported(
                "focused element does not expose UI Automation TextPattern".to_string(),
            );
        };

        let ranges = match pattern.GetSelection() {
            Ok(ranges) => ranges,
            Err(error) => {
                return FocusedTextSelection::Unsupported(format!(
                    "failed to read selected text ranges: {}",
                    error
                ));
            }
        };
        let length = match ranges.Length() {
            Ok(length) => length,
            Err(error) => {
                return FocusedTextSelection::Unsupported(format!(
                    "failed to read selected text range count: {}",
                    error
                ));
            }
        };
        if length <= 0 {
            return FocusedTextSelection::Empty;
        }

        let mut parts = Vec::new();
        for index in 0..length {
            let range = match ranges.GetElement(index) {
                Ok(range) => range,
                Err(error) => {
                    return FocusedTextSelection::Unsupported(format!(
                        "failed to read selected text range: {}",
                        error
                    ));
                }
            };
            let text = match range.GetText(MAX_UIA_TEXT_CHARS) {
                Ok(text) => text.to_string(),
                Err(error) => {
                    return FocusedTextSelection::Unsupported(format!(
                        "failed to read selected text: {}",
                        error
                    ));
                }
            };
            if !text.trim().is_empty() {
                parts.push(text);
            }
        }

        if parts.is_empty() {
            FocusedTextSelection::Empty
        } else {
            FocusedTextSelection::Selected(parts.join(""))
        }
    }

    unsafe fn target_id(element: &IUIAutomationElement) -> String {
        let process_id = element.CurrentProcessId().unwrap_or_default();
        let hwnd = element
            .CurrentNativeWindowHandle()
            .map(|hwnd| format!("{:?}", hwnd))
            .unwrap_or_default();
        let class_name = element
            .CurrentClassName()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let automation_id = element
            .CurrentAutomationId()
            .map(|value| value.to_string())
            .unwrap_or_default();

        format!("{process_id}|{hwnd}|{class_name}|{automation_id}")
    }

    unsafe fn strict_target_id(element: &IUIAutomationElement) -> Result<String, String> {
        let runtime_id = runtime_id(element)?;
        Ok(format!("{}|runtime:{runtime_id}", target_id(element)))
    }

    unsafe fn runtime_id(element: &IUIAutomationElement) -> Result<String, String> {
        let safe_array = element
            .GetRuntimeId()
            .map_err(|error| format!("failed to read focused element runtime id: {}", error))?;
        if safe_array.is_null() {
            return Err("focused element runtime id is empty".to_string());
        }

        let _guard = SafeArrayGuard(safe_array);
        let lower_bound = SafeArrayGetLBound(safe_array, 1)
            .map_err(|error| format!("failed to read runtime id lower bound: {}", error))?;
        let upper_bound = SafeArrayGetUBound(safe_array, 1)
            .map_err(|error| format!("failed to read runtime id upper bound: {}", error))?;
        if upper_bound < lower_bound {
            return Err("focused element runtime id range is empty".to_string());
        }

        let mut values = Vec::new();
        for index in lower_bound..=upper_bound {
            let mut value = 0_i32;
            SafeArrayGetElement(safe_array, &index, &mut value as *mut i32 as *mut c_void)
                .map_err(|error| format!("failed to read runtime id element: {}", error))?;
            values.push(value.to_string());
        }

        if values.is_empty() {
            Err("focused element runtime id is empty".to_string())
        } else {
            Ok(values.join("."))
        }
    }

    struct SafeArrayGuard(*mut SAFEARRAY);

    impl Drop for SafeArrayGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = SafeArrayDestroy(self.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        auto_learn_outcome_log_message, extract_corrected_inserted_text,
        focused_text_snapshot_from_parts, focused_text_within_cap, parse_selection_snapshot_output,
        FocusedTextSelection, MAX_FOCUSED_TEXT_CHARS, POST_PASTE_LEARNING_WINDOW,
    };
    use std::time::Duration;

    #[test]
    fn extracts_corrected_text_from_inside_pasted_span() {
        let corrected = extract_corrected_inserted_text(
            "prefix suffix",
            "prefix meet with robin tomorrow suffix",
            "prefix meet with Robyn tomorrow suffix",
        );

        assert_eq!(corrected.as_deref(), Some("meet with Robyn tomorrow"));
    }

    #[test]
    fn parses_selection_snapshot_output_with_selected_text() {
        let output = b"app|window|text|/0\n--VERBATIM-SELECTED-TEXT--\nselected line\n--VERBATIM-FOCUSED-TEXT--\nfocused text value";

        let snapshot =
            parse_selection_snapshot_output(output).expect("selection snapshot should parse");

        assert_eq!(snapshot.target_id, "app|window|text|/0");
        assert_eq!(snapshot.text, "focused text value");
        assert_eq!(
            snapshot.selection,
            FocusedTextSelection::Selected("selected line".to_string())
        );
    }

    #[test]
    fn parses_selection_snapshot_output_with_empty_selection() {
        let output = b"app|window|text|/0\n--VERBATIM-SELECTED-TEXT--\n\n--VERBATIM-FOCUSED-TEXT--\nfocused text value";

        let snapshot =
            parse_selection_snapshot_output(output).expect("selection snapshot should parse");

        assert_eq!(snapshot.selection, FocusedTextSelection::Empty);
    }

    #[test]
    fn rejects_malformed_selection_snapshot_output() {
        let err = parse_selection_snapshot_output(b"app|window\nfocused text")
            .expect_err("missing separators should fail");

        assert!(err.contains("malformed output"));
    }

    #[test]
    fn extracts_sentence_punctuation_correction_from_pasted_span() {
        let corrected = extract_corrected_inserted_text(
            "prefix suffix",
            "prefix meet robin. suffix",
            "prefix meet Robyn. suffix",
        );

        assert_eq!(corrected.as_deref(), Some("meet Robyn."));
    }

    #[test]
    fn extracts_corrected_span_when_user_deletes_inserted_words() {
        let corrected = extract_corrected_inserted_text(
            "prefix suffix",
            "prefix meet with robin tomorrow suffix",
            "prefix meet Robyn suffix",
        );

        assert_eq!(corrected.as_deref(), Some("meet Robyn"));
    }

    #[test]
    fn accepts_empty_focused_text_before_paste() {
        let snapshot = focused_text_snapshot_from_parts("target".to_string(), String::new())
            .expect("empty focused fields are valid paste targets");

        assert_eq!(snapshot.target_id, "target");
        assert_eq!(snapshot.text, "");
    }

    #[test]
    fn extracts_correction_when_paste_started_in_empty_field() {
        let corrected = extract_corrected_inserted_text(
            "",
            "meet with robin tomorrow",
            "meet with Robyn tomorrow",
        );

        assert_eq!(corrected.as_deref(), Some("meet with Robyn tomorrow"));
    }

    #[test]
    fn strips_direction_marks_from_corrected_pasted_span() {
        let corrected = extract_corrected_inserted_text(
            "",
            "\u{200E}Dear James,\u{200E}",
            "\u{200E}Dear Jaymes,\u{200E}",
        );

        assert_eq!(corrected.as_deref(), Some("Dear Jaymes,"));
    }

    #[test]
    fn ignores_edits_outside_pasted_span() {
        let corrected = extract_corrected_inserted_text(
            "prefix suffix",
            "prefix meet with robin tomorrow suffix",
            "changed meet with Robyn tomorrow suffix",
        );

        assert!(corrected.is_none());
    }

    #[test]
    fn ignores_snapshots_that_are_not_pure_insertions() {
        let corrected = extract_corrected_inserted_text(
            "prefix suffix",
            "prefix changed",
            "prefix changed again",
        );

        assert!(corrected.is_none());
    }

    #[test]
    fn correction_window_allows_human_edit_latency() {
        assert!(POST_PASTE_LEARNING_WINDOW >= Duration::from_secs(6));
    }

    #[test]
    fn auto_learn_outcome_log_message_is_phrase_free() {
        let message = auto_learn_outcome_log_message(1, 2, 0);

        assert!(message.contains("1 promoted"));
        assert!(message.contains("2 new candidates"));
        assert!(!message.contains("Robyn"));
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn parses_script_snapshot_output() {
        let snapshot =
            super::parse_snapshot_output(b"app|window|role|/0\nhello world\n").expect("snapshot");

        assert_eq!(snapshot.target_id, "app|window|role|/0");
        assert_eq!(snapshot.text, "hello world");
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn rejects_script_snapshot_without_target_line() {
        let error = super::parse_snapshot_output(b"hello world")
            .expect_err("malformed snapshot should fail");

        assert!(error.contains("malformed"));
    }

    #[test]
    fn oversized_focused_text_is_rejected_before_diff() {
        let big = "a".repeat(MAX_FOCUSED_TEXT_CHARS + 1);
        assert!(!focused_text_within_cap(&big));
        assert!(focused_text_within_cap("short"));
        let exact = "a".repeat(MAX_FOCUSED_TEXT_CHARS);
        assert!(focused_text_within_cap(&exact));
    }

    #[test]
    fn classifies_secure_sentinels() {
        assert_eq!(
            super::classify_secure_sentinel("__VERBATIM_SECURE__", ""),
            Some("skip: secure_field")
        );
        assert_eq!(
            super::classify_secure_sentinel(" __VERBATIM_SECURE__ \n", ""),
            Some("skip: secure_field")
        );
        assert_eq!(
            super::classify_secure_sentinel("", "__VERBATIM_SECURE__"),
            Some("skip: secure_field")
        );
        assert_eq!(
            super::classify_secure_sentinel(
                "",
                "Traceback...__VERBATIM_SECURE_CHECK_ERROR__: boom"
            ),
            Some("skip: secure_check_error")
        );
        assert_eq!(
            super::classify_secure_sentinel("app|window\ntext", ""),
            None
        );
    }
}
