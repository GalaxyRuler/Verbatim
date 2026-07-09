use crate::adaptive::types::{InsertionMethod, InsertionReceipt};
use crate::input::{self, EnigoState};
use crate::post_paste_learning::{FocusedTextSnapshot, MAX_FOCUSED_TEXT_CHARS};
#[cfg(target_os = "linux")]
use crate::settings::TypingTool;
use crate::settings::{get_settings, AutoSubmitKey, ClipboardHandling, PasteMethod};
use enigo::{Direction, Enigo, Key, Keyboard};
use log::{info, warn};
use serde::Serialize;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[cfg(target_os = "linux")]
use crate::utils::{is_kde_wayland, is_wayland};

pub(crate) type CancellationCheck<'a> = Option<&'a dyn Fn() -> bool>;
const CLIPBOARD_PAYLOAD_POLL_INTERVAL_MS: u64 = 10;
const FOCUSED_TEXT_READ_CAP: usize = MAX_FOCUSED_TEXT_CHARS;
const PASTE_VERIFY_TOTAL_MS: u64 = 600;
const PASTE_VERIFY_POLL_MS: u64 = 75;
const TARGET_CHANGED_BEFORE_INSERTION: &str = "target changed before insertion";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ClipboardPayloadMarker {
    sequence_number: Option<u32>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct NativeSmokeClipboardSafetyDrillCase {
    case: String,
    owned_by_verbatim: bool,
    expected_owned_by_verbatim: bool,
    passed: bool,
}

impl ClipboardPayloadMarker {
    fn capture_current() -> Self {
        Self {
            sequence_number: clipboard_sequence_number(),
        }
    }
}

fn ensure_not_cancelled(is_cancelled: CancellationCheck<'_>, stage: &str) -> Result<(), String> {
    if is_cancelled.is_some_and(|check| check()) {
        return Err(format!("Operation cancelled before {stage}"));
    }

    Ok(())
}

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

        match NativeClipboardSnapshot::capture(app_handle) {
            Ok(native) => Self::Native {
                native,
                fallback_text,
            },
            Err(_) => Self::TextFallback(fallback_text),
        }
    }

    fn restore(&self, app_handle: &AppHandle) -> Result<(), String> {
        match self {
            Self::Native {
                native,
                fallback_text,
            } => {
                if let Err(err) = native.restore(app_handle) {
                    warn!("Failed to restore native clipboard snapshot: {}", err);
                    if let Some(text) = fallback_text {
                        restore_text_clipboard(app_handle, text).map_err(|fallback_err| {
                            format!("{err}; text fallback restore failed: {fallback_err}")
                        })?;
                    }
                    return Err(err);
                }
                Ok(())
            }
            Self::TextFallback(Some(text)) => restore_text_clipboard(app_handle, text),
            Self::TextFallback(None) => Ok(()),
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

/// Restores the pre-paste clipboard on every exit path unless explicitly disarmed.
struct RestoreOnDrop<F: FnMut()> {
    restore: F,
    armed: bool,
}

impl<F: FnMut()> RestoreOnDrop<F> {
    fn new(restore: F) -> Self {
        Self {
            restore,
            armed: true,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl<F: FnMut()> Drop for RestoreOnDrop<F> {
    fn drop(&mut self) {
        if self.armed {
            (self.restore)();
        }
    }
}

/// Owns the pre-paste clipboard snapshot and payload marker. Created inside
/// paste_via_clipboard; on success ownership moves to the caller so restore
/// happens after the caller finishes post-paste handling.
struct ClipboardPasteSession {
    _marker: ClipboardPayloadMarker,
    _restore_guard: RestoreOnDrop<Box<dyn FnMut()>>,
}

impl ClipboardPasteSession {
    fn new(
        app_handle: &AppHandle,
        payload: &str,
        snapshot: ClipboardSnapshot,
        marker: ClipboardPayloadMarker,
    ) -> Self {
        let app_handle = app_handle.clone();
        let payload = payload.to_string();
        let restore_guard = RestoreOnDrop::new(Box::new(move || {
            if clipboard_still_contains_verbatim_payload(&app_handle, &payload, Some(marker)) {
                if let Err(err) = snapshot.restore(&app_handle) {
                    warn!("Failed to restore clipboard on exit path: {err}");
                }
            } else {
                warn!("Skipping clipboard restore: clipboard changed after payload write");
            }
        }) as Box<dyn FnMut()>);

        Self {
            _marker: marker,
            _restore_guard: restore_guard,
        }
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
struct NativeClipboardSnapshot {
    formats: Vec<ClipboardFormatData>,
    image_payload: Option<DesktopClipboardPayload>,
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
struct ClipboardFormatData {
    format: u32,
    bytes: Vec<u8>,
}

#[cfg(target_os = "windows")]
fn is_memory_backed_clipboard_format(format: u32) -> bool {
    const CF_TEXT: u32 = 1;
    const CF_SYLK: u32 = 4;
    const CF_DIF: u32 = 5;
    const CF_TIFF: u32 = 6;
    const CF_OEMTEXT: u32 = 7;
    const CF_DIB: u32 = 8;
    const CF_PENDATA: u32 = 10;
    const CF_RIFF: u32 = 11;
    const CF_WAVE: u32 = 12;
    const CF_UNICODETEXT: u32 = 13;
    const CF_HDROP: u32 = 15;
    const CF_LOCALE: u32 = 16;
    const CF_DIBV5: u32 = 17;
    const REGISTERED_FORMAT_START: u32 = 0xC000;

    matches!(
        format,
        CF_TEXT
            | CF_SYLK
            | CF_DIF
            | CF_TIFF
            | CF_OEMTEXT
            | CF_DIB
            | CF_PENDATA
            | CF_RIFF
            | CF_WAVE
            | CF_UNICODETEXT
            | CF_HDROP
            | CF_LOCALE
            | CF_DIBV5
    ) || format >= REGISTERED_FORMAT_START
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
fn restore_native_clipboard_snapshot_with_image_fallback<
    O: NativeClipboardRestoreOps,
    D: DesktopClipboardOps,
>(
    formats: &[ClipboardFormatData],
    image_payload: Option<&DesktopClipboardPayload>,
    native_ops: &mut O,
    desktop_ops: &mut D,
) -> Result<(), String> {
    if formats.is_empty() {
        if let Some(image_payload) = image_payload {
            return restore_desktop_clipboard_payload(image_payload, desktop_ops)
                .map_err(|err| format!("restore clipboard image fallback: {err}"));
        }
        return Err("native clipboard snapshot is empty".to_string());
    }

    match restore_native_clipboard_snapshot_with_ops(formats, native_ops) {
        Ok(()) => Ok(()),
        Err(native_err) => {
            if let Some(image_payload) = image_payload {
                restore_desktop_clipboard_payload(image_payload, desktop_ops).map_err(|image_err| {
                    format!("{native_err}; image fallback restore failed: {image_err}")
                })
            } else {
                Err(native_err)
            }
        }
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
    fn capture(app_handle: &AppHandle) -> Result<Self, String> {
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

        let image_payload = PluginDesktopClipboardOps { app_handle }
            .read_image_payload()
            .ok();

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
                if !is_memory_backed_clipboard_format(format) {
                    continue;
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

            if formats.is_empty() && image_payload.is_none() {
                Err("no memory-backed or image clipboard formats captured".to_string())
            } else {
                Ok(Self {
                    formats,
                    image_payload,
                })
            }
        }
    }

    fn restore(&self, app_handle: &AppHandle) -> Result<(), String> {
        let mut ops = WinClipboardRestoreOps { opened: false };
        let mut desktop_ops = PluginDesktopClipboardOps { app_handle };
        restore_native_clipboard_snapshot_with_image_fallback(
            &self.formats,
            self.image_payload.as_ref(),
            &mut ops,
            &mut desktop_ops,
        )
    }

    #[cfg(test)]
    fn for_test() -> Self {
        Self {
            formats: vec![ClipboardFormatData {
                format: 13,
                bytes: b"test\0".to_vec(),
            }],
            image_payload: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(target_os = "windows", allow(dead_code))]
enum DesktopClipboardPayload {
    Image {
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    },
    Text(String),
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
trait DesktopClipboardOps {
    fn read_image_payload(&self) -> Result<DesktopClipboardPayload, String>;
    fn read_text_payload(&self) -> Result<DesktopClipboardPayload, String>;
    fn write_image_payload(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), String>;
    fn write_text_payload(&mut self, text: &str) -> Result<(), String>;
}

#[cfg_attr(target_os = "windows", allow(dead_code))]
fn capture_desktop_clipboard_payload<O: DesktopClipboardOps>(
    ops: &O,
) -> Result<DesktopClipboardPayload, String> {
    if let Ok(image) = ops.read_image_payload() {
        return Ok(image);
    }

    match ops.read_text_payload()? {
        DesktopClipboardPayload::Text(text) if !text.is_empty() => {
            Ok(DesktopClipboardPayload::Text(text))
        }
        DesktopClipboardPayload::Text(_) => Err("desktop clipboard text is empty".to_string()),
        image @ DesktopClipboardPayload::Image { .. } => Ok(image),
    }
}

fn restore_desktop_clipboard_payload<O: DesktopClipboardOps>(
    payload: &DesktopClipboardPayload,
    ops: &mut O,
) -> Result<(), String> {
    match payload {
        DesktopClipboardPayload::Image {
            rgba,
            width,
            height,
        } => ops.write_image_payload(rgba, *width, *height),
        DesktopClipboardPayload::Text(text) => ops.write_text_payload(text),
    }
}

struct PluginDesktopClipboardOps<'a> {
    app_handle: &'a AppHandle,
}

impl DesktopClipboardOps for PluginDesktopClipboardOps<'_> {
    fn read_image_payload(&self) -> Result<DesktopClipboardPayload, String> {
        let image = self
            .app_handle
            .clipboard()
            .read_image()
            .map_err(|err| format!("read clipboard image: {}", err))?;
        Ok(DesktopClipboardPayload::Image {
            rgba: image.rgba().to_vec(),
            width: image.width(),
            height: image.height(),
        })
    }

    fn read_text_payload(&self) -> Result<DesktopClipboardPayload, String> {
        let text = self
            .app_handle
            .clipboard()
            .read_text()
            .map_err(|err| format!("read clipboard text: {}", err))?;
        Ok(DesktopClipboardPayload::Text(text))
    }

    fn write_image_payload(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
        let image = tauri::image::Image::new_owned(rgba.to_vec(), width, height);
        self.app_handle
            .clipboard()
            .write_image(&image)
            .map_err(|err| format!("write clipboard image: {}", err))
    }

    fn write_text_payload(&mut self, text: &str) -> Result<(), String> {
        write_text_clipboard(self.app_handle, text)
    }
}

#[cfg(not(target_os = "windows"))]
#[derive(Clone, Debug)]
struct NativeClipboardSnapshot {
    payload: DesktopClipboardPayload,
}

#[cfg(not(target_os = "windows"))]
impl NativeClipboardSnapshot {
    fn capture(app_handle: &AppHandle) -> Result<Self, String> {
        let ops = PluginDesktopClipboardOps { app_handle };
        capture_desktop_clipboard_payload(&ops).map(|payload| Self { payload })
    }

    fn restore(&self, app_handle: &AppHandle) -> Result<(), String> {
        let mut ops = PluginDesktopClipboardOps { app_handle };
        restore_desktop_clipboard_payload(&self.payload, &mut ops)
    }

    #[cfg(test)]
    fn for_test() -> Self {
        Self {
            payload: DesktopClipboardPayload::Text("test".to_string()),
        }
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

fn restore_text_clipboard(app_handle: &AppHandle, text: &str) -> Result<(), String> {
    write_text_clipboard(app_handle, text).map_err(|err| {
        warn!("Failed to restore text clipboard fallback: {}", err);
        err
    })
}

#[cfg(target_os = "windows")]
fn clipboard_sequence_number() -> Option<u32> {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

    let sequence = unsafe { GetClipboardSequenceNumber() };
    (sequence != 0).then_some(sequence)
}

#[cfg(not(target_os = "windows"))]
fn clipboard_sequence_number() -> Option<u32> {
    None
}

fn clipboard_payload_owned_by_verbatim(
    current_text: Option<&str>,
    payload: &str,
    expected_marker: Option<ClipboardPayloadMarker>,
    current_marker: ClipboardPayloadMarker,
) -> bool {
    if current_text != Some(payload) {
        return false;
    }

    match expected_marker.and_then(|marker| marker.sequence_number) {
        Some(expected_sequence) => current_marker.sequence_number == Some(expected_sequence),
        None => true,
    }
}

pub(crate) fn native_smoke_clipboard_safety_drill() -> Vec<NativeSmokeClipboardSafetyDrillCase> {
    fn run_case(
        case: &str,
        current_text: Option<&str>,
        payload: &str,
        expected_marker: Option<ClipboardPayloadMarker>,
        current_marker: ClipboardPayloadMarker,
        expected_owned_by_verbatim: bool,
    ) -> NativeSmokeClipboardSafetyDrillCase {
        let owned_by_verbatim = clipboard_payload_owned_by_verbatim(
            current_text,
            payload,
            expected_marker,
            current_marker,
        );

        NativeSmokeClipboardSafetyDrillCase {
            case: case.to_string(),
            owned_by_verbatim,
            expected_owned_by_verbatim,
            passed: owned_by_verbatim == expected_owned_by_verbatim,
        }
    }

    vec![
        run_case(
            "same_text_sequence_changed",
            Some("verbatim payload"),
            "verbatim payload",
            Some(ClipboardPayloadMarker {
                sequence_number: Some(42),
            }),
            ClipboardPayloadMarker {
                sequence_number: Some(43),
            },
            false,
        ),
        run_case(
            "changed_text_matching_sequence",
            Some("user changed clipboard"),
            "verbatim payload",
            Some(ClipboardPayloadMarker {
                sequence_number: Some(42),
            }),
            ClipboardPayloadMarker {
                sequence_number: Some(42),
            },
            false,
        ),
        run_case(
            "exact_text_without_sequence",
            Some("verbatim payload"),
            "verbatim payload",
            Some(ClipboardPayloadMarker {
                sequence_number: None,
            }),
            ClipboardPayloadMarker {
                sequence_number: None,
            },
            true,
        ),
    ]
}

fn clipboard_still_contains_verbatim_payload(
    app_handle: &AppHandle,
    payload: &str,
    expected_marker: Option<ClipboardPayloadMarker>,
) -> bool {
    let current_text = app_handle.clipboard().read_text().ok();
    clipboard_payload_owned_by_verbatim(
        current_text.as_deref(),
        payload,
        expected_marker,
        ClipboardPayloadMarker::capture_current(),
    )
}

fn clipboard_payload_wait_timeout(paste_delay_ms: u64) -> Duration {
    Duration::from_millis(paste_delay_ms.max(CLIPBOARD_PAYLOAD_POLL_INTERVAL_MS))
}

fn wait_until_clipboard_owns_payload(
    app_handle: &AppHandle,
    payload: &str,
    expected_marker: Option<ClipboardPayloadMarker>,
    paste_delay_ms: u64,
    is_cancelled: CancellationCheck<'_>,
) -> Result<(), String> {
    let timeout = clipboard_payload_wait_timeout(paste_delay_ms);
    let started = Instant::now();

    loop {
        ensure_not_cancelled(is_cancelled, "clipboard payload ownership")?;

        if clipboard_still_contains_verbatim_payload(app_handle, payload, expected_marker) {
            return Ok(());
        }

        if started.elapsed() >= timeout {
            return Err(format!(
                "Clipboard did not report Verbatim paste payload within {}ms",
                timeout.as_millis()
            ));
        }

        let remaining = timeout.saturating_sub(started.elapsed());
        std::thread::sleep(
            remaining.min(Duration::from_millis(CLIPBOARD_PAYLOAD_POLL_INTERVAL_MS)),
        );
    }
}

pub(crate) fn copy_text_for_recovery(
    app_handle: &AppHandle,
    text: &str,
    reason: &str,
) -> Result<(), String> {
    write_text_clipboard(app_handle, text)
        .map_err(|err| format!("failed to copy recovery text after {reason}: {err}"))
}

pub(crate) fn paste_exact_preserving_clipboard_with_cancellation(
    text: &str,
    app_handle: &AppHandle,
    is_cancelled: CancellationCheck<'_>,
) -> Result<(), String> {
    let settings = get_settings(app_handle);
    let paste_method = settings.paste_method;
    let paste_delay_ms = settings.paste_delay_ms;

    info!(
        "Using exact replacement paste method: {:?}, delay: {}ms",
        paste_method, paste_delay_ms
    );

    let enigo_state = app_handle
        .try_state::<EnigoState>()
        .ok_or("Enigo state not initialized")?;
    let mut enigo = enigo_state
        .0
        .lock()
        .map_err(|e| format!("Failed to lock Enigo: {}", e))?;

    match paste_method {
        PasteMethod::None => Err("PasteMethod::None cannot replace selected text".to_string()),
        PasteMethod::Direct => {
            ensure_not_cancelled(is_cancelled, "direct typing")?;
            paste_direct(
                &mut enigo,
                text,
                #[cfg(target_os = "linux")]
                settings.typing_tool,
            )
        }
        PasteMethod::CtrlV | PasteMethod::CtrlShiftV | PasteMethod::ShiftInsert => {
            let session = paste_via_clipboard(
                &mut enigo,
                text,
                app_handle,
                &paste_method,
                paste_delay_ms,
                None,
                is_cancelled,
            )?;
            std::thread::sleep(Duration::from_millis(50));
            drop(session);
            Ok(())
        }
        PasteMethod::ExternalScript => {
            let script_path = settings
                .external_script_path
                .as_ref()
                .filter(|p| !p.is_empty())
                .ok_or("External script path is not configured")?;
            paste_via_external_script(text, script_path, is_cancelled)
        }
    }
}

/// Pastes text using the clipboard: saves current content, writes text, sends paste keystroke.
/// The returned session restores the original clipboard on drop.
fn paste_via_clipboard(
    enigo: &mut Enigo,
    text: &str,
    app_handle: &AppHandle,
    paste_method: &PasteMethod,
    paste_delay_ms: u64,
    expected_target: Option<&str>,
    is_cancelled: CancellationCheck<'_>,
) -> Result<ClipboardPasteSession, String> {
    let clipboard_snapshot = ClipboardSnapshot::capture(app_handle);

    // Write text to clipboard first
    ensure_not_cancelled(is_cancelled, "clipboard write")?;
    write_text_clipboard(app_handle, text)?;
    let payload_marker = ClipboardPayloadMarker::capture_current();
    let session = ClipboardPasteSession::new(app_handle, text, clipboard_snapshot, payload_marker);

    if let Err(err) = wait_until_clipboard_owns_payload(
        app_handle,
        text,
        Some(payload_marker),
        paste_delay_ms,
        is_cancelled,
    ) {
        return Err(err);
    }

    ensure_not_cancelled(is_cancelled, "clipboard paste")?;
    ensure_dispatch_target(expected_target)?;

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

    Ok(session)
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

struct ExternalScriptInvocation<'a> {
    program: &'a str,
    args: Vec<&'a str>,
    stdin: &'a [u8],
}

fn build_external_script_invocation<'a>(
    script_path: &'a str,
    text: &'a str,
) -> ExternalScriptInvocation<'a> {
    ExternalScriptInvocation {
        program: script_path,
        args: Vec::new(),
        stdin: text.as_bytes(),
    }
}

fn external_script_failure_message(script_path: &str, code: Option<i32>) -> String {
    format!(
        "External script '{}' failed with exit code {:?}",
        script_path, code
    )
}

fn wait_for_external_script(
    child: &mut Child,
    script_path: &str,
    is_cancelled: CancellationCheck<'_>,
) -> Result<ExitStatus, String> {
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("Failed to poll external script '{}': {}", script_path, e))?
        {
            return Ok(status);
        }

        if let Err(err) = ensure_not_cancelled(is_cancelled, "external script completion") {
            kill_external_script_child(child, script_path);
            return Err(err);
        }

        std::thread::sleep(Duration::from_millis(25));
    }
}

fn kill_external_script_child(child: &mut Child, script_path: &str) {
    if let Err(kill_err) = child.kill() {
        warn!(
            "Failed to kill cancelled external script '{}': {}",
            script_path, kill_err
        );
    }
    let _ = child.wait();
}

/// Pastes text by invoking an external script.
/// The script receives the text to paste on stdin.
fn paste_via_external_script(
    text: &str,
    script_path: &str,
    is_cancelled: CancellationCheck<'_>,
) -> Result<(), String> {
    use std::io::Write;

    info!("Pasting via external script: {}", script_path);
    ensure_not_cancelled(is_cancelled, "external script invocation")?;

    let invocation = build_external_script_invocation(script_path, text);
    let mut child = Command::new(invocation.program)
        .args(&invocation.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to execute external script '{}': {}", script_path, e))?;

    if let Err(err) = ensure_not_cancelled(is_cancelled, "external script stdin write") {
        kill_external_script_child(&mut child, script_path);
        return Err(err);
    }
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("Failed to open stdin for external script '{}'", script_path))?;
    stdin.write_all(invocation.stdin).map_err(|e| {
        format!(
            "Failed to write to external script '{}': {}",
            script_path, e
        )
    })?;
    drop(stdin);

    let status = wait_for_external_script(&mut child, script_path, is_cancelled)?;
    if !status.success() {
        return Err(external_script_failure_message(script_path, status.code()));
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

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PasteVerification {
    Landed,
    /// Target was readable and the payload is demonstrably absent.
    NotFound,
    /// Target exposes no readable text; keep legacy sent==success behavior.
    Unsupported,
    /// Target was readable but inconclusive; never fabricate a failure.
    Unverified,
}

fn normalize_for_verification(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}'))
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn verification_needle(payload: &str) -> String {
    let normalized = normalize_for_verification(payload);
    let chars = normalized.chars().collect::<Vec<_>>();
    if chars.len() > 120 {
        chars[chars.len() - 60..].iter().collect()
    } else {
        normalized
    }
}

fn verify_paste_outcome(
    before: Option<&FocusedTextSnapshot>,
    after: Option<&FocusedTextSnapshot>,
    payload: &str,
) -> PasteVerification {
    let (Some(before), Some(after)) = (before, after) else {
        return if before.is_none() && after.is_none() {
            PasteVerification::Unsupported
        } else {
            PasteVerification::Unverified
        };
    };

    if before.target_id != after.target_id {
        return PasteVerification::Unverified;
    }

    let needle = verification_needle(payload);
    if !needle.is_empty() && normalize_for_verification(&after.text).contains(&needle) {
        return PasteVerification::Landed;
    }

    if after.text.chars().count() >= FOCUSED_TEXT_READ_CAP {
        return PasteVerification::Unverified;
    }

    PasteVerification::NotFound
}

fn wait_for_paste_landing(
    before: Option<&FocusedTextSnapshot>,
    payload: &str,
) -> PasteVerification {
    let deadline = Instant::now() + Duration::from_millis(PASTE_VERIFY_TOTAL_MS);
    loop {
        let after = crate::post_paste_learning::capture_focused_text_snapshot();
        let outcome = verify_paste_outcome(before, after.as_ref(), payload);
        match outcome {
            PasteVerification::NotFound if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(PASTE_VERIFY_POLL_MS));
            }
            outcome => return outcome,
        }
    }
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

fn should_dispatch_paste(expected_target: Option<&str>, current_target: Option<&str>) -> bool {
    expected_target.is_none() || expected_target == current_target
}

fn target_still_focused(expected_target: &str) -> bool {
    let current_context = crate::adaptive::context::capture_context(&[]);
    should_dispatch_paste(
        Some(expected_target),
        current_context.target_fingerprint.as_deref(),
    )
}

fn ensure_dispatch_target(expected_target: Option<&str>) -> Result<(), String> {
    if let Some(expected_target) = expected_target {
        if !target_still_focused(expected_target) {
            warn!("Paste skipped because the foreground target changed before insertion");
            return Err(TARGET_CHANGED_BEFORE_INSERTION.to_string());
        }
    }
    Ok(())
}

fn is_clipboard_paste_method(paste_method: PasteMethod) -> bool {
    matches!(
        paste_method,
        PasteMethod::CtrlV | PasteMethod::CtrlShiftV | PasteMethod::ShiftInsert
    )
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
    let target_changed_before_dispatch =
        matches!(result.as_ref(), Err(error) if error == TARGET_CHANGED_BEFORE_INSERTION);
    let attempted = paste_method != PasteMethod::None && !target_changed_before_dispatch;
    let method = if target_changed_before_dispatch {
        InsertionMethod::None
    } else {
        insertion_method_for_paste_method(paste_method)
    };
    let target_verified = target_verified && !target_changed_before_dispatch;
    match result {
        Ok(()) => InsertionReceipt {
            attempted,
            succeeded: true,
            method,
            target_verified,
            error: None,
        },
        Err(error) => InsertionReceipt {
            attempted,
            succeeded: false,
            method,
            target_verified,
            error: Some(error),
        },
    }
}

pub(crate) fn receipt_from_current_paste_method(
    app_handle: &AppHandle,
    target_verified: bool,
    result: Result<(), String>,
) -> InsertionReceipt {
    let settings = get_settings(app_handle);
    receipt_from_result(settings.paste_method, target_verified, result)
}

#[allow(dead_code)]
pub fn paste(text: String, app_handle: AppHandle) -> Result<(), String> {
    paste_with_auto_learn(text, app_handle, None, true, None)
}

fn paste_with_auto_learn(
    text: String,
    app_handle: AppHandle,
    expected_target: Option<&str>,
    auto_learn_eligible: bool,
    is_cancelled: CancellationCheck<'_>,
) -> Result<(), String> {
    let settings = get_settings(&app_handle);
    let paste_method = settings.paste_method;
    let paste_delay_ms = settings.paste_delay_ms;
    let private_session_enabled = crate::private_session::is_enabled(&app_handle);

    // Append trailing space if setting is enabled
    let text = if settings.append_trailing_space {
        format!("{} ", text)
    } else {
        text
    };

    let should_capture_for_auto_learn = auto_learn_eligible
        && !private_session_enabled
        && settings.auto_add_dictionary_words
        && paste_method != PasteMethod::None;
    let should_capture_for_verification = is_clipboard_paste_method(paste_method);
    let before_paste_snapshot = if should_capture_for_auto_learn || should_capture_for_verification
    {
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
    let clipboard_session = match paste_method {
        PasteMethod::None => {
            info!("PasteMethod::None selected - skipping paste action");
            None
        }
        PasteMethod::Direct => {
            ensure_not_cancelled(is_cancelled, "direct typing")?;
            ensure_dispatch_target(expected_target)?;
            paste_direct(
                &mut enigo,
                &text,
                #[cfg(target_os = "linux")]
                settings.typing_tool,
            )?;
            None
        }
        PasteMethod::CtrlV | PasteMethod::CtrlShiftV | PasteMethod::ShiftInsert => {
            Some(paste_via_clipboard(
                &mut enigo,
                &text,
                &app_handle,
                &paste_method,
                paste_delay_ms,
                expected_target,
                is_cancelled,
            )?)
        }
        PasteMethod::ExternalScript => {
            let script_path = settings
                .external_script_path
                .as_ref()
                .filter(|p| !p.is_empty())
                .ok_or("External script path is not configured")?;
            ensure_not_cancelled(is_cancelled, "external script")?;
            ensure_dispatch_target(expected_target)?;
            paste_via_external_script(&text, script_path, is_cancelled)?;
            None
        }
    };
    let verification = if clipboard_session.is_some() {
        wait_for_paste_landing(before_paste_snapshot.as_ref(), &text)
    } else {
        PasteVerification::Unsupported
    };
    drop(clipboard_session);

    if verification == PasteVerification::NotFound {
        info!("Skipping auto-submit: paste not verified");
        warn!("Paste keystroke sent but payload not observed in focused element");
        return Err("Paste keystroke sent but payload not observed in focused element".to_string());
    }

    if should_send_auto_submit(settings.auto_submit, paste_method) {
        std::thread::sleep(Duration::from_millis(settings.paste_delay_ms.max(50)));
        ensure_not_cancelled(is_cancelled, "auto-submit")?;
        send_return_key(&mut enigo, settings.auto_submit_key)?;
    }

    // After pasting, optionally copy to clipboard based on settings
    if settings.clipboard_handling == ClipboardHandling::CopyToClipboard {
        ensure_not_cancelled(is_cancelled, "copy-to-clipboard")?;
        let clipboard = app_handle.clipboard();
        clipboard
            .write_text(&text)
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;
    }

    if should_capture_for_auto_learn {
        crate::post_paste_learning::maybe_spawn_auto_add_watcher(
            app_handle.clone(),
            text,
            before_paste_snapshot,
        );
    }

    Ok(())
}

#[allow(dead_code)]
pub fn paste_with_receipt(
    text: String,
    app_handle: AppHandle,
    target_verified: bool,
) -> InsertionReceipt {
    paste_with_receipt_with_auto_learn(text, app_handle, target_verified, None, true)
}

pub fn paste_with_receipt_with_auto_learn(
    text: String,
    app_handle: AppHandle,
    target_verified: bool,
    expected_target: Option<String>,
    auto_learn_eligible: bool,
) -> InsertionReceipt {
    paste_with_receipt_with_auto_learn_and_cancellation(
        text,
        app_handle,
        target_verified,
        expected_target,
        auto_learn_eligible,
        None,
    )
}

pub fn paste_with_receipt_with_auto_learn_and_cancellation(
    text: String,
    app_handle: AppHandle,
    target_verified: bool,
    expected_target: Option<String>,
    auto_learn_eligible: bool,
    is_cancelled: CancellationCheck<'_>,
) -> InsertionReceipt {
    let settings = get_settings(&app_handle);
    let paste_method = settings.paste_method;
    let result = paste_with_auto_learn(
        text,
        app_handle,
        expected_target.as_deref(),
        auto_learn_eligible,
        is_cancelled,
    );
    receipt_from_result(paste_method, target_verified, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn restore_guard_restores_on_drop_when_armed() {
        let restored = Cell::new(false);
        {
            let _guard = RestoreOnDrop::new(|| restored.set(true));
        }
        assert!(restored.get());
    }

    #[test]
    fn restore_guard_does_not_restore_after_disarm() {
        let restored = Cell::new(false);
        {
            let mut guard = RestoreOnDrop::new(|| restored.set(true));
            guard.disarm();
        }
        assert!(!restored.get());
    }

    #[test]
    fn paste_gate_decision_matrix() {
        assert!(should_dispatch_paste(None, None));
        assert!(should_dispatch_paste(Some("a"), Some("a")));
        assert!(!should_dispatch_paste(Some("a"), Some("b")));
        assert!(!should_dispatch_paste(Some("a"), None));
    }

    mod paste_verify_tests {
        use super::*;
        use crate::post_paste_learning::FocusedTextSnapshot;

        fn snap(target_id: &str, text: &str) -> FocusedTextSnapshot {
            FocusedTextSnapshot {
                target_id: target_id.to_string(),
                text: text.to_string(),
            }
        }

        #[test]
        fn verification_passes_when_payload_present() {
            assert_eq!(
                verify_paste_outcome(
                    Some(&snap("a", "hello")),
                    Some(&snap("a", "hello dictated text")),
                    "dictated text"
                ),
                PasteVerification::Landed
            );
        }

        #[test]
        fn same_target_unchanged_is_not_found() {
            assert_eq!(
                verify_paste_outcome(
                    Some(&snap("a", "hello world")),
                    Some(&snap("a", "hello world")),
                    "dictated text"
                ),
                PasteVerification::NotFound
            );
        }

        #[test]
        fn target_changed_between_reads_is_unverified() {
            assert_eq!(
                verify_paste_outcome(
                    Some(&snap("a", "x")),
                    Some(&snap("b", "y")),
                    "dictated text"
                ),
                PasteVerification::Unverified
            );
        }

        #[test]
        fn unreadable_target_is_unsupported() {
            assert_eq!(
                verify_paste_outcome(None, None, "dictated text"),
                PasteVerification::Unsupported
            );
        }

        #[test]
        fn readable_before_unreadable_after_is_unverified() {
            assert_eq!(
                verify_paste_outcome(Some(&snap("a", "x")), None, "dictated text"),
                PasteVerification::Unverified
            );
        }

        #[test]
        fn verification_ignores_direction_marks_and_whitespace() {
            assert_eq!(
                verify_paste_outcome(
                    Some(&snap("a", "x")),
                    Some(&snap("a", "x \u{200E}dictated\u{00A0}text")),
                    "dictated text"
                ),
                PasteVerification::Landed
            );
        }

        #[test]
        fn truncated_read_is_unverified_not_failure() {
            let capped = "z".repeat(FOCUSED_TEXT_READ_CAP);
            assert_eq!(
                verify_paste_outcome(
                    Some(&snap("a", "x")),
                    Some(&snap("a", &capped)),
                    "dictated text"
                ),
                PasteVerification::Unverified
            );
        }
    }

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
    fn cancellation_guard_blocks_side_effect_stage() {
        let cancelled = || true;

        let err = ensure_not_cancelled(Some(&cancelled), "clipboard write")
            .expect_err("cancelled operation must block side effect");

        assert!(err.contains("clipboard write"));
    }

    #[test]
    fn cancellation_guard_allows_uncancelled_or_unguarded_stage() {
        let active = || false;

        ensure_not_cancelled(Some(&active), "direct typing").expect("active operation");
        ensure_not_cancelled(None, "direct typing").expect("unguarded operation");
    }

    #[test]
    fn external_script_invocation_sends_text_on_stdin_not_argv() {
        let invocation = build_external_script_invocation("paste-helper", "sensitive text");

        assert_eq!(invocation.program, "paste-helper");
        assert!(invocation.args.is_empty());
        assert_eq!(invocation.stdin, b"sensitive text");
    }

    #[test]
    fn external_script_failure_message_redacts_script_output() {
        let message = external_script_failure_message("paste-helper", Some(7));

        assert_eq!(
            message,
            "External script 'paste-helper' failed with exit code Some(7)"
        );
        assert!(!message.contains("stdout"));
        assert!(!message.contains("stderr"));
        assert!(!message.contains("sensitive text"));
    }

    #[test]
    fn clipboard_restore_requires_own_payload() {
        assert!(clipboard_payload_owned_by_verbatim(
            Some("temporary payload"),
            "temporary payload",
            None,
            ClipboardPayloadMarker {
                sequence_number: None
            }
        ));
        assert!(!clipboard_payload_owned_by_verbatim(
            Some("user copied something else"),
            "temporary payload",
            None,
            ClipboardPayloadMarker {
                sequence_number: None
            }
        ));
        assert!(!clipboard_payload_owned_by_verbatim(
            None,
            "temporary payload",
            None,
            ClipboardPayloadMarker {
                sequence_number: None
            }
        ));
    }

    #[test]
    fn native_smoke_clipboard_safety_drill_covers_mutation_cases() {
        let cases = native_smoke_clipboard_safety_drill();

        assert_eq!(cases.len(), 3);
        assert!(cases.iter().all(|case| case.passed));
        assert!(cases
            .iter()
            .any(|case| { case.case == "same_text_sequence_changed" && !case.owned_by_verbatim }));
        assert!(cases.iter().any(|case| {
            case.case == "changed_text_matching_sequence" && !case.owned_by_verbatim
        }));
        assert!(cases
            .iter()
            .any(|case| { case.case == "exact_text_without_sequence" && case.owned_by_verbatim }));
    }

    #[test]
    fn clipboard_restore_accepts_matching_platform_sequence_marker() {
        let marker = ClipboardPayloadMarker {
            sequence_number: Some(42),
        };

        assert!(clipboard_payload_owned_by_verbatim(
            Some("temporary payload"),
            "temporary payload",
            Some(marker),
            marker
        ));
    }

    #[test]
    fn clipboard_restore_rejects_same_text_after_platform_sequence_change() {
        let expected_marker = ClipboardPayloadMarker {
            sequence_number: Some(42),
        };
        let current_marker = ClipboardPayloadMarker {
            sequence_number: Some(43),
        };

        assert!(!clipboard_payload_owned_by_verbatim(
            Some("temporary payload"),
            "temporary payload",
            Some(expected_marker),
            current_marker
        ));
    }

    #[test]
    fn clipboard_restore_rejects_changed_text_even_with_matching_sequence() {
        let marker = ClipboardPayloadMarker {
            sequence_number: Some(42),
        };

        assert!(!clipboard_payload_owned_by_verbatim(
            Some("user replacement"),
            "temporary payload",
            Some(marker),
            marker
        ));
    }

    #[test]
    fn clipboard_restore_falls_back_to_exact_text_when_sequence_is_unavailable() {
        let marker_without_sequence = ClipboardPayloadMarker {
            sequence_number: None,
        };

        assert!(clipboard_payload_owned_by_verbatim(
            Some("temporary payload"),
            "temporary payload",
            Some(marker_without_sequence),
            marker_without_sequence
        ));
    }

    #[test]
    fn clipboard_payload_wait_timeout_uses_user_delay_with_minimum_poll_window() {
        assert_eq!(
            clipboard_payload_wait_timeout(0),
            Duration::from_millis(CLIPBOARD_PAYLOAD_POLL_INTERVAL_MS)
        );
        assert_eq!(
            clipboard_payload_wait_timeout(75),
            Duration::from_millis(75)
        );
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
    fn native_snapshot_only_reads_memory_backed_formats() {
        assert!(is_memory_backed_clipboard_format(1));
        assert!(is_memory_backed_clipboard_format(13));
        assert!(is_memory_backed_clipboard_format(15));
        assert!(is_memory_backed_clipboard_format(17));
        assert!(is_memory_backed_clipboard_format(0xC000));
        assert!(is_memory_backed_clipboard_format(0xC123));

        assert!(!is_memory_backed_clipboard_format(2));
        assert!(!is_memory_backed_clipboard_format(3));
        assert!(!is_memory_backed_clipboard_format(9));
        assert!(!is_memory_backed_clipboard_format(14));
        assert!(!is_memory_backed_clipboard_format(0x0200));
        assert!(!is_memory_backed_clipboard_format(0x0300));
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

    #[cfg(target_os = "windows")]
    #[test]
    fn native_restore_uses_image_payload_when_no_memory_formats() {
        let mut native_ops = FakeNativeRestoreOps::default();
        let mut desktop_ops = FakeDesktopClipboardOps::default();
        let image = DesktopClipboardPayload::Image {
            rgba: vec![255, 0, 0, 255],
            width: 1,
            height: 1,
        };

        restore_native_clipboard_snapshot_with_image_fallback(
            &[],
            Some(&image),
            &mut native_ops,
            &mut desktop_ops,
        )
        .unwrap();

        assert!(!native_ops.opened_and_emptied);
        assert_eq!(desktop_ops.writes, vec!["image"]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_restore_uses_image_payload_when_native_restore_fails() {
        let mut native_ops = FakeNativeRestoreOps {
            fail_set_format: Some(13),
            ..FakeNativeRestoreOps::default()
        };
        let mut desktop_ops = FakeDesktopClipboardOps::default();
        let image = DesktopClipboardPayload::Image {
            rgba: vec![255, 0, 0, 255],
            width: 1,
            height: 1,
        };

        restore_native_clipboard_snapshot_with_image_fallback(
            &test_formats(),
            Some(&image),
            &mut native_ops,
            &mut desktop_ops,
        )
        .unwrap();

        assert!(native_ops.opened_and_emptied);
        assert_eq!(native_ops.set_attempts, vec![13, 15]);
        assert_eq!(desktop_ops.writes, vec!["image"]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_restore_preserves_file_and_registered_rich_text_formats() {
        let mut ops = FakeNativeRestoreOps::default();
        let formats = vec![
            ClipboardFormatData {
                format: 15,
                bytes: b"file-drop-list".to_vec(),
            },
            ClipboardFormatData {
                format: 0xC001,
                bytes: b"{\\rtf1 rich text}".to_vec(),
            },
        ];

        restore_native_clipboard_snapshot_with_ops(&formats, &mut ops).unwrap();

        assert!(ops.opened_and_emptied);
        assert_eq!(ops.set_attempts, vec![15, 0xC001]);
        assert!(ops.freed_handles.is_empty());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn native_restore_reports_failure_when_no_snapshot_fallback_exists() {
        let mut native_ops = FakeNativeRestoreOps::default();
        let mut desktop_ops = FakeDesktopClipboardOps::default();

        let error = restore_native_clipboard_snapshot_with_image_fallback(
            &[],
            None,
            &mut native_ops,
            &mut desktop_ops,
        )
        .expect_err("empty snapshot without fallback should fail");

        assert_eq!(error, "native clipboard snapshot is empty");
        assert!(desktop_ops.writes.is_empty());
    }

    #[derive(Default)]
    struct FakeDesktopClipboardOps {
        image: Option<(Vec<u8>, u32, u32)>,
        text: Option<String>,
        writes: Vec<&'static str>,
    }

    impl DesktopClipboardOps for FakeDesktopClipboardOps {
        fn read_image_payload(&self) -> Result<DesktopClipboardPayload, String> {
            let Some((rgba, width, height)) = &self.image else {
                return Err("no image".to_string());
            };
            Ok(DesktopClipboardPayload::Image {
                rgba: rgba.clone(),
                width: *width,
                height: *height,
            })
        }

        fn read_text_payload(&self) -> Result<DesktopClipboardPayload, String> {
            let Some(text) = &self.text else {
                return Err("no text".to_string());
            };
            Ok(DesktopClipboardPayload::Text(text.clone()))
        }

        fn write_image_payload(
            &mut self,
            _rgba: &[u8],
            _width: u32,
            _height: u32,
        ) -> Result<(), String> {
            self.writes.push("image");
            Ok(())
        }

        fn write_text_payload(&mut self, _text: &str) -> Result<(), String> {
            self.writes.push("text");
            Ok(())
        }
    }

    #[test]
    fn desktop_clipboard_capture_prefers_image_payload_over_text() {
        let ops = FakeDesktopClipboardOps {
            image: Some((vec![255, 0, 0, 255], 1, 1)),
            text: Some("fallback text".to_string()),
            writes: Vec::new(),
        };

        let snapshot = capture_desktop_clipboard_payload(&ops).unwrap();

        assert_eq!(
            snapshot,
            DesktopClipboardPayload::Image {
                rgba: vec![255, 0, 0, 255],
                width: 1,
                height: 1,
            }
        );
    }

    #[test]
    fn desktop_clipboard_capture_reads_text_when_no_image_exists() {
        let ops = FakeDesktopClipboardOps {
            image: None,
            text: Some("plain text".to_string()),
            writes: Vec::new(),
        };

        let snapshot = capture_desktop_clipboard_payload(&ops).unwrap();

        assert_eq!(
            snapshot,
            DesktopClipboardPayload::Text("plain text".to_string())
        );
    }

    #[test]
    fn desktop_clipboard_capture_rejects_empty_text_placeholder() {
        let ops = FakeDesktopClipboardOps {
            image: None,
            text: Some(String::new()),
            writes: Vec::new(),
        };

        let error =
            capture_desktop_clipboard_payload(&ops).expect_err("empty text is not a snapshot");

        assert_eq!(error, "desktop clipboard text is empty");
    }

    #[test]
    fn desktop_clipboard_restore_uses_captured_payload_type() {
        let mut ops = FakeDesktopClipboardOps::default();
        restore_desktop_clipboard_payload(
            &DesktopClipboardPayload::Image {
                rgba: vec![255, 0, 0, 255],
                width: 1,
                height: 1,
            },
            &mut ops,
        )
        .unwrap();
        restore_desktop_clipboard_payload(
            &DesktopClipboardPayload::Text("hello".to_string()),
            &mut ops,
        )
        .unwrap();

        assert_eq!(ops.writes, vec!["image", "text"]);
    }
}
