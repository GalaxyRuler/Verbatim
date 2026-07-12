use enigo::{Enigo, Mouse, Settings};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use enigo::{Key, Keyboard};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Wrapper for Enigo to store in Tauri's managed state.
/// Enigo is wrapped in a Mutex since it requires mutable access.
pub struct EnigoState(pub Mutex<Enigo>);

#[cfg(not(any(target_os = "android", target_os = "ios")))]
trait KeySink {
    fn key(&mut self, key: Key, direction: enigo::Direction) -> Result<(), String>;
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl KeySink for Enigo {
    fn key(&mut self, key: Key, direction: enigo::Direction) -> Result<(), String> {
        Keyboard::key(self, key, direction).map_err(|e| e.to_string())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
struct PressedKeyGuard<'a, S: KeySink> {
    sink: &'a mut S,
    pressed: Vec<Key>,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl<'a, S: KeySink> PressedKeyGuard<'a, S> {
    fn new(sink: &'a mut S) -> Self {
        Self {
            sink,
            pressed: Vec::new(),
        }
    }

    fn press(&mut self, key: Key) -> Result<(), String> {
        self.sink
            .key(key, enigo::Direction::Press)
            .map_err(|error| format!("Failed to press key: {error}"))?;
        self.pressed.push(key);
        Ok(())
    }

    fn click(&mut self, key: Key) -> Result<(), String> {
        self.sink
            .key(key, enigo::Direction::Click)
            .map_err(|error| format!("Failed to click key: {error}"))
    }

    fn release_all(&mut self) -> Result<(), String> {
        let mut first_error = None;
        for key in self.pressed.drain(..).rev() {
            if let Err(error) = self.sink.key(key, enigo::Direction::Release) {
                first_error.get_or_insert_with(|| format!("Failed to release key: {error}"));
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl<S: KeySink> Drop for PressedKeyGuard<'_, S> {
    fn drop(&mut self) {
        for key in self.pressed.drain(..).rev() {
            let _ = self.sink.key(key, enigo::Direction::Release);
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn send_paste_chord<S: KeySink>(
    sink: &mut S,
    modifier_key: Key,
    paste_key: Key,
    with_shift: bool,
) -> Result<(), String> {
    let mut guard = PressedKeyGuard::new(sink);
    guard.press(modifier_key)?;
    if with_shift {
        guard.press(Key::Shift)?;
    }
    guard.click(paste_key)?;
    std::thread::sleep(std::time::Duration::from_millis(100));
    guard.release_all()
}

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}

/// Get the current mouse cursor position using the managed Enigo instance.
/// Returns None if the state is not available or if getting the location fails.
pub fn get_cursor_position(app_handle: &AppHandle) -> Option<(i32, i32)> {
    let enigo_state = app_handle.try_state::<EnigoState>()?;
    let enigo = enigo_state.0.lock().ok()?;
    enigo.location().ok()
}

/// Sends a Ctrl+V or Cmd+V paste command using platform-specific virtual key codes.
/// This ensures the paste works regardless of keyboard layout (e.g., Russian, AZERTY, DVORAK).
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn send_paste_ctrl_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9));
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    send_paste_chord(enigo, modifier_key, v_key_code, false)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn send_paste_ctrl_v(_enigo: &mut Enigo) -> Result<(), String> {
    Err("Desktop paste shortcuts are not supported on mobile".to_string())
}

/// Sends a Ctrl+Shift+V paste command.
/// This is commonly used in terminal applications on Linux to paste without formatting.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn send_paste_ctrl_shift_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9)); // Cmd+Shift+V on macOS
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    send_paste_chord(enigo, modifier_key, v_key_code, true)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn send_paste_ctrl_shift_v(_enigo: &mut Enigo) -> Result<(), String> {
    Err("Desktop paste shortcuts are not supported on mobile".to_string())
}

/// Sends a Shift+Insert paste command (Windows and Linux only).
/// This is more universal for terminal applications and legacy software.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn send_paste_shift_insert(enigo: &mut Enigo) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let insert_key_code = Key::Other(0x2D); // VK_INSERT
    #[cfg(not(target_os = "windows"))]
    let insert_key_code = Key::Other(0x76); // XK_Insert (keycode 118 / 0x76, also used as fallback)

    send_paste_chord(enigo, Key::Shift, insert_key_code, false)
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn send_paste_shift_insert(_enigo: &mut Enigo) -> Result<(), String> {
    Err("Desktop paste shortcuts are not supported on mobile".to_string())
}

/// Sets left-to-right reading order in Windows rich edit controls.
#[cfg(target_os = "windows")]
pub fn send_ltr_reading_order(enigo: &mut Enigo) -> Result<(), String> {
    const VK_LSHIFT: u32 = 0xA0;

    Keyboard::key(enigo, Key::Control, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Control key: {}", e))?;
    Keyboard::key(enigo, Key::Other(VK_LSHIFT), enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Left Shift key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(20));

    Keyboard::key(enigo, Key::Other(VK_LSHIFT), enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Left Shift key: {}", e))?;
    Keyboard::key(enigo, Key::Control, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Control key: {}", e))?;

    Ok(())
}

/// Pastes text directly using the enigo text method.
/// This tries to use system input methods if possible, otherwise simulates keystrokes one by one.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn paste_text_direct(enigo: &mut Enigo, text: &str) -> Result<(), String> {
    enigo
        .text(text)
        .map_err(|e| format!("Failed to send text directly: {}", e))?;

    Ok(())
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn paste_text_direct(_enigo: &mut Enigo, _text: &str) -> Result<(), String> {
    Err("Desktop direct text injection is not supported on mobile".to_string())
}

#[cfg(all(test, not(any(target_os = "android", target_os = "ios"))))]
mod tests {
    use super::*;

    struct FaultInjectingSink {
        calls: Vec<(Key, enigo::Direction)>,
        fail_at: usize,
        next_call: usize,
    }

    impl KeySink for FaultInjectingSink {
        fn key(&mut self, key: Key, direction: enigo::Direction) -> Result<(), String> {
            let call = self.next_call;
            self.next_call += 1;
            if call == self.fail_at {
                return Err("injected enigo failure".to_string());
            }
            self.calls.push((key, direction));
            Ok(())
        }
    }

    fn assert_failed_chord_releases_pressed_modifiers(
        modifier: Key,
        paste_key: Key,
        with_shift: bool,
        fail_at: usize,
    ) {
        let mut sink = FaultInjectingSink {
            calls: Vec::new(),
            fail_at,
            next_call: 0,
        };

        assert!(send_paste_chord(&mut sink, modifier, paste_key, with_shift).is_err());
        assert!(sink
            .calls
            .iter()
            .any(|(key, direction)| *key == modifier && *direction == enigo::Direction::Release));
        if with_shift {
            assert!(sink
                .calls
                .iter()
                .any(|(key, direction)| *key == Key::Shift
                    && *direction == enigo::Direction::Release));
        }
    }

    #[test]
    fn ctrl_or_cmd_v_failure_releases_modifier() {
        assert_failed_chord_releases_pressed_modifiers(Key::Control, Key::Unicode('v'), false, 1);
        assert_failed_chord_releases_pressed_modifiers(Key::Meta, Key::Unicode('v'), false, 1);
    }

    #[test]
    fn ctrl_or_cmd_shift_v_failure_releases_both_modifiers() {
        assert_failed_chord_releases_pressed_modifiers(Key::Control, Key::Unicode('v'), true, 2);
        assert_failed_chord_releases_pressed_modifiers(Key::Meta, Key::Unicode('v'), true, 2);
    }

    #[test]
    fn shift_insert_failure_releases_shift() {
        assert_failed_chord_releases_pressed_modifiers(Key::Shift, Key::Other(0x2D), false, 1);
    }
}
