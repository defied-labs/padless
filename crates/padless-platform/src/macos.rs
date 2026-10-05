use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes, kCFRunLoopDefaultMode};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use padless_core::{Digit, Engine, Key, KeyAction, KeyEvent, Output, Response, Trigger};

use crate::{KeyboardBackend, PlatformError, ShutdownHandle};

const OWN_EVENT_MARKER: i64 = 0x0070_6164_6C65_7373;
const MAX_UTF16_UNITS_PER_EVENT: usize = 20;
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(500);

const KEYCODE_RETURN: u16 = 0x24;
const KEYCODE_KEYPAD_ENTER: u16 = 0x4C;
const KEYCODE_SPACE: u16 = 0x31;
const KEYCODE_ESCAPE: u16 = 0x35;
const KEYCODE_CAPS_LOCK: u16 = 0x39;

const DIGIT_KEYCODES: [u16; 10] = [0x1D, 0x12, 0x13, 0x14, 0x15, 0x17, 0x16, 0x1A, 0x1C, 0x19];

const LETTER_KEYCODES: [u16; 26] = [
    0x00, 0x0B, 0x08, 0x02, 0x0E, 0x03, 0x05, 0x04, 0x22, 0x26, 0x28, 0x25, 0x2E, 0x2D, 0x1F, 0x23,
    0x0C, 0x0F, 0x01, 0x11, 0x20, 0x09, 0x0D, 0x07, 0x10, 0x06,
];

const FUNCTION_KEYCODES: [u16; 20] = [
    0x7A, 0x78, 0x63, 0x76, 0x60, 0x61, 0x62, 0x64, 0x65, 0x6D, 0x67, 0x6F, 0x69, 0x6B, 0x71, 0x6A,
    0x40, 0x4F, 0x50, 0x5A,
];

struct ModifierKey {
    keycode: u16,
    key: Key,
    device_bit: u64,
    group: CGEventFlags,
}

const MODIFIER_KEYS: [ModifierKey; 8] = [
    ModifierKey {
        keycode: 0x3B,
        key: Key::LeftCtrl,
        device_bit: 0x0000_0001,
        group: CGEventFlags::CGEventFlagControl,
    },
    ModifierKey {
        keycode: 0x3E,
        key: Key::RightCtrl,
        device_bit: 0x0000_2000,
        group: CGEventFlags::CGEventFlagControl,
    },
    ModifierKey {
        keycode: 0x38,
        key: Key::LeftShift,
        device_bit: 0x0000_0002,
        group: CGEventFlags::CGEventFlagShift,
    },
    ModifierKey {
        keycode: 0x3C,
        key: Key::RightShift,
        device_bit: 0x0000_0004,
        group: CGEventFlags::CGEventFlagShift,
    },
    ModifierKey {
        keycode: 0x37,
        key: Key::LeftMeta,
        device_bit: 0x0000_0008,
        group: CGEventFlags::CGEventFlagCommand,
    },
    ModifierKey {
        keycode: 0x36,
        key: Key::RightMeta,
        device_bit: 0x0000_0010,
        group: CGEventFlags::CGEventFlagCommand,
    },
    ModifierKey {
        keycode: 0x3A,
        key: Key::LeftAlt,
        device_bit: 0x0000_0020,
        group: CGEventFlags::CGEventFlagAlternate,
    },
    ModifierKey {
        keycode: 0x3D,
        key: Key::RightAlt,
        device_bit: 0x0000_0040,
        group: CGEventFlags::CGEventFlagAlternate,
    },
];

const MODIFIER_GROUPS: [CGEventFlags; 4] = [
    CGEventFlags::CGEventFlagControl,
    CGEventFlags::CGEventFlagShift,
    CGEventFlags::CGEventFlagCommand,
    CGEventFlags::CGEventFlagAlternate,
];

impl ModifierKey {
    fn by_keycode(keycode: u16) -> Option<&'static Self> {
        MODIFIER_KEYS
            .iter()
            .find(|modifier| modifier.keycode == keycode)
    }

    fn is_down(&self, flags: CGEventFlags) -> bool {
        flags.bits() & self.device_bit != 0
    }

    fn apply(&self, flags: CGEventFlags, action: KeyAction) -> CGEventFlags {
        let bits = match action {
            KeyAction::Press => flags.bits() | self.device_bit,
            KeyAction::Release => flags.bits() & !self.device_bit,
        };
        let mut flags = CGEventFlags::from_bits_retain(bits);
        for group in MODIFIER_GROUPS {
            let held = MODIFIER_KEYS
                .iter()
                .any(|modifier| modifier.group == group && modifier.is_down(flags));
            flags.set(group, held);
        }
        flags
    }
}

