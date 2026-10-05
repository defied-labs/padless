use std::env;
use std::io;
use std::thread;
use std::time::Duration;

use evdev::KeyCode;
use padless_core::KeyAction;
use tracing::{info, warn};

use super::VirtualKeyboard;
use super::keymap::HEX_DIGITS;
use super::x11::X11Typer;

const MODIFIER_SETTLE: Duration = Duration::from_millis(15);

pub(super) enum TextOutput {
    X11(Box<X11Typer>),
    UnicodeInput,
}

impl TextOutput {
    pub(super) fn detect() -> Self {
        if is_x11_session() {
            match X11Typer::connect() {
                Ok(typer) => return Self::X11(Box::new(typer)),
                Err(error) => info!("X11 text output unavailable: {error}"),
            }
        }
        Self::UnicodeInput
    }

    pub(super) fn name(&self) -> &'static str {
        match self {
            Self::X11(_) => "X11 XTest",
            Self::UnicodeInput => "Ctrl+Shift+U hex input",
        }
    }

    pub(super) fn type_text(
        &mut self,
        text: &str,
        keyboard: &mut VirtualKeyboard,
        after_key_events: bool,
    ) -> io::Result<()> {
        match self {
            Self::X11(typer) => {
                if after_key_events {
                    thread::sleep(MODIFIER_SETTLE);
                }
                match typer.type_text(text) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        warn!("X11 text output failed, falling back to Ctrl+Shift+U: {error}");
                        *self = Self::UnicodeInput;
                        type_unicode_input(text, keyboard)
                    }
                }
            }
            Self::UnicodeInput => type_unicode_input(text, keyboard),
        }
    }
}

fn is_x11_session() -> bool {
    match env::var("XDG_SESSION_TYPE").as_deref() {
        Ok("x11") => true,
        Ok("wayland") => false,
        _ => env::var_os("DISPLAY").is_some() && env::var_os("WAYLAND_DISPLAY").is_none(),
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
