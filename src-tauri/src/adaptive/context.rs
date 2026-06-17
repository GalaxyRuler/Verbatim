use crate::adaptive::types::{CapturedContext, TargetKind};
use chrono::Utc;
use sha2::{Digest, Sha256};

#[derive(serde::Serialize)]
struct PersistedContextMetadata {
    captured_at_ms: i64,
    process_name: Option<String>,
    title_hash: Option<String>,
    window_class: Option<String>,
    target_kind: TargetKind,
    target_fingerprint: Option<String>,
    is_sensitive: bool,
}

pub fn capture_context(private_patterns: &[String]) -> CapturedContext {
    let context = capture_platform_context();
    sanitize_context(context, private_patterns)
}

pub fn unknown_context() -> CapturedContext {
    CapturedContext {
        captured_at_ms: Utc::now().timestamp_millis(),
        process_name: None,
        window_title: None,
        window_title_hash: None,
        window_class: None,
        target_kind: TargetKind::Unknown,
        target_fingerprint: None,
        is_sensitive: false,
    }
}

pub fn context_history_metadata_json(context: &CapturedContext) -> Option<String> {
    if context.process_name.is_none()
        && context.window_class.is_none()
        && context.target_fingerprint.is_none()
        && context.target_kind == TargetKind::Unknown
        && !context.is_sensitive
    {
        return None;
    }

    let metadata = PersistedContextMetadata {
        captured_at_ms: context.captured_at_ms,
        process_name: context.process_name.clone(),
        title_hash: context.window_title_hash.clone(),
        window_class: context.window_class.clone(),
        target_kind: context.target_kind.clone(),
        target_fingerprint: context.target_fingerprint.clone(),
        is_sensitive: context.is_sensitive,
    };

    serde_json::to_string(&metadata).ok()
}

pub fn redact_context_json_for_history(context_json: &str) -> Option<String> {
    let context = serde_json::from_str::<CapturedContext>(context_json).ok()?;
    context_history_metadata_json(&context)
}

pub fn classify_target(
    process_name: Option<&str>,
    window_title: Option<&str>,
    window_class: Option<&str>,
) -> TargetKind {
    let process = process_name.unwrap_or_default().to_lowercase();
    let title = window_title.unwrap_or_default().to_lowercase();
    let class = window_class.unwrap_or_default().to_lowercase();
    let is_new_outlook_host = matches!(process.as_str(), "olk.exe" | "olkbg.exe")
        || (process.contains("msedgewebview2") && title.contains("outlook"));

    if process.contains("outlook")
        || is_new_outlook_host
        || title.contains("outlook")
        || title.contains("gmail")
        || title.contains("mail")
        || title.contains("message")
        || class.contains("rctrl_renwnd32")
    {
        TargetKind::Email
    } else if process.contains("whatsapp")
        || process.contains("telegram")
        || title.contains("whatsapp")
        || title.contains("telegram")
    {
        TargetKind::CasualMessage
    } else if process.contains("code")
        || process.contains("terminal")
        || process.contains("powershell")
        || process.contains("cmd")
        || title.contains(".rs")
        || title.contains(".tsx")
        || title.contains(".ts")
    {
        TargetKind::Technical
    } else if process.contains("obsidian") || process.contains("notepad") || title.ends_with(".md")
    {
        TargetKind::Notes
    } else if process.contains("chrome")
        || process.contains("msedge")
        || process.contains("firefox")
    {
        TargetKind::BrowserPrompt
    } else {
        TargetKind::Unknown
    }
}

pub fn sanitize_context(
    mut context: CapturedContext,
    private_patterns: &[String],
) -> CapturedContext {
    let process = context
        .process_name
        .clone()
        .unwrap_or_default()
        .to_lowercase();
    let title = context.window_title.clone().unwrap_or_default();
    let title_lower = title.to_lowercase();
    let is_sensitive = private_patterns
        .iter()
        .map(|pattern| pattern.to_lowercase())
        .any(|pattern| process.contains(&pattern) || title_lower.contains(&pattern));

    if is_sensitive {
        context.is_sensitive = true;
        context.window_title_hash = Some(hash_text(&title));
        context.window_title = None;
    }

    context
}

fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(not(target_os = "windows"))]
fn capture_platform_context() -> CapturedContext {
    CapturedContext {
        captured_at_ms: Utc::now().timestamp_millis(),
        process_name: None,
        window_title: None,
        window_title_hash: None,
        window_class: None,
        target_kind: TargetKind::Unknown,
        target_fingerprint: None,
        is_sensitive: false,
    }
}

#[cfg(target_os = "windows")]
fn capture_platform_context() -> CapturedContext {
    use std::path::Path;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    unsafe {
        let hwnd = GetForegroundWindow();
        let mut process_id = 0u32;
        if hwnd != HWND::default() {
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        }

        let window_title = read_window_title(hwnd);
        let window_class = read_window_class(hwnd);
        let process_name = if process_id == 0 {
            None
        } else {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok();
            handle.and_then(|handle| {
                let mut buffer = vec![0u16; 32768];
                let mut size = buffer.len() as u32;
                let result = QueryFullProcessImageNameW(
                    handle,
                    PROCESS_NAME_WIN32,
                    PWSTR(buffer.as_mut_ptr()),
                    &mut size,
                );
                let _ = CloseHandle(handle);
                if result.is_err() || size == 0 {
                    return None;
                }
                let full_path = String::from_utf16_lossy(&buffer[..size as usize]);
                Path::new(&full_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.to_string())
            })
        };
        let target_kind = classify_target(
            process_name.as_deref(),
            window_title.as_deref(),
            window_class.as_deref(),
        );
        let target_fingerprint =
            build_target_fingerprint(process_name.as_deref(), window_class.as_deref());

        CapturedContext {
            captured_at_ms: Utc::now().timestamp_millis(),
            process_name,
            window_title,
            window_title_hash: None,
            window_class,
            target_kind,
            target_fingerprint,
            is_sensitive: false,
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn read_window_title(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    if hwnd == HWND::default() {
        return None;
    }
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize + 1];
    let copied = GetWindowTextW(hwnd, &mut buffer);
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

#[cfg(target_os = "windows")]
unsafe fn read_window_class(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;

    if hwnd == HWND::default() {
        return None;
    }
    let mut buffer = vec![0u16; 256];
    let copied = GetClassNameW(hwnd, &mut buffer);
    if copied <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..copied as usize]))
}