fn key_for_keycode(keycode: u16) -> Key {
    if let Some(modifier) = ModifierKey::by_keycode(keycode) {
        return modifier.key;
    }
    if let Some(position) = DIGIT_KEYCODES.iter().position(|code| *code == keycode) {
        return u8::try_from(position)
            .ok()
            .and_then(Digit::new)
            .map_or(Key::Other(u32::from(keycode)), Key::Digit);
    }
    if let Some(position) = LETTER_KEYCODES.iter().position(|code| *code == keycode) {
        return u8::try_from(position)
            .ok()
            .and_then(|offset| b'a'.checked_add(offset))
            .map_or(Key::Other(u32::from(keycode)), Key::Letter);
    }
    if let Some(position) = FUNCTION_KEYCODES.iter().position(|code| *code == keycode) {
        return u8::try_from(position + 1)
            .ok()
            .and_then(Key::function)
            .unwrap_or(Key::Other(u32::from(keycode)));
    }
    match keycode {
        KEYCODE_RETURN | KEYCODE_KEYPAD_ENTER => Key::Enter,
        KEYCODE_SPACE => Key::Space,
        KEYCODE_ESCAPE => Key::Escape,
        KEYCODE_CAPS_LOCK => Key::CapsLock,
        _ => Key::Other(u32::from(keycode)),
    }
}

fn keycode_for_key(key: Key) -> Option<u16> {
    match key {
        Key::Digit(digit) => DIGIT_KEYCODES.get(usize::from(digit.value())).copied(),
        Key::Letter(letter) => letter
            .checked_sub(b'a')
            .and_then(|offset| LETTER_KEYCODES.get(usize::from(offset)))
            .copied(),
        Key::Function(number) => usize::from(number)
            .checked_sub(1)
            .and_then(|index| FUNCTION_KEYCODES.get(index))
            .copied(),
        Key::Enter => Some(KEYCODE_RETURN),
        Key::Space => Some(KEYCODE_SPACE),
        Key::Escape => Some(KEYCODE_ESCAPE),
        Key::CapsLock => Some(KEYCODE_CAPS_LOCK),
        Key::Other(raw) => u16::try_from(raw).ok(),
        modifier => MODIFIER_KEYS
            .iter()
            .find(|candidate| candidate.key == modifier)
            .map(|candidate| candidate.keycode),
    }
}

fn translate(event_type: CGEventType, event: &CGEvent) -> Option<KeyEvent> {
    let keycode =
        u16::try_from(event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE)).ok()?;
    let key = key_for_keycode(keycode);
    match event_type {
        CGEventType::KeyDown => Some(KeyEvent::press(key)),
        CGEventType::KeyUp => Some(KeyEvent::release(key)),
        CGEventType::FlagsChanged => match ModifierKey::by_keycode(keycode) {
            Some(modifier) if !modifier.is_down(event.get_flags()) => Some(KeyEvent::release(key)),
            _ => Some(KeyEvent::press(key)),
        },
        _ => None,
    }
}

fn text_chunks(text: &str) -> Vec<Vec<u16>> {
    let mut chunks = Vec::new();
    let mut current: Vec<u16> = Vec::new();
    let mut buffer = [0u16; 2];
    for c in text.chars() {
        let units = c.encode_utf16(&mut buffer);
        if current.len() + units.len() > MAX_UTF16_UNITS_PER_EVENT {
            chunks.push(std::mem::take(&mut current));
        }
        current.extend_from_slice(units);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn new_key_event(keycode: u16, keydown: bool) -> Option<CGEvent> {
    let source = CGEventSource::new(CGEventSourceStateID::Private).ok()?;
    let event = CGEvent::new_keyboard_event(source, keycode, keydown).ok()?;
    event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, OWN_EVENT_MARKER);
    Some(event)
}

struct TapState {
    engine: Engine,
    downstream_flags: CGEventFlags,
}

impl TapState {
    fn on_event(
        &mut self,
        proxy: CGEventTapProxy,
        event_type: CGEventType,
        event: &CGEvent,
    ) -> CallbackResult {
        let Some(key_event) = translate(event_type, event) else {
            return CallbackResult::Keep;
        };
        match self.engine.handle(key_event, Instant::now()) {
            Response::Pass => {
                self.downstream_flags = event.get_flags();
                CallbackResult::Keep
            }
            Response::Replace(outputs) => {
                for output in &outputs {
                    self.post(output, proxy);
                }
                CallbackResult::Drop
            }
        }
    }

    fn post(&mut self, output: &Output, proxy: CGEventTapProxy) {
        match output {
            Output::Key(key_event) => self.post_key(*key_event, proxy),
            Output::Mask => {}
            Output::Text(text) => post_text(text, proxy),
        }
    }

    fn post_key(&mut self, key_event: KeyEvent, proxy: CGEventTapProxy) {
        let Some(keycode) = keycode_for_key(key_event.key) else {
            return;
        };
        let Some(event) = new_key_event(keycode, key_event.is_press()) else {
            return;
        };
        if let Some(modifier) = ModifierKey::by_keycode(keycode) {
            self.downstream_flags = modifier.apply(self.downstream_flags, key_event.action);
            event.set_type(CGEventType::FlagsChanged);
        }
        event.set_flags(self.downstream_flags);
        event.post_from_tap(proxy);
    }
}

fn post_text(text: &str, proxy: CGEventTapProxy) {
    for chunk in text_chunks(text) {
        for keydown in [true, false] {
            let Some(event) = new_key_event(0, keydown) else {
                return;
            };
            event.set_flags(CGEventFlags::CGEventFlagNull);
            event.set_string_from_utf16_unchecked(&chunk);
            event.post_from_tap(proxy);
        }
    }
}

fn is_own_event(event: &CGEvent) -> bool {
    event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == OWN_EVENT_MARKER
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> u8;
}

fn accessibility_granted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

#[derive(Default)]
struct Shared {
    stop: AtomicBool,
    run_loop: Mutex<Option<CFRunLoop>>,
}

impl Shared {
    fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(run_loop) = self.run_loop.lock()
            && let Some(run_loop) = run_loop.as_ref()
        {
            run_loop.stop();
        }
    }

    fn set_run_loop(&self, run_loop: Option<CFRunLoop>) {
        if let Ok(mut slot) = self.run_loop.lock() {
            *slot = run_loop;
        }
    }
}

