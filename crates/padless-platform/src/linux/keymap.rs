use evdev::KeyCode;
use padless_core::{Digit, Key};

const DIGIT_ROW: [KeyCode; 10] = [
    KeyCode::KEY_0,
    KeyCode::KEY_1,
    KeyCode::KEY_2,
    KeyCode::KEY_3,
    KeyCode::KEY_4,
    KeyCode::KEY_5,
    KeyCode::KEY_6,
    KeyCode::KEY_7,
    KeyCode::KEY_8,
    KeyCode::KEY_9,
];

const LETTERS: [KeyCode; 26] = [
    KeyCode::KEY_A,
    KeyCode::KEY_B,
    KeyCode::KEY_C,
    KeyCode::KEY_D,
    KeyCode::KEY_E,
    KeyCode::KEY_F,
    KeyCode::KEY_G,
    KeyCode::KEY_H,
    KeyCode::KEY_I,
    KeyCode::KEY_J,
    KeyCode::KEY_K,
    KeyCode::KEY_L,
    KeyCode::KEY_M,
    KeyCode::KEY_N,
    KeyCode::KEY_O,
    KeyCode::KEY_P,
    KeyCode::KEY_Q,
    KeyCode::KEY_R,
    KeyCode::KEY_S,
    KeyCode::KEY_T,
    KeyCode::KEY_U,
    KeyCode::KEY_V,
    KeyCode::KEY_W,
    KeyCode::KEY_X,
    KeyCode::KEY_Y,
    KeyCode::KEY_Z,
];

const FUNCTION_KEYS: [KeyCode; 24] = [
    KeyCode::KEY_F1,
    KeyCode::KEY_F2,
    KeyCode::KEY_F3,
    KeyCode::KEY_F4,
    KeyCode::KEY_F5,
    KeyCode::KEY_F6,
    KeyCode::KEY_F7,
    KeyCode::KEY_F8,
    KeyCode::KEY_F9,
    KeyCode::KEY_F10,
    KeyCode::KEY_F11,
    KeyCode::KEY_F12,
    KeyCode::KEY_F13,
    KeyCode::KEY_F14,
    KeyCode::KEY_F15,
    KeyCode::KEY_F16,
    KeyCode::KEY_F17,
    KeyCode::KEY_F18,
    KeyCode::KEY_F19,
    KeyCode::KEY_F20,
    KeyCode::KEY_F21,
    KeyCode::KEY_F22,
    KeyCode::KEY_F23,
    KeyCode::KEY_F24,
];

const NAMED: [(KeyCode, Key); 12] = [
    (KeyCode::KEY_LEFTALT, Key::LeftAlt),
    (KeyCode::KEY_RIGHTALT, Key::RightAlt),
    (KeyCode::KEY_LEFTCTRL, Key::LeftCtrl),
    (KeyCode::KEY_RIGHTCTRL, Key::RightCtrl),
    (KeyCode::KEY_LEFTSHIFT, Key::LeftShift),
    (KeyCode::KEY_RIGHTSHIFT, Key::RightShift),
    (KeyCode::KEY_LEFTMETA, Key::LeftMeta),
    (KeyCode::KEY_RIGHTMETA, Key::RightMeta),
    (KeyCode::KEY_CAPSLOCK, Key::CapsLock),
    (KeyCode::KEY_ENTER, Key::Enter),
    (KeyCode::KEY_SPACE, Key::Space),
    (KeyCode::KEY_ESC, Key::Escape),
];

pub(super) const HEX_DIGITS: [KeyCode; 16] = [
    KeyCode::KEY_0,
    KeyCode::KEY_1,
    KeyCode::KEY_2,
    KeyCode::KEY_3,
    KeyCode::KEY_4,
    KeyCode::KEY_5,
    KeyCode::KEY_6,
    KeyCode::KEY_7,
    KeyCode::KEY_8,
    KeyCode::KEY_9,
    KeyCode::KEY_A,
    KeyCode::KEY_B,
    KeyCode::KEY_C,
    KeyCode::KEY_D,
    KeyCode::KEY_E,
    KeyCode::KEY_F,
];

pub(super) const MASK: KeyCode = KeyCode::KEY_UNKNOWN;

pub(super) fn key_for_code(code: KeyCode) -> Key {
    if let Some(position) = DIGIT_ROW.iter().position(|candidate| *candidate == code) {
        return u8::try_from(position)
            .ok()
            .and_then(Digit::new)
            .map_or(Key::Other(u32::from(code.code())), Key::Digit);
    }
    if let Some(position) = LETTERS.iter().position(|candidate| *candidate == code) {
        return u8::try_from(position)
            .ok()
            .and_then(|offset| b'a'.checked_add(offset))
            .map_or(Key::Other(u32::from(code.code())), Key::Letter);
    }
    if let Some(position) = FUNCTION_KEYS
        .iter()
        .position(|candidate| *candidate == code)
    {
        return u8::try_from(position + 1)
            .ok()
            .and_then(Key::function)
            .unwrap_or(Key::Other(u32::from(code.code())));
    }
    if code == KeyCode::KEY_KPENTER {
        return Key::Enter;
    }
    NAMED
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map_or(Key::Other(u32::from(code.code())), |(_, key)| *key)
}

pub(super) fn code_for_key(key: Key) -> Option<KeyCode> {
    match key {
        Key::Digit(digit) => DIGIT_ROW.get(usize::from(digit.value())).copied(),
        Key::Letter(letter) => letter
            .checked_sub(b'a')
            .and_then(|offset| LETTERS.get(usize::from(offset)))
            .copied(),
        Key::Function(number) => usize::from(number)
            .checked_sub(1)
            .and_then(|index| FUNCTION_KEYS.get(index))
            .copied(),
        Key::Other(raw) => u16::try_from(raw).ok().map(KeyCode::new),
        named => NAMED
            .iter()
            .find(|(_, candidate)| *candidate == named)
            .map(|(code, _)| *code),
    }
}

pub(super) fn emitted_by_padless() -> impl Iterator<Item = KeyCode> {
    HEX_DIGITS.into_iter().chain([
        MASK,
        KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_LEFTSHIFT,
        KeyCode::KEY_U,
        KeyCode::KEY_SPACE,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        let keys = (0..=9)
            .filter_map(Digit::new)
            .map(Key::Digit)
            .chain((b'a'..=b'z').map(Key::Letter))
            .chain((1..=24).filter_map(Key::function))
            .chain(NAMED.iter().map(|(_, key)| *key))
            .chain([Key::Other(u32::from(KeyCode::KEY_TAB.code()))]);
        for key in keys {
            assert_eq!(key_for_code(code_for_key(key).unwrap()), key);
        }
    }

    #[test]
    fn keypad_keys_are_not_digit_row() {
        assert!(matches!(key_for_code(KeyCode::KEY_KP1), Key::Other(_)));
        assert_eq!(key_for_code(KeyCode::KEY_KPENTER), Key::Enter);
    }

    #[test]
    fn hex_digits_cover_lowercase_hex() {
        for (value, code) in HEX_DIGITS.iter().enumerate() {
            let c = char::from_digit(u32::try_from(value).unwrap(), 16).unwrap();
            let key = key_for_code(*code);
            assert_eq!(key.to_string(), c.to_string());
        }
    }
}
