use std::io;

use evdev::KeyCode;
use padless_core::KeyAction;

use super::VirtualKeyboard;
use super::keymap::HEX_DIGITS;

pub(super) enum TextOutput {
    UnicodeInput,
}

impl TextOutput {
    pub(super) fn detect() -> Self {
        Self::UnicodeInput
    }

    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::UnicodeInput => "Ctrl+Shift+U hex input",
        }
    }

    pub(super) fn type_text(
        &mut self,
        text: &str,
        keyboard: &mut VirtualKeyboard,
    ) -> io::Result<()> {
        match self {
            Self::UnicodeInput => type_unicode_input(text, keyboard),
        }
    }
}

fn type_unicode_input(text: &str, keyboard: &mut VirtualKeyboard) -> io::Result<()> {
    for c in text.chars() {
        keyboard.key(KeyCode::KEY_LEFTCTRL, KeyAction::Press)?;
        keyboard.key(KeyCode::KEY_LEFTSHIFT, KeyAction::Press)?;
        keyboard.tap(KeyCode::KEY_U)?;
        keyboard.key(KeyCode::KEY_LEFTSHIFT, KeyAction::Release)?;
        keyboard.key(KeyCode::KEY_LEFTCTRL, KeyAction::Release)?;
        for code in hex_keys(c) {
            keyboard.tap(code)?;
        }
        keyboard.tap(KeyCode::KEY_SPACE)?;
    }
    Ok(())
}

fn hex_keys(c: char) -> Vec<KeyCode> {
    format!("{:x}", u32::from(c))
        .chars()
        .filter_map(|digit| digit.to_digit(16))
        .filter_map(|value| usize::try_from(value).ok())
        .filter_map(|index| HEX_DIGITS.get(index).copied())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_keys_spell_the_code_point() {
        assert_eq!(hex_keys('\u{E9}'), [KeyCode::KEY_E, KeyCode::KEY_9]);
        assert_eq!(hex_keys('\0'), [KeyCode::KEY_0]);
        assert_eq!(
            hex_keys('\u{1F600}'),
            [
                KeyCode::KEY_1,
                KeyCode::KEY_F,
                KeyCode::KEY_6,
                KeyCode::KEY_0,
                KeyCode::KEY_0
            ]
        );
    }
}
