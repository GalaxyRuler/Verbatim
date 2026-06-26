//! Windows low-level keyboard hook implementation

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL,
    MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, MSG, MSLLHOOKSTRUCT,
    PM_REMOVE, QS_ALLINPUT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP,
    WM_SYSKEYDOWN, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use crate::error::Result;
use crate::platform::state::BlockingHotkeys;
use crate::types::{Hotkey, Key, KeyEvent, Modifiers};

use super::keycode::{vk_from_key, vk_to_key, vk_to_modifier};

const HOOK_LOOP_TIMEOUT_MS: u32 = 10;
const WM_HOTKEY_DEDUP_MS: u64 = 50;
const MAX_HOTKEY_ID: i32 = 0xBFFF;

/// Thread-local state for the keyboard hook callback.
///
/// Windows low-level hooks require a callback function with a specific signature,
/// so we use thread-local storage to access our state from within the callback.
struct HookContext {
    event_sender: Sender<KeyEvent>,
    current_modifiers: Modifiers,
    blocking_hotkeys: Option<BlockingHotkeys>,
    last_ll_fire_ms: Arc<AtomicU64>,
}

#[derive(Default)]
struct RegisteredHotkeyState {
    desired: HashSet<Hotkey>,
    registered: HashMap<i32, Hotkey>,
}

#[derive(Default)]
struct ModifierOnlyPoller {
    hotkeys: Vec<Hotkey>,
    desired: HashSet<Hotkey>,
    prev_active: HashMap<u64, bool>,
}

impl ModifierOnlyPoller {
    fn poll(&mut self, sender: &Sender<KeyEvent>, last_ll_fire_ms: &AtomicU64) {
        let now_ms = unsafe { GetTickCount64() };
        self.poll_with_combo_state(sender, last_ll_fire_ms, now_ms, all_combo_keys_down);
    }

    #[cfg(test)]
    fn poll_with_key_state<F>(
        &mut self,
        sender: &Sender<KeyEvent>,
        last_ll_fire_ms: &AtomicU64,
        now_ms: u64,
        mut is_down: F,
    ) where
        F: FnMut(u16) -> bool,
    {
        self.poll_with_combo_state(sender, last_ll_fire_ms, now_ms, |hotkey| {
            all_combo_keys_down_with(hotkey, &mut is_down)
        });
    }

    fn poll_with_combo_state<F>(
        &mut self,
        sender: &Sender<KeyEvent>,
        last_ll_fire_ms: &AtomicU64,
        now_ms: u64,
        mut is_combo_down: F,
    ) where
        F: FnMut(Hotkey) -> bool,
    {
        for hotkey in &self.hotkeys {
            let bits = u64::from(hotkey.modifiers.bits());
            let is_active = is_combo_down(*hotkey);
            let was_active = *self.prev_active.get(&bits).unwrap_or(&false);

            if is_active && !was_active {
                let last_ll_fire = last_ll_fire_ms.load(Ordering::Relaxed);
                if last_ll_fire == 0 || now_ms.saturating_sub(last_ll_fire) >= WM_HOTKEY_DEDUP_MS {
                    let _ = sender.send(KeyEvent {
                        modifiers: hotkey.modifiers,
                        key: None,
                        is_key_down: true,
                        changed_modifier: None,
                    });
                }
            } else if !is_active && was_active {
                let _ = sender.send(KeyEvent {
                    modifiers: hotkey.modifiers,
                    key: None,
                    is_key_down: false,
                    changed_modifier: None,
                });
            }

            self.prev_active.insert(bits, is_active);
        }
    }
}

thread_local! {
    static HOOK_CONTEXT: std::cell::RefCell<Option<HookContext>> = const { std::cell::RefCell::new(None) };
}

/// Drain all pending thread messages and return `true` if WM_QUIT was received.
fn drain_thread_messages(
    msg: &mut MSG,
    registered_hotkeys: &HashMap<i32, Hotkey>,
    event_sender: &Sender<KeyEvent>,
    last_ll_fire_ms: &AtomicU64,
) -> bool {
    unsafe {
        while PeekMessageW(msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_QUIT {
                return true;
            }
            if msg.message == WM_HOTKEY {
                handle_registered_hotkey_message(
                    msg,
                    registered_hotkeys,
                    event_sender,
                    last_ll_fire_ms,
                );
                continue;
            }
            let _ = TranslateMessage(msg);
            DispatchMessageW(msg);
        }
    }
    false
}

