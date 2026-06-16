use crate::adaptive::types::{InsertionMethod, InsertionReceipt};
use crate::input::{self, EnigoState};
#[cfg(target_os = "linux")]
use crate::settings::TypingTool;
use crate::settings::{get_settings, AutoSubmitKey, ClipboardHandling, PasteMethod};
use enigo::{Direction, Enigo, Key, Keyboard};
use log::{info, warn};
use std::process::Command;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[cfg(target_os = "linux")]
use crate::utils::{is_kde_wayland, is_wayland};

enum ClipboardSnapshot {
    Native {
        native: NativeClipboardSnapshot,
        fallback_text: Option<String>,
    },
    TextFallback(Option<String>),
}

impl ClipboardSnapshot {
    fn capture(app_handle: &AppHandle) -> Self {
        let fallback_text = app_handle
            .clipboard()
            .read_text()
            .ok()
            .filter(|text| !text.is_empty());

        match NativeClipboardSnapshot::capture() {
            Ok(native) => Self::Native {
                native,
                fallback_text,
            },
            Err(_) => Self::TextFallback(fallback_text),
        }
    }

    fn restore(&self, app_handle: &AppHandle) {
        match self {
            Self::Native {
                native,
                fallback_text,
            } => {
                if let Err(err) = native.restore() {
                    warn!("Failed to restore native clipboard snapshot: {}", err);
                    if let Some(text) = fallback_text {
                        restore_text_clipboard(app_handle, text);
                    }
                }
            }
            Self::TextFallback(Some(text)) => restore_text_clipboard(app_handle, text),
            Self::TextFallback(None) => {}
        }
    }

    #[cfg(test)]
    fn fallback_text_for_restore(&self) -> Option<&str> {
        match self {
            Self::Native { fallback_text, .. } => fallback_text.as_deref(),
            Self::TextFallback(text) => text.as_deref(),
        }
    }