pub struct MacBackend {
    shared: Arc<Shared>,
}

pub fn open() -> Result<MacBackend, PlatformError> {
    if !accessibility_granted() {
        return Err(PlatformError::AccessibilityPermission);
    }
    Ok(MacBackend {
        shared: Arc::new(Shared::default()),
    })
}

#[must_use]
pub fn supports_trigger(trigger: Trigger) -> bool {
    trigger != Trigger::CapsLock
}

impl KeyboardBackend for MacBackend {
    fn name(&self) -> &'static str {
        "macOS event tap"
    }

    fn shutdown_handle(&self) -> ShutdownHandle {
        let shared = Arc::clone(&self.shared);
        ShutdownHandle::new(move || shared.request_stop())
    }

    fn run(&mut self, engine: Engine) -> Result<(), PlatformError> {
        let state = RefCell::new(TapState {
            engine,
            downstream_flags: CGEventFlags::CGEventFlagNull,
        });
        let tap = CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            move |proxy, event_type, event| {
                if matches!(
                    event_type,
                    CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
                ) {
                    CFRunLoop::get_current().stop();
                    return CallbackResult::Keep;
                }
                if is_own_event(event) {
                    return CallbackResult::Keep;
                }
                match state.try_borrow_mut() {
                    Ok(mut state) => state.on_event(proxy, event_type, event),
                    Err(_) => CallbackResult::Keep,
                }
            },
        )
        .map_err(|()| PlatformError::EventTap)?;
        let source = tap
            .mach_port()
            .create_runloop_source(0)
            .map_err(|()| PlatformError::EventTap)?;
        let run_loop = CFRunLoop::get_current();
        let common_modes = unsafe { kCFRunLoopCommonModes };
        run_loop.add_source(&source, common_modes);
        tap.enable();
        self.shared.set_run_loop(Some(run_loop.clone()));
        let default_mode = unsafe { kCFRunLoopDefaultMode };
        while !self.shared.stop.load(Ordering::SeqCst) {
            CFRunLoop::run_in_mode(default_mode, STOP_POLL_INTERVAL, false);
            tap.enable();
        }
        self.shared.set_run_loop(None);
        run_loop.remove_source(&source, common_modes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keycodes_round_trip() {
        let mut keys: Vec<Key> = (0..=9)
            .filter_map(Digit::new)
            .map(Key::Digit)
            .chain((b'a'..=b'z').map(Key::Letter))
            .chain((1..=20).filter_map(Key::function))
            .collect();
        keys.extend([
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
            Key::Other(0x7E),
        ]);
        for key in keys {
            let keycode = keycode_for_key(key).unwrap();
            assert_eq!(key_for_keycode(keycode), key);
        }
    }

    #[test]
    fn function_keys_beyond_f20_have_no_keycode() {
        assert_eq!(keycode_for_key(Key::Function(21)), None);
    }

    #[test]
    fn keypad_enter_counts_as_enter() {
        assert_eq!(key_for_keycode(KEYCODE_KEYPAD_ENTER), Key::Enter);
    }

    #[test]
    fn modifier_flags_track_both_sides() {
        let left = ModifierKey::by_keycode(0x3A).unwrap();
        let right = ModifierKey::by_keycode(0x3D).unwrap();
        let both = right.apply(
            left.apply(CGEventFlags::CGEventFlagNull, KeyAction::Press),
            KeyAction::Press,
        );
        assert!(both.contains(CGEventFlags::CGEventFlagAlternate));
        let right_only = left.apply(both, KeyAction::Release);
        assert!(right_only.contains(CGEventFlags::CGEventFlagAlternate));
        let none = right.apply(right_only, KeyAction::Release);
        assert!(!none.contains(CGEventFlags::CGEventFlagAlternate));
        assert_eq!(none.bits() & 0x60, 0);
    }

    #[test]
    fn text_is_chunked_without_splitting_surrogates() {
        let text = "a".repeat(19) + "\u{1F600}" + "b";
        let chunks = text_chunks(&text);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 19);
        assert_eq!(chunks[1].len(), 3);
        assert_eq!(text_chunks(""), Vec::<Vec<u16>>::new());
    }
}