fn handle_registered_hotkey_message(
    msg: &MSG,
    registered_hotkeys: &HashMap<i32, Hotkey>,
    event_sender: &Sender<KeyEvent>,
    last_ll_fire_ms: &AtomicU64,
) {
    let id = msg.wParam.0 as i32;
    let Some(hotkey) = registered_hotkeys.get(&id) else {
        return;
    };

    let now = unsafe { GetTickCount64() };
    let last_ll_fire = last_ll_fire_ms.load(Ordering::Relaxed);
    if last_ll_fire != 0 && now.saturating_sub(last_ll_fire) < WM_HOTKEY_DEDUP_MS {
        return;
    }

    let _ = event_sender.send(KeyEvent {
        modifiers: hotkey.modifiers,
        key: hotkey.key,
        is_key_down: true,
        changed_modifier: None,
    });
}

/// Wait for new input/messages or until timeout expires.
fn wait_for_message_or_timeout(timeout_ms: u32) {
    unsafe {
        let _ = MsgWaitForMultipleObjects(None, false, timeout_ms, QS_ALLINPUT);
    }
}

fn sync_modifier_only_poller(
    blocking_hotkeys: &Option<BlockingHotkeys>,
    poller: &mut ModifierOnlyPoller,
) {
    let Some(desired) = snapshot_modifier_only_hotkeys(blocking_hotkeys) else {
        return;
    };

    if desired == poller.desired {
        return;
    }

    poller.desired = desired.clone();

    let mut hotkeys: Vec<Hotkey> = desired.into_iter().collect();
    hotkeys.sort_by_key(|hotkey| hotkey.to_string());
    poller.hotkeys = hotkeys;

    let active_bits = poller
        .desired
        .iter()
        .map(|hotkey| u64::from(hotkey.modifiers.bits()))
        .collect::<HashSet<_>>();
    poller
        .prev_active
        .retain(|bits, _| active_bits.contains(bits));
}

fn snapshot_modifier_only_hotkeys(
    blocking_hotkeys: &Option<BlockingHotkeys>,
) -> Option<HashSet<Hotkey>> {
    let Some(hotkeys) = blocking_hotkeys else {
        return Some(HashSet::new());
    };

    let hotkeys = hotkeys.lock().ok()?;
    Some(
        hotkeys
            .iter()
            .copied()
            .filter(|hotkey| is_modifier_only_pollable(*hotkey))
            .collect(),
    )
}

fn is_modifier_only_pollable(hotkey: Hotkey) -> bool {
    hotkey.key.is_none()
        && !hotkey.modifiers.is_empty()
        && !hotkey.modifiers.contains(Modifiers::FN)
        && !modifier_vks(hotkey.modifiers).is_empty()
}

fn modifier_vks(modifiers: Modifiers) -> Vec<u16> {
    let mut vks = Vec::new();

    push_modifier_group_vks(
        &mut vks,
        modifiers,
        Modifiers::CTRL_LEFT,
        Modifiers::CTRL_RIGHT,
        VK_LCONTROL.0,
        VK_RCONTROL.0,
        Some(VK_CONTROL.0),
    );
    push_modifier_group_vks(
        &mut vks,
        modifiers,
        Modifiers::SHIFT_LEFT,
        Modifiers::SHIFT_RIGHT,
        VK_LSHIFT.0,
        VK_RSHIFT.0,
        Some(VK_SHIFT.0),
    );
    push_modifier_group_vks(
        &mut vks,
        modifiers,
        Modifiers::OPT_LEFT,
        Modifiers::OPT_RIGHT,
        VK_LMENU.0,
        VK_RMENU.0,
        Some(VK_MENU.0),
    );
    push_modifier_group_vks(
        &mut vks,
        modifiers,
        Modifiers::CMD_LEFT,
        Modifiers::CMD_RIGHT,
        VK_LWIN.0,
        VK_RWIN.0,
        None,
    );

    vks
}