#[cfg(target_os = "windows")]
fn build_target_fingerprint(
    process_name: Option<&str>,
    window_class: Option<&str>,
) -> Option<String> {
    let process = process_name.unwrap_or_default().to_lowercase();
    let class = window_class.unwrap_or_default().to_lowercase();
    if process.is_empty() && class.is_empty() {
        None
    } else {
        Some(format!("{}|{}", process, class))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_outlook_as_email() {
        let kind = classify_target(
            Some("OUTLOOK.EXE"),
            Some("RE: Contract terms - Message"),
            Some("rctrl_renwnd32"),
        );
        assert_eq!(kind, TargetKind::Email);
    }

    #[test]
    fn classifies_new_outlook_hosts_as_email() {
        assert_eq!(
            classify_target(
                Some("olk.exe"),
                Some("Inbox - abdullah.alkulaib@pmbsr.gov.sa - Outlook"),
                Some("Chrome_WidgetWin_1")
            ),
            TargetKind::Email
        );
        assert_eq!(
            classify_target(
                Some("msedgewebview2.exe"),
                Some("Inbox - abdullah.alkulaib@pmbsr.gov.sa - Outlook"),
                Some("Chrome_WidgetWin_1")
            ),
            TargetKind::Email
        );
    }

    #[test]
    fn classifies_whatsapp_as_casual_message() {
        let kind = classify_target(
            Some("WhatsApp.exe"),
            Some("WhatsApp"),
            Some("Chrome_WidgetWin_1"),
        );
        assert_eq!(kind, TargetKind::CasualMessage);
    }

    #[test]
    fn classifies_telegram_as_casual_message() {
        let kind = classify_target(
            Some("Telegram.exe"),
            Some("Telegram"),
            Some("Qt5152QWindowIcon"),
        );
        assert_eq!(kind, TargetKind::CasualMessage);
    }

    #[test]
    fn classifies_vscode_and_terminal_as_technical() {
        assert_eq!(
            classify_target(Some("Code.exe"), Some("actions.rs - Verbatim"), None),
            TargetKind::Technical
        );
        assert_eq!(
            classify_target(Some("WindowsTerminal.exe"), Some("PowerShell"), None),
            TargetKind::Technical
        );
    }

    #[test]
    fn classifies_obsidian_and_notepad_as_notes() {
        assert_eq!(
            classify_target(Some("Obsidian.exe"), Some("adaptive-dictation.md"), None),
            TargetKind::Notes
        );
        assert_eq!(
            classify_target(Some("notepad.exe"), Some("Untitled - Notepad"), None),
            TargetKind::Notes
        );
    }

    #[test]
    fn sensitive_process_suppresses_title() {
        let context = sanitize_context(
            CapturedContext {
                captured_at_ms: 1,
                process_name: Some("Bitwarden.exe".to_string()),
                window_title: Some("Personal Vault".to_string()),
                window_title_hash: None,
                window_class: Some("Chrome_WidgetWin_1".to_string()),
                target_kind: TargetKind::Unknown,
                target_fingerprint: Some("bitwarden".to_string()),
                is_sensitive: false,
            },
            &["bitwarden".to_string()],
        );

        assert!(context.is_sensitive);
        assert!(context.window_title.is_none());
        assert!(context.window_title_hash.is_some());
    }

    #[test]
    fn history_metadata_omits_window_title() {
        let context = CapturedContext {
            captured_at_ms: 1,
            process_name: Some("OUTLOOK.EXE".to_string()),
            window_title: Some("Inbox - private@example.com - Outlook".to_string()),
            window_title_hash: None,
            window_class: Some("rctrl_renwnd32".to_string()),
            target_kind: TargetKind::Email,
            target_fingerprint: Some("outlook.exe|rctrl_renwnd32".to_string()),
            is_sensitive: false,
        };

        let metadata = context_history_metadata_json(&context).expect("metadata json");

        assert!(metadata.contains("\"target_kind\":\"Email\""));
        assert!(metadata.contains("\"process_name\":\"OUTLOOK.EXE\""));
        assert!(!metadata.contains("private@example.com"));
        assert!(!metadata.contains("window_title"));
    }

    #[test]
    fn unknown_context_has_no_history_metadata() {
        let context = unknown_context();

        assert!(context_history_metadata_json(&context).is_none());
    }

    #[test]
    fn legacy_context_json_is_redacted_for_history() {
        let legacy_json = serde_json::json!({
            "captured_at_ms": 1,
            "process_name": "OUTLOOK.EXE",
            "window_title": "Inbox - private@example.com - Outlook",
            "window_title_hash": null,
            "window_class": "rctrl_renwnd32",
            "target_kind": "Email",
            "target_fingerprint": "outlook.exe|rctrl_renwnd32",
            "is_sensitive": false
        })
        .to_string();

        let redacted = redact_context_json_for_history(&legacy_json).expect("redacted");

        assert!(redacted.contains("\"target_kind\":\"Email\""));
        assert!(!redacted.contains("private@example.com"));
        assert!(!redacted.contains("window_title"));
    }

    #[test]
    fn invalid_legacy_context_json_is_not_retained() {
        assert!(redact_context_json_for_history("{not json").is_none());
    }
}
