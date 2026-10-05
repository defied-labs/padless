use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Write as _};
use std::os::fd::AsFd;
use std::time::Instant;

use rustix::fs::{MemfdFlags, memfd_create};
use thiserror::Error;
use wayland_client::globals::{BindError, GlobalError, GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{
    ConnectError, Connection, Dispatch, DispatchError, EventQueue, QueueHandle, delegate_noop,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;

const KEYMAP_FORMAT_XKB_V1: u32 = 1;
const KEY_PRESSED: u32 = 1;
const KEY_RELEASED: u32 = 0;
const XKB_KEYCODE_OFFSET: u32 = 8;
const MAX_KEYS_PER_KEYMAP: usize = 200;

#[derive(Debug, Error)]
pub(super) enum WaylandError {
    #[error(transparent)]
    Connect(#[from] ConnectError),
    #[error(transparent)]
    Globals(#[from] GlobalError),
    #[error("the compositor does not offer zwp_virtual_keyboard_manager_v1: {0}")]
    Unsupported(#[from] BindError),
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
    #[error("failed to share the keymap with the compositor")]
    Keymap(#[from] io::Error),
}

struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ZwpVirtualKeyboardManagerV1);
delegate_noop!(State: ZwpVirtualKeyboardV1);

pub(super) struct WaylandTyper {
    queue: EventQueue<State>,
    keyboard: ZwpVirtualKeyboardV1,
    started: Instant,
}

impl WaylandTyper {
    pub(super) fn connect() -> Result<Self, WaylandError> {
        let connection = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<State>(&connection)?;
        let handle = queue.handle();
        let manager: ZwpVirtualKeyboardManagerV1 = globals.bind(&handle, 1..=1, ())?;
        let seat: wl_seat::WlSeat = globals.bind(&handle, 1..=1, ())?;
        let keyboard = manager.create_virtual_keyboard(&seat, &handle, ());
        queue.roundtrip(&mut State)?;
        Ok(Self {
            queue,
            keyboard,
            started: Instant::now(),
        })
    }

    pub(super) fn type_text(&mut self, text: &str) -> Result<(), WaylandError> {
        let chars: Vec<char> = text.chars().collect();
        let mut start = 0;
        while start < chars.len() {
            let mut layout = Layout::default();
            let mut end = start;
            while end < chars.len() && layout.assign(chars[end]).is_some() {
                end += 1;
            }
            self.upload(&layout)?;
            for c in &chars[start..end] {
                if let Some(key) = layout.key_of(*c) {
                    let time = self.timestamp();
                    self.keyboard.key(time, key, KEY_PRESSED);
                    self.keyboard.key(time, key, KEY_RELEASED);
                }
            }
            self.queue.roundtrip(&mut State)?;
            start = end;
        }
        Ok(())
    }

    fn upload(&self, layout: &Layout) -> Result<(), WaylandError> {
        let mut keymap = layout.keymap().into_bytes();
        keymap.push(0);
        let size = u32::try_from(keymap.len())
            .map_err(|_| io::Error::other("keymap exceeds the protocol size limit"))?;
        let descriptor =
            memfd_create("padless-keymap", MemfdFlags::CLOEXEC).map_err(io::Error::from)?;
        let mut file = File::from(descriptor);
        file.write_all(&keymap)?;
        self.keyboard
            .keymap(KEYMAP_FORMAT_XKB_V1, file.as_fd(), size);
        Ok(())
    }

    fn timestamp(&self) -> u32 {
        let millis = self.started.elapsed().as_millis() % (u128::from(u32::MAX) + 1);
        u32::try_from(millis).unwrap_or(0)
    }
}

impl Drop for WaylandTyper {
    fn drop(&mut self) {
        self.keyboard.destroy();
        let _ = self.queue.flush();
    }
}

#[derive(Default)]
struct Layout {
    chars: Vec<char>,
}

impl Layout {
    fn assign(&mut self, c: char) -> Option<u32> {
        if let Some(key) = self.key_of(c) {
            return Some(key);
        }
        if self.chars.len() >= MAX_KEYS_PER_KEYMAP {
            return None;
        }
        self.chars.push(c);
        self.key_of(c)
    }

    fn key_of(&self, c: char) -> Option<u32> {
        let index = self.chars.iter().position(|candidate| *candidate == c)?;
        u32::try_from(index + 1).ok()
    }

    fn keymap(&self) -> String {
        let maximum = XKB_KEYCODE_OFFSET + u32::try_from(self.chars.len()).unwrap_or(0) + 1;
        let mut keycodes = String::new();
        let mut symbols = String::new();
        for (index, c) in self.chars.iter().enumerate() {
            let number = index + 1;
            let _ = writeln!(
                keycodes,
                "<K{number}> = {};",
                u32::try_from(number).unwrap_or(0) + XKB_KEYCODE_OFFSET
            );
            let _ = writeln!(symbols, "key <K{number}> {{[{}]}};", keysym_name(*c));
        }
        format!(
            "xkb_keymap {{\n\
             xkb_keycodes \"padless\" {{\nminimum = {XKB_KEYCODE_OFFSET};\nmaximum = {maximum};\n{keycodes}}};\n\
             xkb_types \"padless\" {{ include \"complete\" }};\n\
             xkb_compatibility \"padless\" {{ include \"complete\" }};\n\
             xkb_symbols \"padless\" {{\n{symbols}}};\n\
             }};\n"
        )
    }
}

fn keysym_name(c: char) -> String {
    match c {
        '\u{8}' => "BackSpace".to_owned(),
        '\t' => "Tab".to_owned(),
        '\n' | '\r' => "Return".to_owned(),
        '\u{1B}' => "Escape".to_owned(),
        '\u{7F}' => "Delete".to_owned(),
        _ => format!("U{:04X}", u32::from(c)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_reuses_keys_for_repeated_characters() {
        let mut layout = Layout::default();
        assert_eq!(layout.assign('a'), Some(1));
        assert_eq!(layout.assign('\u{E9}'), Some(2));
        assert_eq!(layout.assign('a'), Some(1));
        assert_eq!(layout.key_of('z'), None);
    }

    #[test]
    fn layout_is_bounded() {
        let mut layout = Layout::default();
        for value in 0..MAX_KEYS_PER_KEYMAP {
            let c = char::from_u32(0x4E00 + u32::try_from(value).unwrap()).unwrap();
            assert!(layout.assign(c).is_some());
        }
        assert_eq!(layout.assign('\u{1F600}'), None);
        assert!(layout.assign('\u{4E00}').is_some());
    }

    #[test]
    fn keymap_binds_each_character() {
        let mut layout = Layout::default();
        layout.assign('\u{E9}');
        layout.assign('\u{1F600}');
        layout.assign('\t');
        let keymap = layout.keymap();
        assert!(keymap.contains("maximum = 12;"));
        assert!(keymap.contains("<K1> = 9;"));
        assert!(keymap.contains("<K3> = 11;"));
        assert!(keymap.contains("key <K1> {[U00E9]};"));
        assert!(keymap.contains("key <K2> {[U1F600]};"));
        assert!(keymap.contains("key <K3> {[Tab]};"));
    }
}