fn push_modifier_group_vks(
    vks: &mut Vec<u16>,
    modifiers: Modifiers,
    left: Modifiers,
    right: Modifiers,
    left_vk: u16,
    right_vk: u16,
    generic_vk: Option<u16>,
) {
    let has_left = modifiers.contains(left);
    let has_right = modifiers.contains(right);

    if has_left && has_right {
        if let Some(vk) = generic_vk {
            vks.push(vk);
        } else {
            vks.push(left_vk);
            vks.push(right_vk);
        }
    } else if has_left {
        vks.push(left_vk);
    } else if has_right {
        vks.push(right_vk);
    }
}

fn is_vk_down(vk: u16) -> bool {
    (unsafe { GetAsyncKeyState(vk as i32) } as u16 & 0x8000) != 0
}

fn all_combo_keys_down(hotkey: Hotkey) -> bool {
    all_combo_keys_down_with(hotkey, &mut is_vk_down)
}

fn all_combo_keys_down_with<F>(hotkey: Hotkey, is_down: &mut F) -> bool
where
    F: FnMut(u16) -> bool,
{
    if !is_modifier_only_pollable(hotkey) {
        return false;
    }

    let mut required_vks = modifier_vks(hotkey.modifiers);
    let generic_cmd = hotkey.modifiers.contains(Modifiers::CMD);

    if generic_cmd {
        required_vks.retain(|vk| *vk != VK_LWIN.0 && *vk != VK_RWIN.0);
    }

    if required_vks.into_iter().any(|vk| !is_down(vk)) {
        return false;
    }

    !generic_cmd || is_down(VK_LWIN.0) || is_down(VK_RWIN.0)
}

fn sync_registered_hotkeys(
    blocking_hotkeys: &Option<BlockingHotkeys>,
    state: &mut RegisteredHotkeyState,
) {
    let Some(desired) = snapshot_registerable_hotkeys(blocking_hotkeys) else {
        return;
    };

    if desired == state.desired {
        return;
    }

    unregister_hotkeys(&mut state.registered);
    state.desired = desired.clone();

    let mut hotkeys: Vec<Hotkey> = desired.into_iter().collect();
    hotkeys.sort_by_key(|hotkey| hotkey.to_string());

    for (index, hotkey) in hotkeys.into_iter().enumerate() {
        let id = index as i32 + 1;
        if id > MAX_HOTKEY_ID {
            eprintln!(
                "Skipping RegisterHotKey fallback for {}: too many hotkeys",
                hotkey
            );
            break;
        }

        let Some((modifiers, vk)) = register_hotkey_args(hotkey) else {
            continue;
        };

        match unsafe { RegisterHotKey(None, id, modifiers, vk) } {
            Ok(()) => {
                state.registered.insert(id, hotkey);
            }
            Err(e) => {
                eprintln!(
                    "Failed to register WM_HOTKEY fallback for {}: {:?}",
                    hotkey, e
                );
            }
        }
    }
}

fn snapshot_registerable_hotkeys(
    blocking_hotkeys: &Option<BlockingHotkeys>,
) -> Option<HashSet<Hotkey>> {
    let Some(hotkeys) = blocking_hotkeys else {
        return Some(HashSet::new());
    };

    let hotkeys = hotkeys.lock().ok()?;
    Some(
        hotkeys
            .iter()
            .copied()
            .filter(|hotkey| register_hotkey_args(*hotkey).is_some())
            .collect(),
    )
}

fn unregister_hotkeys(registered_hotkeys: &mut HashMap<i32, Hotkey>) {
    for id in registered_hotkeys.keys().copied().collect::<Vec<_>>() {
        let _ = unsafe { UnregisterHotKey(None, id) };
    }
    registered_hotkeys.clear();
}

fn register_hotkey_args(hotkey: Hotkey) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let key = hotkey.key?;
    let vk = vk_from_key(key)? as u32;
    let modifiers = hot_key_modifiers_from_modifiers(hotkey.modifiers)?;
    Some((modifiers, vk))
}