    #[cfg(test)]
    fn native_for_test() -> Self {
        Self::Native {
            native: NativeClipboardSnapshot::for_test(),
            fallback_text: None,
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
struct NativeClipboardSnapshot {
    formats: Vec<ClipboardFormatData>,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
struct ClipboardFormatData {
    format: u32,
    bytes: Vec<u8>,
}

#[cfg(target_os = "windows")]
trait NativeClipboardRestoreOps {
    type Handle: Copy;

    fn allocate_format(&mut self, format: &ClipboardFormatData) -> Result<Self::Handle, String>;
    fn open_and_empty(&mut self) -> Result<(), String>;
    fn set_format(&mut self, format: u32, handle: Self::Handle) -> Result<(), String>;
    fn free_handle(&mut self, handle: Self::Handle);
}

#[cfg(target_os = "windows")]
fn restore_native_clipboard_snapshot_with_ops<O: NativeClipboardRestoreOps>(
    formats: &[ClipboardFormatData],
    ops: &mut O,
) -> Result<(), String> {
    if formats.is_empty() {
        return Err("native clipboard snapshot is empty".to_string());
    }

    let mut prepared = Vec::with_capacity(formats.len());
    for format in formats {
        match ops.allocate_format(format) {
            Ok(handle) => prepared.push((format.format, handle)),
            Err(err) => {
                for (_, handle) in prepared {
                    ops.free_handle(handle);
                }
                return Err(format!(
                    "prepare clipboard format {} before clearing clipboard: {}",
                    format.format, err
                ));
            }
        }
    }

    if let Err(err) = ops.open_and_empty() {
        for (_, handle) in prepared {
            ops.free_handle(handle);
        }
        return Err(err);
    }

    let format_count = prepared.len();
    let mut failures = Vec::new();
    for (format, handle) in prepared {
        if let Err(err) = ops.set_format(format, handle) {
            ops.free_handle(handle);
            failures.push(format!("format {format}: {err}"));
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "set clipboard data failed for {} of {} formats: {}",
            failures.len(),
            format_count,
            failures.join("; ")
        ))
    }
}

#[cfg(target_os = "windows")]
struct WinClipboardRestoreOps {
    opened: bool,
}

#[cfg(target_os = "windows")]
impl Drop for WinClipboardRestoreOps {
    fn drop(&mut self) {
        if self.opened {
            unsafe {
                let _ = windows::Win32::System::DataExchange::CloseClipboard();
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl NativeClipboardRestoreOps for WinClipboardRestoreOps {
    type Handle = windows::Win32::Foundation::HGLOBAL;

    fn allocate_format(&mut self, format: &ClipboardFormatData) -> Result<Self::Handle, String> {
        use windows::Win32::Foundation::GlobalFree;
        use windows::Win32::System::Memory::{
            GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
        };

        unsafe {
            let hglobal = GlobalAlloc(GMEM_MOVEABLE, format.bytes.len())
                .map_err(|err| format!("allocate clipboard memory: {}", err))?;
            let locked = GlobalLock(hglobal);
            if locked.is_null() {
                let _ = GlobalFree(Some(hglobal));
                return Err("lock clipboard memory".to_string());
            }

            std::ptr::copy_nonoverlapping(
                format.bytes.as_ptr(),
                locked.cast::<u8>(),
                format.bytes.len(),
            );
            let _ = GlobalUnlock(hglobal);
            Ok(hglobal)
        }
    }

    fn open_and_empty(&mut self) -> Result<(), String> {
        use windows::Win32::System::DataExchange::{EmptyClipboard, OpenClipboard};

        unsafe {
            OpenClipboard(None).map_err(|err| format!("open clipboard: {}", err))?;
            self.opened = true;
            EmptyClipboard().map_err(|err| format!("empty clipboard: {}", err))
        }
    }

    fn set_format(&mut self, format: u32, handle: Self::Handle) -> Result<(), String> {
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::DataExchange::SetClipboardData;

        unsafe {
            SetClipboardData(format, Some(HANDLE(handle.0)))
                .map(|_| ())
                .map_err(|err| format!("set clipboard data: {}", err))
        }
    }

    fn free_handle(&mut self, handle: Self::Handle) {
        use windows::Win32::Foundation::GlobalFree;

        unsafe {
            let _ = GlobalFree(Some(handle));
        }
    }
}

#[cfg(target_os = "windows")]
impl NativeClipboardSnapshot {
    fn capture() -> Result<Self, String> {
        use windows::Win32::Foundation::HGLOBAL;
        use windows::Win32::System::DataExchange::{
            EnumClipboardFormats, GetClipboardData, OpenClipboard,
        };
        use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

        struct ClipboardGuard;

        impl Drop for ClipboardGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = windows::Win32::System::DataExchange::CloseClipboard();
                }
            }
        }

        unsafe {
            OpenClipboard(None).map_err(|err| format!("open clipboard: {}", err))?;
            let _guard = ClipboardGuard;
            let mut formats = Vec::new();
            let mut format = 0;

            loop {
                format = EnumClipboardFormats(format);
                if format == 0 {
                    break;
                }

                let handle = match GetClipboardData(format) {
                    Ok(handle) if !handle.0.is_null() => handle,
                    _ => continue,
                };
                let hglobal = HGLOBAL(handle.0);
                let size = GlobalSize(hglobal);
                if size == 0 {
                    continue;
                }

                let locked = GlobalLock(hglobal);
                if locked.is_null() {
                    continue;
                }

                let bytes = std::slice::from_raw_parts(locked.cast::<u8>(), size).to_vec();
                let _ = GlobalUnlock(hglobal);
                formats.push(ClipboardFormatData { format, bytes });
            }

            if formats.is_empty() {
                Err("no memory-backed clipboard formats captured".to_string())
            } else {
                Ok(Self { formats })
            }
        }
    }

    fn restore(&self) -> Result<(), String> {
        let mut ops = WinClipboardRestoreOps { opened: false };
        restore_native_clipboard_snapshot_with_ops(&self.formats, &mut ops)
    }

    #[cfg(test)]
    fn for_test() -> Self {
        Self {
            formats: vec![ClipboardFormatData {
                format: 13,
                bytes: b"test\0".to_vec(),
            }],
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Clone, Debug)]
struct NativeClipboardSnapshot;

#[cfg(not(target_os = "windows"))]
impl NativeClipboardSnapshot {
    fn capture() -> Result<Self, String> {
        Err("native clipboard snapshots are not supported on this platform".to_string())
    }

    fn restore(&self) -> Result<(), String> {
        Err("native clipboard snapshots are not supported on this platform".to_string())
    }

    #[cfg(test)]
    fn for_test() -> Self {
        Self
    }
}

fn write_text_clipboard(app_handle: &AppHandle, text: &str) -> Result<(), String> {
    let clipboard = app_handle.clipboard();

    #[cfg(target_os = "linux")]
    {
        if is_wayland() && is_wl_copy_available() {
            info!("Using wl-copy for clipboard write on Wayland");
            return write_clipboard_via_wl_copy(text);
        }
    }

    clipboard
        .write_text(text)
        .map_err(|e| format!("Failed to write to clipboard: {}", e))
}

fn restore_text_clipboard(app_handle: &AppHandle, text: &str) {
    if let Err(err) = write_text_clipboard(app_handle, text) {
        warn!("Failed to restore text clipboard fallback: {}", err);
    }
}

/// Pastes text using the clipboard: saves current content, writes text, sends paste keystroke, restores clipboard.
fn paste_via_clipboard(
    enigo: &mut Enigo,
    text: &str,
    app_handle: &AppHandle,
    paste_method: &PasteMethod,
    paste_delay_ms: u64,
) -> Result<(), String> {
    let clipboard_snapshot = ClipboardSnapshot::capture(app_handle);

    // Write text to clipboard first
    write_text_clipboard(app_handle, text)?;

    std::thread::sleep(Duration::from_millis(paste_delay_ms));

    // Send paste key combo
    #[cfg(target_os = "linux")]
    let key_combo_sent = try_send_key_combo_linux(paste_method)?;

    #[cfg(not(target_os = "linux"))]
    let key_combo_sent = false;

    // Fall back to enigo if no native tool handled it
    if !key_combo_sent {
        match paste_method {
            PasteMethod::CtrlV => input::send_paste_ctrl_v(enigo)?,
            PasteMethod::CtrlShiftV => input::send_paste_ctrl_shift_v(enigo)?,
            PasteMethod::ShiftInsert => input::send_paste_shift_insert(enigo)?,
            _ => return Err("Invalid paste method for clipboard paste".into()),
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(50));

    // Restore original clipboard content, including non-text formats where supported.
    clipboard_snapshot.restore(app_handle);

    Ok(())
}

/// Attempts to send a key combination using Linux-native tools.
/// Returns `Ok(true)` if a native tool handled it, `Ok(false)` to fall back to enigo.
#[cfg(target_os = "linux")]
fn try_send_key_combo_linux(paste_method: &PasteMethod) -> Result<bool, String> {
    if is_wayland() {
        // Wayland: prefer wtype (but not on KDE), then dotool, then ydotool
        // Note: wtype doesn't work on KDE (no zwp_virtual_keyboard_manager_v1 support)
        if !is_kde_wayland() && is_wtype_available() {
            info!("Using wtype for key combo");
            send_key_combo_via_wtype(paste_method)?;
            return Ok(true);
        }
        if is_dotool_available() {
            info!("Using dotool for key combo");
            send_key_combo_via_dotool(paste_method)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for key combo");
            send_key_combo_via_ydotool(paste_method)?;
            return Ok(true);
        }
    } else {
        // X11: prefer xdotool, then ydotool
        if is_xdotool_available() {
            info!("Using xdotool for key combo");
            send_key_combo_via_xdotool(paste_method)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for key combo");
            send_key_combo_via_ydotool(paste_method)?;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Attempts to type text directly using Linux-native tools.
/// Returns `Ok(true)` if a native tool handled it, `Ok(false)` to fall back to enigo.
#[cfg(target_os = "linux")]
fn try_direct_typing_linux(text: &str, preferred_tool: TypingTool) -> Result<bool, String> {
    // If user specified a tool, try only that one
    if preferred_tool != TypingTool::Auto {
        return match preferred_tool {
            TypingTool::Wtype if is_wtype_available() => {
                info!("Using user-specified wtype");
                type_text_via_wtype(text)?;
                Ok(true)
            }
            TypingTool::Kwtype if is_kwtype_available() => {
                info!("Using user-specified kwtype");
                type_text_via_kwtype(text)?;
                Ok(true)
            }
            TypingTool::Dotool if is_dotool_available() => {
                info!("Using user-specified dotool");
                type_text_via_dotool(text)?;
                Ok(true)
            }
            TypingTool::Ydotool if is_ydotool_available() => {
                info!("Using user-specified ydotool");
                type_text_via_ydotool(text)?;
                Ok(true)
            }
            TypingTool::Xdotool if is_xdotool_available() => {
                info!("Using user-specified xdotool");
                type_text_via_xdotool(text)?;
                Ok(true)
            }
            _ => Err(format!(
                "Typing tool {:?} is not available on this system",
                preferred_tool
            )),
        };
    }

    // Auto mode - existing fallback chain
    if is_wayland() {
        // KDE Wayland: prefer kwtype (uses KDE Fake Input protocol, supports umlauts)
        if is_kde_wayland() && is_kwtype_available() {
            info!("Using kwtype for direct text input on KDE Wayland");
            type_text_via_kwtype(text)?;
            return Ok(true);
        }
        // Wayland: prefer wtype, then dotool, then ydotool
        // Note: wtype doesn't work on KDE (no zwp_virtual_keyboard_manager_v1 support)
        if !is_kde_wayland() && is_wtype_available() {
            info!("Using wtype for direct text input");
            type_text_via_wtype(text)?;
            return Ok(true);
        }
        if is_dotool_available() {
            info!("Using dotool for direct text input");
            type_text_via_dotool(text)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for direct text input");
            type_text_via_ydotool(text)?;
            return Ok(true);
        }
    } else {
        // X11: prefer xdotool, then ydotool
        if is_xdotool_available() {
            info!("Using xdotool for direct text input");
            type_text_via_xdotool(text)?;
            return Ok(true);
        }
        if is_ydotool_available() {
            info!("Using ydotool for direct text input");
            type_text_via_ydotool(text)?;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Returns the list of available typing tools on this system.
/// Always includes "auto" as the first entry.
#[cfg(target_os = "linux")]
pub fn get_available_typing_tools() -> Vec<String> {
    let mut tools = vec!["auto".to_string()];
    if is_wtype_available() {
        tools.push("wtype".to_string());
    }
    if is_kwtype_available() {
        tools.push("kwtype".to_string());
    }
    if is_dotool_available() {
        tools.push("dotool".to_string());
    }
    if is_ydotool_available() {
        tools.push("ydotool".to_string());
    }
    if is_xdotool_available() {
        tools.push("xdotool".to_string());
    }
    tools
}

/// Check if wtype is available (Wayland text input tool)
#[cfg(target_os = "linux")]
fn is_wtype_available() -> bool {
    Command::new("which")
        .arg("wtype")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if dotool is available (another Wayland text input tool)
#[cfg(target_os = "linux")]
fn is_dotool_available() -> bool {
    Command::new("which")
        .arg("dotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if ydotool is available (uinput-based, works on both Wayland and X11)
#[cfg(target_os = "linux")]
fn is_ydotool_available() -> bool {
    Command::new("which")
        .arg("ydotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn is_xdotool_available() -> bool {
    Command::new("which")
        .arg("xdotool")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if kwtype is available (KDE Wayland virtual keyboard input tool)
#[cfg(target_os = "linux")]
fn is_kwtype_available() -> bool {
    Command::new("which")
        .arg("kwtype")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Check if wl-copy is available (Wayland clipboard tool)
#[cfg(target_os = "linux")]
fn is_wl_copy_available() -> bool {
    Command::new("which")
        .arg("wl-copy")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Type text directly via wtype on Wayland.
#[cfg(target_os = "linux")]
fn type_text_via_wtype(text: &str) -> Result<(), String> {
    let output = Command::new("wtype")
        .arg("--") // Protect against text starting with -
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute wtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("wtype failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via xdotool on X11.
#[cfg(target_os = "linux")]
fn type_text_via_xdotool(text: &str) -> Result<(), String> {
    let output = Command::new("xdotool")
        .arg("type")
        .arg("--clearmodifiers")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute xdotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("xdotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via dotool (works on both Wayland and X11 via uinput).
#[cfg(target_os = "linux")]
fn type_text_via_dotool(text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new("dotool")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn dotool: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        // dotool uses "type <text>" command
        writeln!(stdin, "type {}", text)
            .map_err(|e| format!("Failed to write to dotool stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for dotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("dotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via ydotool (uinput-based, requires ydotoold daemon).
#[cfg(target_os = "linux")]
fn type_text_via_ydotool(text: &str) -> Result<(), String> {
    let output = Command::new("ydotool")
        .arg("type")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute ydotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ydotool failed: {}", stderr));
    }

    Ok(())
}

/// Type text directly via kwtype (KDE Wayland virtual keyboard, uses KDE Fake Input protocol).
#[cfg(target_os = "linux")]
fn type_text_via_kwtype(text: &str) -> Result<(), String> {
    let output = Command::new("kwtype")
        .arg("--")
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute kwtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("kwtype failed: {}", stderr));
    }

    Ok(())
}

/// Write text to clipboard via wl-copy (Wayland clipboard tool).
/// Uses Stdio::null() to avoid blocking on repeated calls — wl-copy forks a
/// daemon that inherits piped fds, causing read_to_end to hang indefinitely.
#[cfg(target_os = "linux")]
fn write_clipboard_via_wl_copy(text: &str) -> Result<(), String> {
    use std::process::Stdio;
    let status = Command::new("wl-copy")
        .arg("--")
        .arg(text)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute wl-copy: {}", e))?;

    if !status.success() {
        return Err("wl-copy failed".into());
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via wtype on Wayland.
#[cfg(target_os = "linux")]
fn send_key_combo_via_wtype(paste_method: &PasteMethod) -> Result<(), String> {
    let args: Vec<&str> = match paste_method {
        PasteMethod::CtrlV => vec!["-M", "ctrl", "-k", "v"],
        PasteMethod::ShiftInsert => vec!["-M", "shift", "-k", "Insert"],
        PasteMethod::CtrlShiftV => vec!["-M", "ctrl", "-M", "shift", "-k", "v"],
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("wtype")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute wtype: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("wtype failed: {}", stderr));
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via dotool.
#[cfg(target_os = "linux")]
fn send_key_combo_via_dotool(paste_method: &PasteMethod) -> Result<(), String> {
    let command;
    match paste_method {
        PasteMethod::CtrlV => command = "echo key ctrl+v | dotool",
        PasteMethod::ShiftInsert => command = "echo key shift+insert | dotool",
        PasteMethod::CtrlShiftV => command = "echo key ctrl+shift+v | dotool",
        _ => return Err("Unsupported paste method".into()),
    }
    use std::process::Stdio;
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("Failed to execute dotool: {}", e))?;
    if !status.success() {
        return Err("dotool failed".into());
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via ydotool (requires ydotoold daemon).
#[cfg(target_os = "linux")]
fn send_key_combo_via_ydotool(paste_method: &PasteMethod) -> Result<(), String> {
    // ydotool uses Linux input event keycodes with format <keycode>:<pressed>
    // where pressed is 1 for down, 0 for up. Keycodes: ctrl=29, shift=42, v=47, insert=110
    let args: Vec<&str> = match paste_method {
        PasteMethod::CtrlV => vec!["key", "29:1", "47:1", "47:0", "29:0"],
        PasteMethod::ShiftInsert => vec!["key", "42:1", "110:1", "110:0", "42:0"],
        PasteMethod::CtrlShiftV => vec!["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"],
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("ydotool")
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute ydotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ydotool failed: {}", stderr));
    }

    Ok(())
}

/// Send a key combination (e.g., Ctrl+V) via xdotool on X11.
#[cfg(target_os = "linux")]
fn send_key_combo_via_xdotool(paste_method: &PasteMethod) -> Result<(), String> {
    let key_combo = match paste_method {
        PasteMethod::CtrlV => "ctrl+v",
        PasteMethod::CtrlShiftV => "ctrl+shift+v",
        PasteMethod::ShiftInsert => "shift+Insert",
        _ => return Err("Unsupported paste method".into()),
    };

    let output = Command::new("xdotool")
        .arg("key")
        .arg("--clearmodifiers")
        .arg(key_combo)
        .output()
        .map_err(|e| format!("Failed to execute xdotool: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("xdotool failed: {}", stderr));
    }

    Ok(())
}

/// Pastes text by invoking an external script.
/// The script receives the text to paste as a single argument.
fn paste_via_external_script(text: &str, script_path: &str) -> Result<(), String> {
    info!("Pasting via external script: {}", script_path);

    let output = Command::new(script_path)
        .arg(text)
        .output()
        .map_err(|e| format!("Failed to execute external script '{}': {}", script_path, e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "External script '{}' failed with exit code {:?}. stderr: {}, stdout: {}",
            script_path,
            output.status.code(),
            stderr.trim(),
            stdout.trim()
        ));
    }

    Ok(())
}

/// Types text directly by simulating individual key presses.
fn paste_direct(
    enigo: &mut Enigo,
    text: &str,
    #[cfg(target_os = "linux")] typing_tool: TypingTool,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if try_direct_typing_linux(text, typing_tool)? {
            return Ok(());
        }
        info!("Falling back to enigo for direct text input");
    }

    input::paste_text_direct(enigo, text)
}

fn send_return_key(enigo: &mut Enigo, key_type: AutoSubmitKey) -> Result<(), String> {
    match key_type {
        AutoSubmitKey::Enter => {
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
        }
        AutoSubmitKey::CtrlEnter => {
            enigo
                .key(Key::Control, Direction::Press)
                .map_err(|e| format!("Failed to press Control key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
            enigo
                .key(Key::Control, Direction::Release)
                .map_err(|e| format!("Failed to release Control key: {}", e))?;
        }
        AutoSubmitKey::CmdEnter => {
            enigo
                .key(Key::Meta, Direction::Press)
                .map_err(|e| format!("Failed to press Meta/Cmd key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Press)
                .map_err(|e| format!("Failed to press Return key: {}", e))?;
            enigo
                .key(Key::Return, Direction::Release)
                .map_err(|e| format!("Failed to release Return key: {}", e))?;
            enigo
                .key(Key::Meta, Direction::Release)
                .map_err(|e| format!("Failed to release Meta/Cmd key: {}", e))?;
        }
    }

    Ok(())
}

fn should_send_auto_submit(auto_submit: bool, paste_method: PasteMethod) -> bool {
    auto_submit && paste_method != PasteMethod::None
}

fn insertion_method_for_paste_method(paste_method: PasteMethod) -> InsertionMethod {
    match paste_method {
        PasteMethod::None => InsertionMethod::None,
        PasteMethod::Direct => InsertionMethod::Direct,
        PasteMethod::CtrlV | PasteMethod::CtrlShiftV | PasteMethod::ShiftInsert => {
            InsertionMethod::Clipboard
        }
        PasteMethod::ExternalScript => InsertionMethod::ExternalScript,
    }
}

fn receipt_from_result(
    paste_method: PasteMethod,
    target_verified: bool,
    result: Result<(), String>,
) -> InsertionReceipt {
    let attempted = paste_method != PasteMethod::None;
    match result {
        Ok(()) => InsertionReceipt {
            attempted,
            succeeded: true,
            method: insertion_method_for_paste_method(paste_method),
            target_verified,
            error: None,
        },
        Err(error) => InsertionReceipt {
            attempted,
            succeeded: false,
            method: insertion_method_for_paste_method(paste_method),
            target_verified,
            error: Some(error),
        },
    }
}

pub fn paste(text: String, app_handle: AppHandle) -> Result<(), String> {
    let settings = get_settings(&app_handle);
    let paste_method = settings.paste_method;
    let paste_delay_ms = settings.paste_delay_ms;

    // Append trailing space if setting is enabled
    let text = if settings.append_trailing_space {
        format!("{} ", text)
    } else {
        text
    };

    let before_paste_snapshot =
        if settings.auto_add_dictionary_words && paste_method != PasteMethod::None {
            crate::post_paste_learning::capture_focused_text_snapshot()
        } else {
            None
        };

    info!(
        "Using paste method: {:?}, delay: {}ms",
        paste_method, paste_delay_ms
    );

    // Get the managed Enigo instance
    let enigo_state = app_handle
        .try_state::<EnigoState>()
        .ok_or("Enigo state not initialized")?;
    let mut enigo = enigo_state
        .0
        .lock()
        .map_err(|e| format!("Failed to lock Enigo: {}", e))?;

    // Perform the paste operation
    match paste_method {
        PasteMethod::None => {
            info!("PasteMethod::None selected - skipping paste action");
        }
        PasteMethod::Direct => {
            paste_direct(
                &mut enigo,
                &text,
                #[cfg(target_os = "linux")]
                settings.typing_tool,
            )?;
        }
        PasteMethod::CtrlV | PasteMethod::CtrlShiftV | PasteMethod::ShiftInsert => {
            paste_via_clipboard(
                &mut enigo,
                &text,
                &app_handle,
                &paste_method,
                paste_delay_ms,
            )?
        }
        PasteMethod::ExternalScript => {
            let script_path = settings
                .external_script_path
                .as_ref()
                .filter(|p| !p.is_empty())
                .ok_or("External script path is not configured")?;
            paste_via_external_script(&text, script_path)?;
        }
    }

    if should_send_auto_submit(settings.auto_submit, paste_method) {
        std::thread::sleep(Duration::from_millis(50));
        send_return_key(&mut enigo, settings.auto_submit_key)?;
    }

    // After pasting, optionally copy to clipboard based on settings
    if settings.clipboard_handling == ClipboardHandling::CopyToClipboard {
        let clipboard = app_handle.clipboard();
        clipboard
            .write_text(&text)
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    }

    if settings.auto_add_dictionary_words && paste_method != PasteMethod::None {
        crate::post_paste_learning::maybe_spawn_auto_add_watcher(
            app_handle.clone(),
            text,
            before_paste_snapshot,
        );
    }

    Ok(())
}

pub fn paste_with_receipt(
    text: String,
    app_handle: AppHandle,
    target_verified: bool,
) -> InsertionReceipt {
    let settings = get_settings(&app_handle);
    let paste_method = settings.paste_method;
    let result = paste(text, app_handle);
    receipt_from_result(paste_method, target_verified, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_submit_requires_setting_enabled() {
        assert!(!should_send_auto_submit(false, PasteMethod::CtrlV));
        assert!(!should_send_auto_submit(false, PasteMethod::Direct));
    }

    #[test]
    fn auto_submit_skips_none_paste_method() {
        assert!(!should_send_auto_submit(true, PasteMethod::None));
    }

    #[test]
    fn auto_submit_runs_for_active_paste_methods() {
        assert!(should_send_auto_submit(true, PasteMethod::CtrlV));
        assert!(should_send_auto_submit(true, PasteMethod::Direct));
        assert!(should_send_auto_submit(true, PasteMethod::CtrlShiftV));
        assert!(should_send_auto_submit(true, PasteMethod::ShiftInsert));
    }

    #[test]
    fn receipt_method_matches_paste_method() {
        assert_eq!(
            insertion_method_for_paste_method(PasteMethod::None),
            InsertionMethod::None
        );
        assert_eq!(
            insertion_method_for_paste_method(PasteMethod::Direct),
            InsertionMethod::Direct
        );
        assert_eq!(
            insertion_method_for_paste_method(PasteMethod::CtrlV),
            InsertionMethod::Clipboard
        );
        assert_eq!(
            insertion_method_for_paste_method(PasteMethod::CtrlShiftV),
            InsertionMethod::Clipboard
        );
        assert_eq!(
            insertion_method_for_paste_method(PasteMethod::ShiftInsert),
            InsertionMethod::Clipboard
        );
        assert_eq!(
            insertion_method_for_paste_method(PasteMethod::ExternalScript),
            InsertionMethod::ExternalScript
        );
    }

    #[test]
    fn paste_none_receipt_counts_as_not_attempted() {
        let receipt = receipt_from_result(PasteMethod::None, true, Ok(()));
        assert!(!receipt.attempted);
        assert!(receipt.succeeded);
        assert_eq!(receipt.method, InsertionMethod::None);
    }

    #[test]
    fn failed_receipt_keeps_error_message() {
        let receipt = receipt_from_result(PasteMethod::Direct, true, Err("failed".to_string()));
        assert!(receipt.attempted);
        assert!(!receipt.succeeded);
        assert_eq!(receipt.error.as_deref(), Some("failed"));
    }

    #[test]
    fn native_clipboard_snapshot_does_not_restore_empty_text_placeholder() {
        let snapshot = ClipboardSnapshot::native_for_test();

        assert_eq!(snapshot.fallback_text_for_restore(), None);
    }

    #[cfg(target_os = "windows")]
    #[derive(Default)]
    struct FakeNativeRestoreOps {
        fail_allocate_format: Option<u32>,
        fail_set_format: Option<u32>,
        opened_and_emptied: bool,
        next_handle: usize,
        set_attempts: Vec<u32>,
        freed_handles: Vec<usize>,
    }

    #[cfg(target_os = "windows")]
    impl NativeClipboardRestoreOps for FakeNativeRestoreOps {
        type Handle = usize;

        fn allocate_format(
            &mut self,
            format: &ClipboardFormatData,
        ) -> Result<Self::Handle, String> {
            if self.fail_allocate_format == Some(format.format) {
                return Err(format!("allocate {}", format.format));
            }

            self.next_handle += 1;
            Ok(self.next_handle)
        }

        fn open_and_empty(&mut self) -> Result<(), String> {
            self.opened_and_emptied = true;
            Ok(())
        }

        fn set_format(&mut self, format: u32, handle: Self::Handle) -> Result<(), String> {
            self.set_attempts.push(format);
            if self.fail_set_format == Some(format) {
                return Err(format!("set {format} via {handle}"));
            }
            Ok(())
        }

        fn free_handle(&mut self, handle: Self::Handle) {
            self.freed_handles.push(handle);
        }
    }

    #[cfg(target_os = "windows")]
    fn test_formats() -> Vec<ClipboardFormatData> {
        vec![
            ClipboardFormatData {
                format: 13,
                bytes: b"text\0".to_vec(),
            },
            ClipboardFormatData {
                format: 15,
                bytes: b"files".to_vec(),
            },
        ]
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_restore_does_not_empty_clipboard_when_preparation_fails() {
        let mut ops = FakeNativeRestoreOps {
            fail_allocate_format: Some(15),
            ..FakeNativeRestoreOps::default()
        };

        let result = restore_native_clipboard_snapshot_with_ops(&test_formats(), &mut ops);

        assert!(result.is_err());
        assert!(!ops.opened_and_emptied);
        assert_eq!(ops.freed_handles, vec![1]);
        assert!(ops.set_attempts.is_empty());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_restore_attempts_remaining_formats_after_set_failure() {
        let mut ops = FakeNativeRestoreOps {
            fail_set_format: Some(13),
            ..FakeNativeRestoreOps::default()
        };

        let result = restore_native_clipboard_snapshot_with_ops(&test_formats(), &mut ops);

        assert!(result.is_err());
        assert!(ops.opened_and_emptied);
        assert_eq!(ops.set_attempts, vec![13, 15]);
        assert_eq!(ops.freed_handles, vec![1]);
    }
}
