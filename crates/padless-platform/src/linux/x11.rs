use std::thread;
use std::time::Duration;

use thiserror::Error;
use x11rb::CURRENT_TIME;
use x11rb::connection::{Connection, RequestConnection};
use x11rb::errors::{ConnectError, ConnectionError, ReplyError};
use x11rb::protocol::xproto::{
    ConnectionExt as _, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, Keycode, Keysym, Window,
};
use x11rb::protocol::xtest::{self, ConnectionExt as _};
use x11rb::rust_connection::RustConnection;

const MAX_SCRATCH_KEYCODES: usize = 16;
const NO_SYMBOL: Keysym = 0;
const UNICODE_KEYSYM_BASE: Keysym = 0x0100_0000;
const KEYCODE_REUSE_DELAY: Duration = Duration::from_millis(20);

#[derive(Debug, Error)]
pub(super) enum X11Error {
    #[error(transparent)]
    Connect(#[from] ConnectError),
    #[error(transparent)]
    Connection(#[from] ConnectionError),
    #[error(transparent)]
    Reply(#[from] ReplyError),
    #[error("the X server does not support the XTEST extension")]
    MissingXTest,
    #[error("the X server reported no screen for this display")]
    MissingScreen,
    #[error("the keyboard mapping has no unused keycode to borrow")]
    NoFreeKeycode,
}

pub(super) struct X11Typer {
    connection: RustConnection,
    root: Window,
    scratch: Vec<Keycode>,
    keysyms_per_keycode: u8,
}

impl X11Typer {
    pub(super) fn connect() -> Result<Self, X11Error> {
        let (connection, screen) = x11rb::connect(None)?;
        if connection
            .extension_information(xtest::X11_EXTENSION_NAME)?
            .is_none()
        {
            return Err(X11Error::MissingXTest);
        }
        let setup = connection.setup();
        let root = setup
            .roots
            .get(screen)
            .map(|screen| screen.root)
            .ok_or(X11Error::MissingScreen)?;
        let (first, last) = (setup.min_keycode, setup.max_keycode);
        let count = last
            .checked_sub(first)
            .and_then(|span| span.checked_add(1))
            .ok_or(X11Error::NoFreeKeycode)?;
        let mapping = connection.get_keyboard_mapping(first, count)?.reply()?;
        let keysyms_per_keycode = mapping.keysyms_per_keycode;
        if keysyms_per_keycode == 0 {
            return Err(X11Error::NoFreeKeycode);
        }
        let scratch: Vec<Keycode> = mapping
            .keysyms
            .chunks(usize::from(keysyms_per_keycode))
            .zip(first..=last)
            .filter(|(keysyms, _)| keysyms.iter().all(|keysym| *keysym == NO_SYMBOL))
            .map(|(_, keycode)| keycode)
            .rev()
            .take(MAX_SCRATCH_KEYCODES)
            .collect();
        if scratch.is_empty() {
            return Err(X11Error::NoFreeKeycode);
        }
        Ok(Self {
            connection,
            root,
            scratch,
            keysyms_per_keycode,
        })
    }

    pub(super) fn type_text(&mut self, text: &str) -> Result<(), X11Error> {
        for (index, c) in text.chars().enumerate() {
            let slot = index % self.scratch.len();
            if index > 0 && slot == 0 {
                self.sync()?;
                thread::sleep(KEYCODE_REUSE_DELAY);
            }
            let keycode = self.scratch[slot];
            self.remap(keycode, keysym_for(c))?;
            self.sync()?;
            for event in [KEY_PRESS_EVENT, KEY_RELEASE_EVENT] {
                self.connection.xtest_fake_input(
                    event,
                    keycode,
                    CURRENT_TIME,
                    self.root,
                    0,
                    0,
                    0,
                )?;
            }
        }
        self.sync()
    }

    fn remap(&self, keycode: Keycode, keysym: Keysym) -> Result<(), X11Error> {
        let keysyms = vec![keysym; usize::from(self.keysyms_per_keycode)];
        self.connection
            .change_keyboard_mapping(1, keycode, self.keysyms_per_keycode, &keysyms)?;
        Ok(())
    }

    fn sync(&self) -> Result<(), X11Error> {
        self.connection.get_input_focus()?.reply()?;
        Ok(())
    }
}

impl Drop for X11Typer {
    fn drop(&mut self) {
        for keycode in &self.scratch {
            let _ = self.remap(*keycode, NO_SYMBOL);
        }
        let _ = self.connection.flush();
    }
}

fn keysym_for(c: char) -> Keysym {
    match c {
        '\u{8}' => 0xFF08,
        '\t' => 0xFF09,
        '\n' | '\r' => 0xFF0D,
        '\u{1B}' => 0xFF1B,
        '\u{7F}' => 0xFFFF,
        ' '..='~' | '\u{A0}'..='\u{FF}' => u32::from(c),
        _ => UNICODE_KEYSYM_BASE | u32::from(c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin1_keysyms_equal_code_points() {
        assert_eq!(keysym_for('A'), 0x41);
        assert_eq!(keysym_for('\u{E9}'), 0xE9);
        assert_eq!(keysym_for('\u{A0}'), 0xA0);
    }

    #[test]
    fn other_characters_use_unicode_keysyms() {
        assert_eq!(keysym_for('\u{20AC}'), 0x0100_20AC);
        assert_eq!(keysym_for('\u{1F600}'), 0x0101_F600);
        assert_eq!(keysym_for('\u{85}'), 0x0100_0085);
    }

    #[test]
    fn control_characters_use_function_keysyms() {
        assert_eq!(keysym_for('\t'), 0xFF09);
        assert_eq!(keysym_for('\r'), 0xFF0D);
    }
}