fn hot_key_modifiers_from_modifiers(modifiers: Modifiers) -> Option<HOT_KEY_MODIFIERS> {
    if modifiers.contains(Modifiers::FN)
        || has_side_specific_modifier(modifiers, Modifiers::CMD_LEFT, Modifiers::CMD_RIGHT)
        || has_side_specific_modifier(modifiers, Modifiers::CTRL_LEFT, Modifiers::CTRL_RIGHT)
        || has_side_specific_modifier(modifiers, Modifiers::OPT_LEFT, Modifiers::OPT_RIGHT)
        || has_side_specific_modifier(modifiers, Modifiers::SHIFT_LEFT, Modifiers::SHIFT_RIGHT)
    {
        return None;
    }

    let mut hotkey_modifiers = MOD_NOREPEAT;
    if modifiers.contains(Modifiers::CMD) {
        hotkey_modifiers |= MOD_WIN;
    }
    if modifiers.contains(Modifiers::CTRL) {
        hotkey_modifiers |= MOD_CONTROL;
    }
    if modifiers.contains(Modifiers::OPT) {
        hotkey_modifiers |= MOD_ALT;
    }
    if modifiers.contains(Modifiers::SHIFT) {
        hotkey_modifiers |= MOD_SHIFT;
    }

    Some(hotkey_modifiers)
}

fn has_side_specific_modifier(modifiers: Modifiers, left: Modifiers, right: Modifiers) -> bool {
    modifiers.contains(left) ^ modifiers.contains(right)
}

/// Internal listener state returned to KeyboardListener
pub(crate) struct WindowsListenerState {
    pub event_receiver: mpsc::Receiver<KeyEvent>,
    pub thread_handle: Option<JoinHandle<()>>,
    pub running: Arc<AtomicBool>,
    pub blocking_hotkeys: Option<BlockingHotkeys>,
}

/// Spawn a Windows low-level keyboard hook listener
pub(crate) fn spawn(blocking_hotkeys: Option<BlockingHotkeys>) -> Result<WindowsListenerState> {
    let (tx, rx) = mpsc::channel();
    let running = Arc::new(AtomicBool::new(true));
    let thread_running = Arc::clone(&running);
    let thread_blocking = blocking_hotkeys.clone();

    let handle = thread::spawn(move || {
        let message_sender = tx.clone();
        let last_ll_fire_ms = Arc::new(AtomicU64::new(0));
        let hook_last_ll_fire_ms = Arc::clone(&last_ll_fire_ms);
        let registration_blocking_hotkeys = thread_blocking.clone();

        // Initialize thread-local hook context
        HOOK_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = Some(HookContext {
                event_sender: tx,
                current_modifiers: Modifiers::empty(),
                blocking_hotkeys: thread_blocking,
                last_ll_fire_ms: hook_last_ll_fire_ms,
            });
        });

        // Install the low-level keyboard hook
        let kb_hook =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) };

        let kb_hook = match kb_hook {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to install keyboard hook: {:?}", e);
                return;
            }
        };

        // Install the low-level mouse hook
        let mouse_hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) };

        let mouse_hook = match mouse_hook {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to install mouse hook: {:?}", e);
                // Clean up keyboard hook before returning
                unsafe {
                    let _ = UnhookWindowsHookEx(kb_hook);
                }
                return;
            }
        };

        // Message loop - required for low-level hooks to function.
        // Keep the short timeout so shutdown polling behavior remains unchanged.
        let mut msg = MSG::default();
        let mut registered_hotkeys = RegisteredHotkeyState::default();
        let mut modifier_poller = ModifierOnlyPoller::default();
        loop {
            // Check if we should stop
            if !thread_running.load(Ordering::SeqCst) {
                break;
            }

            sync_registered_hotkeys(&registration_blocking_hotkeys, &mut registered_hotkeys);
            sync_modifier_only_poller(&registration_blocking_hotkeys, &mut modifier_poller);

            // Process all pending messages
            if drain_thread_messages(
                &mut msg,
                &registered_hotkeys.registered,
                &message_sender,
                &last_ll_fire_ms,
            ) {
                break;
            }

            modifier_poller.poll(&message_sender, &last_ll_fire_ms);

            // Wait for messages or timeout — unlike thread::sleep, this returns
            // immediately when a message arrives, so hook callbacks are never delayed.
            wait_for_message_or_timeout(HOOK_LOOP_TIMEOUT_MS);
        }

        // Clean up the hooks
        unsafe {
            unregister_hotkeys(&mut registered_hotkeys.registered);
            let _ = UnhookWindowsHookEx(kb_hook);
            let _ = UnhookWindowsHookEx(mouse_hook);
        }

        // Clear thread-local state
        HOOK_CONTEXT.with(|ctx| {
            *ctx.borrow_mut() = None;
        });
    });

    Ok(WindowsListenerState {
        event_receiver: rx,
        thread_handle: Some(handle),
        running,
        blocking_hotkeys,
    })
}

