use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Instant;

use padless_core::{Digit, Engine, Key, KeyAction, KeyEvent, Output, Response, Trigger};
use tracing::warn;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE, SendInput, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_INJECTED,
    MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::{KeyboardBackend, PlatformError, ShutdownHandle};

const VK_RETURN: u16 = 0x0D;
const VK_CAPITAL: u16 = 0x14;
const VK_ESCAPE: u16 = 0x1B;
const VK_SPACE: u16 = 0x20;
const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;
const VK_F1: u16 = 0x70;
const VK_LSHIFT: u16 = 0xA0;
const VK_RSHIFT: u16 = 0xA1;
const VK_LCONTROL: u16 = 0xA2;
const VK_RCONTROL: u16 = 0xA3;
const VK_LMENU: u16 = 0xA4;
const VK_RMENU: u16 = 0xA5;
const VK_UNASSIGNED_MASK: u16 = 0xE8;

const ALTGR_SYNTHETIC_CTRL_SCAN: u32 = 0x21D;

const DIGIT_ROW_SCANS: [u16; 10] = [0x0B, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];

const LETTER_SCANS: [u16; 26] = [
    0x1E, 0x30, 0x2E, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18, 0x19,
    0x10, 0x13, 0x1F, 0x14, 0x16, 0x2F, 0x11, 0x2D, 0x15, 0x2C,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct NativeKey {
    vk: u16,
    scan: u16,
    extended: bool,
}

impl NativeKey {
    const fn new(vk: u16, scan: u16, extended: bool) -> Self {
        Self { vk, scan, extended }
    }

    fn to_key(self) -> Key {
        match self.vk {
            VK_LSHIFT => return Key::LeftShift,
            VK_RSHIFT => return Key::RightShift,
            VK_LCONTROL if u32::from(self.scan) != ALTGR_SYNTHETIC_CTRL_SCAN => {
                return Key::LeftCtrl;
            }
            VK_RCONTROL => return Key::RightCtrl,
            VK_LMENU => return Key::LeftAlt,
            VK_RMENU => return Key::RightAlt,
            VK_LWIN => return Key::LeftMeta,
            VK_RWIN => return Key::RightMeta,
            VK_CAPITAL => return Key::CapsLock,
            VK_RETURN => return Key::Enter,
            VK_SPACE => return Key::Space,
            VK_ESCAPE => return Key::Escape,
            vk if (VK_F1..VK_F1 + 24).contains(&vk) => {
                return Key::Function(u8::try_from(vk - VK_F1 + 1).unwrap_or(0));
            }
            _ => {}
        }
        if !self.extended {
            if let Some(position) = DIGIT_ROW_SCANS.iter().position(|scan| *scan == self.scan) {
                return digit_key(position);
            }
            if let Some(position) = LETTER_SCANS.iter().position(|scan| *scan == self.scan) {
                return letter_key(position);
            }
        }
        Key::Other(self.encode())
    }

    fn from_key(key: Key) -> Self {
        match key {
            Key::Digit(digit) => Self::new(0, DIGIT_ROW_SCANS[usize::from(digit.value())], false),
            Key::Letter(letter) => {
                let scan = letter
                    .checked_sub(b'a')
                    .and_then(|offset| LETTER_SCANS.get(usize::from(offset)))
                    .copied()
                    .unwrap_or(0);
                Self::new(0, scan, false)
            }
            Key::Function(number) => {
                Self::new(VK_F1 + u16::from(number.clamp(1, 24)) - 1, 0, false)
            }
            Key::LeftAlt => Self::new(VK_LMENU, 0x38, false),
            Key::RightAlt => Self::new(VK_RMENU, 0x38, true),
            Key::LeftCtrl => Self::new(VK_LCONTROL, 0x1D, false),
            Key::RightCtrl => Self::new(VK_RCONTROL, 0x1D, true),
            Key::LeftShift => Self::new(VK_LSHIFT, 0x2A, false),
            Key::RightShift => Self::new(VK_RSHIFT, 0x36, false),
            Key::LeftMeta => Self::new(VK_LWIN, 0x5B, true),
            Key::RightMeta => Self::new(VK_RWIN, 0x5C, true),
            Key::CapsLock => Self::new(VK_CAPITAL, 0x3A, false),
            Key::Enter => Self::new(VK_RETURN, 0x1C, false),
            Key::Space => Self::new(VK_SPACE, 0x39, false),
            Key::Escape => Self::new(VK_ESCAPE, 0x01, false),
            Key::Other(raw) => Self::decode(raw),
        }
    }

    fn encode(self) -> u32 {
        u32::from(self.vk) | (u32::from(self.scan) << 8) | (u32::from(self.extended) << 24)
    }

    fn decode(raw: u32) -> Self {
        Self {
            vk: u16::try_from(raw & 0xFF).unwrap_or(0),
            scan: u16::try_from((raw >> 8) & 0xFFFF).unwrap_or(0),
            extended: (raw >> 24) & 1 == 1,
        }
    }

    fn input(self, action: KeyAction) -> INPUT {
        let mut flags = KEYBD_EVENT_FLAGS(0);
        if self.vk == 0 {
            flags |= KEYEVENTF_SCANCODE;
        }
        if self.extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if action == KeyAction::Release {
            flags |= KEYEVENTF_KEYUP;
        }
        keyboard_input(self.vk, self.scan, flags)
    }
}

fn digit_key(position: usize) -> Key {
    u8::try_from(position)
        .ok()
        .and_then(Digit::new)
        .map_or(Key::Other(0), Key::Digit)
}

fn letter_key(position: usize) -> Key {
    u8::try_from(position)
        .ok()
        .and_then(|offset| b'a'.checked_add(offset))
        .map_or(Key::Other(0), Key::Letter)
}

fn keyboard_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn inputs_for(outputs: &[Output]) -> Vec<INPUT> {
    let mut inputs = Vec::new();
    for output in outputs {
        match output {
            Output::Key(event) => inputs.push(NativeKey::from_key(event.key).input(event.action)),
            Output::Mask => {
                inputs.push(keyboard_input(VK_UNASSIGNED_MASK, 0, KEYBD_EVENT_FLAGS(0)));
                inputs.push(keyboard_input(VK_UNASSIGNED_MASK, 0, KEYEVENTF_KEYUP));
            }
            Output::Text(text) => {
                for unit in text.encode_utf16() {
                    inputs.push(keyboard_input(0, unit, KEYEVENTF_UNICODE));
                    inputs.push(keyboard_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                }
            }
        }
    }
    inputs
}

fn send(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    let size = i32::try_from(size_of::<INPUT>()).unwrap_or(i32::MAX);
    let sent = unsafe { SendInput(inputs, size) };
    if usize::try_from(sent).unwrap_or(0) != inputs.len() {
        warn!(
            "SendInput injected {sent} of {} events; the focused window may run at a higher integrity level",
            inputs.len()
        );
    }
}

thread_local! {
    static ACTIVE_ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

struct ActiveEngine;

impl ActiveEngine {
    fn install(engine: Engine) -> Self {
        ACTIVE_ENGINE.with(|slot| *slot.borrow_mut() = Some(engine));
        Self
    }
}

impl Drop for ActiveEngine {
    fn drop(&mut self) {
        ACTIVE_ENGINE.with(|slot| slot.borrow_mut().take());
    }
}

fn dispatch(event: KeyEvent) -> bool {
    ACTIVE_ENGINE.with(|slot| {
        let Ok(mut slot) = slot.try_borrow_mut() else {
            return false;
        };
        let Some(engine) = slot.as_mut() else {
            return false;
        };
        match engine.handle(event, Instant::now()) {
            Response::Pass => false,
            Response::Replace(outputs) => {
                send(&inputs_for(&outputs));
                true
            }
        }
    })
}

fn translate(message: u32, info: &KBDLLHOOKSTRUCT) -> Option<KeyEvent> {
    if info.flags.0 & LLKHF_INJECTED.0 != 0 {
        return None;
    }
    let action = match message {
        WM_KEYDOWN | WM_SYSKEYDOWN => KeyAction::Press,
        WM_KEYUP | WM_SYSKEYUP => KeyAction::Release,
        _ => return None,
    };
    let native = NativeKey {
        vk: u16::try_from(info.vkCode).ok()?,
        scan: u16::try_from(info.scanCode).ok()?,
        extended: info.flags.0 & LLKHF_EXTENDED.0 != 0,
    };
    Some(KeyEvent {
        key: native.to_key(),
        action,
    })
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == i32::try_from(HC_ACTION).unwrap_or(0) {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let message = u32::try_from(wparam.0).unwrap_or(0);
        if translate(message, info).is_some_and(dispatch) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

struct InstalledHook(HHOOK);

impl InstalledHook {
    fn install() -> Result<Self, PlatformError> {
        let module = unsafe { GetModuleHandleW(None) }.map_err(PlatformError::Hook)?;
        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook),
                Some(HINSTANCE(module.0)),
                0,
            )
        }
        .map_err(PlatformError::Hook)?;
        Ok(Self(hook))
    }
}

impl Drop for InstalledHook {
    fn drop(&mut self) {
        if let Err(error) = unsafe { UnhookWindowsHookEx(self.0) } {
            warn!("failed to remove the keyboard hook: {error}");
        }
    }
}

#[derive(Default)]
struct Shared {
    thread_id: AtomicU32,
    stop: AtomicBool,
}

impl Shared {
    fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let thread_id = self.thread_id.load(Ordering::SeqCst);
        if thread_id != 0 {
            let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        }
    }
}

pub struct WindowsBackend {
    shared: Arc<Shared>,
}

pub fn open() -> Result<WindowsBackend, PlatformError> {
    Ok(WindowsBackend {
        shared: Arc::new(Shared::default()),
    })
}

#[must_use]
pub fn supports_trigger(_trigger: Trigger) -> bool {
    true
}

fn create_message_queue() {
    let mut message = MSG::default();
    let _ = unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE) };
}

