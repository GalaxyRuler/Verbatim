use log::{debug, info};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const POST_PASTE_SETTLE_DELAY: Duration = Duration::from_millis(120);
const POST_PASTE_POLL_INTERVAL: Duration = Duration::from_millis(150);
const POST_PASTE_STABLE_EDIT_DELAY: Duration = Duration::from_millis(300);
const POST_PASTE_LEARNING_WINDOW: Duration = Duration::from_secs(6);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FocusedTextSnapshot {
    pub target_id: String,
    pub text: String,
}

pub fn maybe_spawn_auto_add_watcher(
    app: AppHandle,
    inserted_text: String,
    before_paste: Option<FocusedTextSnapshot>,
) {
    let Some(before_paste) = before_paste else {
        return;
    };

    if inserted_text.trim().is_empty() {
        return;
    }

    std::thread::spawn(move || {
        if let Err(error) = watch_for_post_paste_correction(app, inserted_text, before_paste) {
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

fn watch_for_post_paste_correction(
    app: AppHandle,
    inserted_text: String,
    before_paste: FocusedTextSnapshot,
) -> Result<(), String> {
    std::thread::sleep(POST_PASTE_SETTLE_DELAY);

    let after_paste = capture_matching_snapshot(&before_paste.target_id)?
        .ok_or_else(|| "focused text target changed after paste".to_string())?;

    if after_paste.text == before_paste.text {
        return Err("post-paste text snapshot did not change".to_string());
    }

    let deadline = Instant::now() + POST_PASTE_LEARNING_WINDOW;
    let mut last_seen_text = after_paste.text.clone();
    let mut changed_text: Option<String> = None;
    let mut stable_since = Instant::now();

    while Instant::now() < deadline {
        std::thread::sleep(POST_PASTE_POLL_INTERVAL);

        let Some(current) = capture_matching_snapshot(&before_paste.target_id)? else {
            return Err("focused text target changed during learning window".to_string());
        };

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
    inserted_text: &str,
    before_paste_text: &str,
    after_paste_text: &str,
    after_edit_text: &str,
) -> Result<(), String> {
    let Some(corrected_text) =
        extract_corrected_inserted_text(before_paste_text, after_paste_text, after_edit_text)
    else {
        return Ok(());
    };

    let mut settings = crate::settings::get_settings(app);
    let candidates = crate::dictionary_learning::infer_auto_learn_candidates(
        inserted_text,
        &corrected_text,
        &settings.custom_words,
    );

    if candidates.is_empty() {
        return Ok(());
    }

    let mut learned_entries = Vec::new();
    let now_ms = crate::dictionary::current_unix_ms();
    for candidate in candidates {
        if let Some(entry) = crate::dictionary::upsert_auto_learn_entry(
            &mut settings,
            now_ms,
            candidate.phrase,
            candidate.replacement_of,
        )? {
            learned_entries.push(entry);
        }
    }

    if learned_entries.is_empty() {
        return Ok(());
    }

    crate::settings::write_settings(app, settings);
    let learned_words = learned_entries
        .iter()
        .map(|entry| entry.phrase.clone())
        .collect::<Vec<_>>();
    let _ = app.emit("dictionary-entries-learned", learned_entries.clone());
    let _ = app.emit("custom-words-learned", learned_words.clone());
    info!(
        "Auto-added {} corrected custom word(s): {}",
        learned_words.len(),
        learned_words.join(", ")
    );

    Ok(())
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
    let corrected = corrected.trim();
    if corrected.is_empty() {
        return None;
    }

    Some(corrected.to_string())
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

#[cfg(target_os = "macos")]
fn capture_platform_focused_text_snapshot() -> Result<FocusedTextSnapshot, String> {
    macos_focused_text::capture()
}

#[cfg(target_os = "linux")]
fn capture_platform_focused_text_snapshot() -> Result<FocusedTextSnapshot, String> {
    linux_focused_text::capture()
}

#[cfg(target_os = "windows")]
fn capture_platform_focused_text_snapshot() -> Result<FocusedTextSnapshot, String> {
    windows_focused_text::capture()
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

#[cfg(target_os = "macos")]
mod macos_focused_text {
    use super::{parse_snapshot_output, Command, FocusedTextSnapshot};

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

        parse_snapshot_output(&output.stdout)
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
  set textValue to my attrText(focusedElement, "AXValue")

  set targetParts to {unix id of frontApp as text, my attrText(focusedElement, "AXRole"), my attrText(focusedElement, "AXSubrole"), my attrText(focusedElement, "AXIdentifier"), my attrText(focusedElement, "AXTitle")}
  set AppleScript's text item delimiters to "|"
  set targetId to targetParts as text
  set AppleScript's text item delimiters to ""
  return targetId & linefeed & textValue
end tell
"#;
}

#[cfg(target_os = "linux")]
mod linux_focused_text {
    use super::{parse_snapshot_output, Command, FocusedTextSnapshot};

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
            return Err(format!(
                "{} AT-SPI snapshot failed: {}",
                binary,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        parse_snapshot_output(&output.stdout)
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
}

#[cfg(target_os = "windows")]
mod windows_focused_text {
    use super::FocusedTextSnapshot;
    use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, S_FALSE, S_OK};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
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
            let text = read_element_text(&element)
                .ok_or_else(|| "focused element has no readable text pattern".to_string())?;

            Ok(FocusedTextSnapshot {
                target_id: target_id(&element),
                text,
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
}

#[cfg(test)]
mod tests {
    use super::{
        extract_corrected_inserted_text, focused_text_snapshot_from_parts,
        POST_PASTE_LEARNING_WINDOW,
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
}