/// Low-level keyboard hook callback
///
/// This function is called by Windows for every keyboard event system-wide.
/// It must return quickly to avoid input lag.
unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // If code < 0, we must pass to next hook without processing
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let mut should_block = false;

    // Process the keyboard event
    HOOK_CONTEXT.with(|ctx_cell| {
        let mut ctx_ref = ctx_cell.borrow_mut();
        if let Some(ctx) = ctx_ref.as_mut() {
            // Extract key information from KBDLLHOOKSTRUCT
            let kb_struct = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let vk_code = kb_struct.vkCode as u16;
            let is_extended = (kb_struct.flags.0 & LLKHF_EXTENDED.0) != 0;

            let is_key_down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);

            // Check if this is a modifier key
            if let Some(modifier) = vk_to_modifier(vk_code) {
                let prev_modifiers = ctx.current_modifiers;

                // Update modifier state
                if is_key_down {
                    ctx.current_modifiers |= modifier;
                } else {
                    ctx.current_modifiers &= !modifier;
                }

                // Only emit event if modifiers actually changed
                if ctx.current_modifiers != prev_modifiers {
                    // Check if modifier-only combo should be blocked
                    should_block =
                        should_block_hotkey(&ctx.blocking_hotkeys, ctx.current_modifiers, None);
                    if should_block && is_key_down {
                        ctx.last_ll_fire_ms
                            .store(unsafe { GetTickCount64() }, Ordering::Relaxed);
                    }

                    let _ = ctx.event_sender.send(KeyEvent {
                        modifiers: ctx.current_modifiers,
                        key: None,
                        is_key_down,
                        changed_modifier: Some(modifier),
                    });
                }
            } else if let Some(key) = vk_to_key(vk_code, is_extended) {
                // Regular key event
                should_block =
                    should_block_hotkey(&ctx.blocking_hotkeys, ctx.current_modifiers, Some(key));
                if should_block && is_key_down {
                    ctx.last_ll_fire_ms
                        .store(unsafe { GetTickCount64() }, Ordering::Relaxed);
                }

                let _ = ctx.event_sender.send(KeyEvent {
                    modifiers: ctx.current_modifiers,
                    key: Some(key),
                    is_key_down,
                    changed_modifier: None,
                });
            }
        }
    });

    if should_block {
        // Return non-zero to block the event from propagating
        LRESULT(1)
    } else {
        // Pass to next hook in chain
        CallNextHookEx(None, code, wparam, lparam)
    }
}