impl KeyboardBackend for WindowsBackend {
    fn name(&self) -> &'static str {
        "windows low-level keyboard hook"
    }

    fn shutdown_handle(&self) -> ShutdownHandle {
        let shared = Arc::clone(&self.shared);
        ShutdownHandle::new(move || shared.request_stop())
    }

    fn run(&mut self, engine: Engine) -> Result<(), PlatformError> {
        create_message_queue();
        self.shared
            .thread_id
            .store(unsafe { GetCurrentThreadId() }, Ordering::SeqCst);
        if self.shared.stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        let _engine = ActiveEngine::install(engine);
        let _hook = InstalledHook::install()?;
        let mut message = MSG::default();
        loop {
            let status = unsafe { GetMessageW(&raw mut message, None, 0, 0) };
            match status.0 {
                0 => return Ok(()),
                -1 => {
                    return Err(PlatformError::MessageLoop(
                        ::windows::core::Error::from_thread(),
                    ));
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(key: Key) -> Key {
        NativeKey::from_key(key).to_key()
    }

    #[test]
    fn digit_row_maps_by_scan_code() {
        for value in 0..=9 {
            let key = Key::Digit(Digit::new(value).unwrap());
            assert_eq!(round_trip(key), key);
        }
    }

    #[test]
    fn letters_map_by_scan_code() {
        for letter in b'a'..=b'z' {
            assert_eq!(round_trip(Key::Letter(letter)), Key::Letter(letter));
        }
    }

    #[test]
    fn named_keys_round_trip() {
        let keys = [
            Key::LeftAlt,
            Key::RightAlt,
            Key::LeftCtrl,
            Key::RightCtrl,
            Key::LeftShift,
            Key::RightShift,
            Key::LeftMeta,
            Key::RightMeta,
            Key::CapsLock,
            Key::Enter,
            Key::Space,
            Key::Escape,
            Key::Function(1),
            Key::Function(13),
            Key::Function(24),
        ];
        for key in keys {
            assert_eq!(round_trip(key), key);
        }
    }

    #[test]
    fn numpad_digits_are_not_digit_row() {
        let numpad_one = NativeKey::new(0x61, 0x4F, false);
        assert!(matches!(numpad_one.to_key(), Key::Other(_)));
    }

    #[test]
    fn altgr_synthetic_ctrl_is_not_left_ctrl() {
        let synthetic = NativeKey::new(VK_LCONTROL, 0x21D, false);
        let key = synthetic.to_key();
        assert!(matches!(key, Key::Other(_)));
        assert_eq!(NativeKey::from_key(key), synthetic);
    }

    #[test]
    fn unknown_keys_round_trip_through_other() {
        let native = NativeKey::new(0xAD, 0x20, true);
        assert_eq!(NativeKey::from_key(native.to_key()), native);
    }

    #[test]
    fn text_becomes_unicode_key_pairs() {
        let inputs = inputs_for(&[Output::Text("a\u{1F600}".to_owned())]);
        assert_eq!(inputs.len(), 6);
        let scans: Vec<u16> = inputs
            .iter()
            .map(|input| unsafe { input.Anonymous.ki.wScan })
            .collect();
        assert_eq!(scans, [0x61, 0x61, 0xD83D, 0xD83D, 0xDE00, 0xDE00]);
    }

    #[test]
    fn mask_is_an_unassigned_key_tap() {
        let inputs = inputs_for(&[Output::Mask]);
        let keys: Vec<(u16, u32)> = inputs
            .iter()
            .map(|input| unsafe { (input.Anonymous.ki.wVk.0, input.Anonymous.ki.dwFlags.0) })
            .collect();
        assert_eq!(
            keys,
            [
                (VK_UNASSIGNED_MASK, 0),
                (VK_UNASSIGNED_MASK, KEYEVENTF_KEYUP.0)
            ]
        );
    }
}
