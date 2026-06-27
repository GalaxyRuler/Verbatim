//! Foreground integrity-level watcher (Windows only).
//!
//! Verbatim runs at Medium integrity. When a higher-integrity window — i.e. an
//! app launched "as administrator" — is in the foreground, Windows UIPI blocks
//! a Medium-integrity process from observing keyboard input directed at it. That
//! kills BOTH the low-level keyboard hook AND `GetAsyncKeyState`, so the
//! dictation shortcut silently does nothing while such an app is focused
//! (`RegisterHotKey` is the only path that crosses UIPI, and it can't represent
//! modifier-only chords).
//!
//! The failure is otherwise completely silent — no pill, no error — which is
//! very hard to diagnose. This watcher polls the foreground window's integrity
//! level and, when it exceeds our own, shows a native notification explaining
//! why dictation won't work there. It warns at most once per process per
//! session to avoid noise.

#[cfg(target_os = "windows")]
pub fn spawn(app: tauri::AppHandle) {
    std::thread::spawn(move || imp::run(app));
}

#[cfg(not(target_os = "windows"))]
pub fn spawn(_app: tauri::AppHandle) {
    // UIPI integrity isolation is Windows-specific; nothing to do elsewhere.
}

#[cfg(target_os = "windows")]
mod imp {
    use std::collections::HashSet;
    use std::time::Duration;

    use tauri::{AppHandle, Emitter};

    use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
    use windows::Win32::Security::{
        GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
        TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    /// Emitted to the frontend when a higher-integrity foreground window is
    /// detected. The frontend localizes and shows the native notification so the
    /// message follows the user's language (see `notifications.elevatedWindow.*`).
    #[derive(Clone, serde::Serialize)]
    struct DictationBlockedEvent {
        app_name: String,
    }

    const POLL_INTERVAL: Duration = Duration::from_secs(2);
    // Bound the dedup set so a session that churns through many elevated PIDs
    // can't grow it without limit.
    const MAX_TRACKED_PIDS: usize = 256;

    pub fn run(app: AppHandle) {
        let self_il = match current_process_integrity_level() {
            Some(level) => level,
            None => {
                log::warn!("elevation_watch: could not read own integrity level; watcher disabled");
                return;
            }
        };
        let self_pid = std::process::id();
        let mut warned: HashSet<u32> = HashSet::new();

        loop {
            std::thread::sleep(POLL_INTERVAL);

            let Some(pid) = foreground_pid() else {
                continue;
            };
            if pid == 0 || pid == self_pid || warned.contains(&pid) {
                continue;
            }

            let Some(target_il) = process_integrity_level(pid) else {
                continue;
            };
            if target_il <= self_il {
                continue;
            }

            // Higher-integrity foreground window: keyboard input is isolated from
            // us by UIPI, so the dictation shortcut can't fire here.
            //
            // Honor the user setting, read live so the toggle takes effect without
            // a restart. Skip without marking the pid so re-enabling can still warn.
            if !crate::settings::get_settings(&app).warn_on_elevated_target {
                continue;
            }

            warned.insert(pid);
            if warned.len() > MAX_TRACKED_PIDS {
                warned.clear();
                warned.insert(pid);
            }

            let app_name = process_display_name(pid).unwrap_or_else(|| "that app".to_string());
            log::info!(
                "elevation_watch: foreground app '{app_name}' (pid {pid}) runs at a higher \
                 integrity level than Verbatim; dictation input is blocked by Windows UIPI"
            );

            // The frontend localizes and shows the native notification.
            let _ = app.emit(
                "dictation-blocked-elevated",
                DictationBlockedEvent { app_name },
            );
        }
    }

    fn foreground_pid() -> Option<u32> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd == HWND::default() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            Some(pid)
        }
    }

    /// Integrity-level RID of our own process (e.g. 0x2000 = Medium, 0x3000 = High).
    fn current_process_integrity_level() -> Option<u32> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).ok()?;
            let level = token_integrity_level(token);
            let _ = CloseHandle(token);
            level
        }
    }

    /// Integrity-level RID of another process. Querying the integrity label of a
    /// higher-integrity process is permitted with limited query rights, so this
    /// works against elevated targets from our Medium-integrity process.
    fn process_integrity_level(pid: u32) -> Option<u32> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut token = HANDLE::default();
            let level = if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_ok() {
                let l = token_integrity_level(token);
                let _ = CloseHandle(token);
                l
            } else {
                None
            };
            let _ = CloseHandle(process);
            level
        }
    }

    unsafe fn token_integrity_level(token: HANDLE) -> Option<u32> {
        // First call sizes the buffer (returns an error we intentionally ignore).
        let mut needed = 0u32;
        let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut needed);
        if needed == 0 {
            return None;
        }
        let mut buffer = vec![0u8; needed as usize];
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buffer.as_mut_ptr() as *mut _),
            needed,
            &mut needed,
        )
        .ok()?;

        let label = &*(buffer.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let sid = label.Label.Sid;
        let count_ptr = GetSidSubAuthorityCount(sid);
        if count_ptr.is_null() {
            return None;
        }
        let count = *count_ptr;
        if count == 0 {
            return None;
        }
        let rid_ptr = GetSidSubAuthority(sid, (count - 1) as u32);
        if rid_ptr.is_null() {
            return None;
        }
        Some(*rid_ptr)
    }

    /// Foreground process file name, with a trailing `.exe` trimmed for display.
    fn process_display_name(pid: u32) -> Option<String> {
        use std::path::Path;
        use windows::core::PWSTR;

        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = vec![0u16; 32768];
            let mut size = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(process);
            if result.is_err() || size == 0 {
                return None;
            }
            let full_path = String::from_utf16_lossy(&buffer[..size as usize]);
            let file_name = Path::new(&full_path).file_name()?.to_str()?;
            let trimmed = file_name
                .strip_suffix(".exe")
                .or_else(|| file_name.strip_suffix(".EXE"))
                .unwrap_or(file_name);
            Some(trimmed.to_string())
        }
    }
}