/// Low-level mouse hook callback
///
/// This function is called by Windows for every mouse event system-wide.
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // If code < 0, we must pass to next hook without processing
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    // Process the mouse event
    HOOK_CONTEXT.with(|ctx_cell| {
        let mut ctx_ref = ctx_cell.borrow_mut();
        if let Some(ctx) = ctx_ref.as_mut() {
            let mouse_struct = &*(lparam.0 as *const MSLLHOOKSTRUCT);

            // Only report left/right clicks when modifiers are held (to avoid noise)
            let has_modifiers = !ctx.current_modifiers.is_empty();

            let (key, is_down) = match wparam.0 as u32 {
                WM_LBUTTONDOWN if has_modifiers => (Some(Key::MouseLeft), true),
                WM_LBUTTONUP if has_modifiers => (Some(Key::MouseLeft), false),
                WM_RBUTTONDOWN if has_modifiers => (Some(Key::MouseRight), true),
                WM_RBUTTONUP if has_modifiers => (Some(Key::MouseRight), false),
                // Middle and X buttons always reported
                WM_MBUTTONDOWN => (Some(Key::MouseMiddle), true),
                WM_MBUTTONUP => (Some(Key::MouseMiddle), false),
                WM_XBUTTONDOWN => {
                    // High word of mouseData contains which X button (1 or 2)
                    let xbutton = (mouse_struct.mouseData >> 16) & 0xFFFF;
                    let key = if xbutton == 1 {
                        Some(Key::MouseX1)
                    } else if xbutton == 2 {
                        Some(Key::MouseX2)
                    } else {
                        None
                    };
                    (key, true)
                }
                WM_XBUTTONUP => {
                    let xbutton = (mouse_struct.mouseData >> 16) & 0xFFFF;
                    let key = if xbutton == 1 {
                        Some(Key::MouseX1)
                    } else if xbutton == 2 {
                        Some(Key::MouseX2)
                    } else {
                        None
                    };
                    (key, false)
                }
                _ => (None, false),
            };

            if let Some(key) = key {
                let _ = ctx.event_sender.send(KeyEvent {
                    modifiers: ctx.current_modifiers,
                    key: Some(key),
                    is_key_down: is_down,
                    changed_modifier: None,
                });
            }
        }
    });

    // Always pass mouse events through (no blocking for mouse)
    CallNextHookEx(None, code, wparam, lparam)
}

/// Check if a hotkey combination should be blocked
fn should_block_hotkey(
    blocking_hotkeys: &Option<BlockingHotkeys>,
    modifiers: Modifiers,
    key: Option<Key>,
) -> bool {
    if let Some(ref hotkeys) = blocking_hotkeys {
        if let Ok(set) = hotkeys.lock() {
            return set
                .iter()
                .any(|h| h.modifiers.matches(modifiers) && h.key == key);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;

    fn drain_test_messages(msg: &mut MSG) -> bool {
        let registered_hotkeys = HashMap::new();
        let (tx, _rx) = mpsc::channel();
        let last_ll_fire_ms = AtomicU64::new(0);

        drain_thread_messages(msg, &registered_hotkeys, &tx, &last_ll_fire_ms)
    }

    fn clear_message_queue() {
        let mut msg = MSG::default();
        unsafe { while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {} }
    }

    #[test]
    fn wait_times_out_when_no_messages() {
        clear_message_queue();
        let start = Instant::now();
        wait_for_message_or_timeout(20);
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(8),
            "expected wait to block close to timeout, elapsed={elapsed:?}"
        );
        clear_message_queue();
    }

    #[test]
    fn wait_returns_immediately_when_message_is_pending() {
        clear_message_queue();
        unsafe {
            PostQuitMessage(0);
        }
        let start = Instant::now();
        wait_for_message_or_timeout(200);
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(100),
            "expected pending message to wake wait early, elapsed={elapsed:?}"
        );
        clear_message_queue();
    }

    #[test]
    fn drain_messages_stops_on_wm_quit() {
        clear_message_queue();
        unsafe {
            PostQuitMessage(0);
        }
        let mut msg = MSG::default();
        assert!(drain_test_messages(&mut msg));
        clear_message_queue();
    }

    #[test]
    fn register_hotkey_args_use_no_repeat_and_generic_modifier_flags() {
        let hotkey = Hotkey::new(Modifiers::CTRL | Modifiers::OPT, Key::Space).unwrap();
        let (modifiers, vk) = register_hotkey_args(hotkey).unwrap();

        assert_eq!(vk, 0x20);
        assert_eq!(modifiers, MOD_NOREPEAT | MOD_CONTROL | MOD_ALT);
    }

    #[test]
    fn register_hotkey_args_skip_side_specific_modifiers() {
        let hotkey = Hotkey::new(Modifiers::CTRL_RIGHT, Key::Space).unwrap();

        assert!(register_hotkey_args(hotkey).is_none());
    }

    #[test]
    fn register_hotkey_args_skip_unsupported_keys() {
        let hotkey = Hotkey::new(Modifiers::CTRL, Key::MouseLeft).unwrap();

        assert!(register_hotkey_args(hotkey).is_none());
    }

    #[test]
    fn wm_hotkey_message_sends_key_down_event() {
        let hotkey = Hotkey::new(Modifiers::CTRL, Key::Space).unwrap();
        let registered_hotkeys = HashMap::from([(1, hotkey)]);
        let (tx, rx) = mpsc::channel();
        let last_ll_fire_ms = AtomicU64::new(0);
        let msg = MSG {
            message: WM_HOTKEY,
            wParam: WPARAM(1),
            ..MSG::default()
        };

        handle_registered_hotkey_message(&msg, &registered_hotkeys, &tx, &last_ll_fire_ms);

        let event = rx.try_recv().unwrap();
        assert_eq!(event.modifiers, Modifiers::CTRL);
        assert_eq!(event.key, Some(Key::Space));
        assert!(event.is_key_down);
    }

    #[test]
    fn wm_hotkey_message_skips_recent_ll_hook_fire() {
        let hotkey = Hotkey::new(Modifiers::CTRL, Key::Space).unwrap();
        let registered_hotkeys = HashMap::from([(1, hotkey)]);
        let (tx, rx) = mpsc::channel();
        let last_ll_fire_ms = AtomicU64::new(unsafe { GetTickCount64() });
        let msg = MSG {
            message: WM_HOTKEY,
            wParam: WPARAM(1),
            ..MSG::default()
        };

        handle_registered_hotkey_message(&msg, &registered_hotkeys, &tx, &last_ll_fire_ms);

        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn modifier_vks_maps_right_ctrl_right_shift() {
        let vks = modifier_vks(Modifiers::CTRL_RIGHT | Modifiers::SHIFT_RIGHT);

        assert_eq!(vks, vec![VK_RCONTROL.0, VK_RSHIFT.0]);
    }

    #[test]
    fn modifier_poller_emits_down_once_and_up_on_transition() {
        let hotkey = Hotkey::new(Modifiers::CTRL_RIGHT | Modifiers::SHIFT_RIGHT, None).unwrap();
        let mut poller = ModifierOnlyPoller {
            hotkeys: vec![hotkey],
            desired: HashSet::from([hotkey]),
            prev_active: HashMap::new(),
        };
        let (tx, rx) = mpsc::channel();
        let last_ll_fire_ms = AtomicU64::new(0);

        let inactive = HashSet::<u16>::new();
        poller.poll_with_key_state(&tx, &last_ll_fire_ms, 100, |vk| inactive.contains(&vk));
        assert!(rx.try_recv().is_err());

        let active = HashSet::from([VK_RCONTROL.0, VK_RSHIFT.0]);
        poller.poll_with_key_state(&tx, &last_ll_fire_ms, 110, |vk| active.contains(&vk));
        let event = rx.try_recv().unwrap();
        assert_eq!(event.modifiers, hotkey.modifiers);
        assert_eq!(event.key, None);
        assert!(event.is_key_down);
        assert!(rx.try_recv().is_err());

        poller.poll_with_key_state(&tx, &last_ll_fire_ms, 120, |vk| active.contains(&vk));
        assert!(rx.try_recv().is_err());

        poller.poll_with_key_state(&tx, &last_ll_fire_ms, 130, |vk| inactive.contains(&vk));
        let event = rx.try_recv().unwrap();
        assert_eq!(event.modifiers, hotkey.modifiers);
        assert_eq!(event.key, None);
        assert!(!event.is_key_down);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn modifier_poller_dedups_recent_ll_hook_fire() {
        let hotkey = Hotkey::new(Modifiers::CTRL_RIGHT | Modifiers::SHIFT_RIGHT, None).unwrap();
        let mut poller = ModifierOnlyPoller {
            hotkeys: vec![hotkey],
            desired: HashSet::from([hotkey]),
            prev_active: HashMap::new(),
        };
        let (tx, rx) = mpsc::channel();
        let last_ll_fire_ms = AtomicU64::new(100);
        let active = HashSet::from([VK_RCONTROL.0, VK_RSHIFT.0]);

        poller.poll_with_key_state(&tx, &last_ll_fire_ms, 120, |vk| active.contains(&vk));

        assert!(rx.try_recv().is_err());
        assert_eq!(
            poller.prev_active.get(&u64::from(hotkey.modifiers.bits())),
            Some(&true)
        );
    }
}
