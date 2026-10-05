mod keymap;
mod text;
mod wayland;
mod x11;

use std::fs;
use std::io;
use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, Device, EventType, InputEvent, KeyCode};
use padless_core::{Engine, KeyAction, KeyEvent, Output, Response, Trigger};
use rustix::event::{EventfdFlags, PollFd, PollFlags, eventfd, poll};
use tracing::{debug, info, warn};

use crate::{KeyboardBackend, PlatformError, ShutdownHandle};

use text::TextOutput;

const VIRTUAL_DEVICE_NAME: &str = "padless virtual keyboard";
const INPUT_DIRECTORY: &str = "/dev/input";
const GRAB_IDLE_TIMEOUT: Duration = Duration::from_secs(5);
const GRAB_IDLE_POLL: Duration = Duration::from_millis(10);
const ENODEV: i32 = 19;

const KEYBOARD_PROBE_KEYS: [KeyCode; 5] = [
    KeyCode::KEY_A,
    KeyCode::KEY_Z,
    KeyCode::KEY_1,
    KeyCode::KEY_0,
    KeyCode::KEY_ENTER,
];

struct Keyboard {
    device: Device,
    name: String,
}

fn probe(path: &Path) -> io::Result<Option<Keyboard>> {
    let device = Device::open(path)?;
    let name = device.name().unwrap_or("unnamed device").to_owned();
    if name.starts_with(VIRTUAL_DEVICE_NAME) {
        return Ok(None);
    }
    let has_keyboard_keys = device.supported_keys().is_some_and(|keys| {
        KEYBOARD_PROBE_KEYS
            .iter()
            .all(|probe_key| keys.contains(*probe_key))
    });
    if !has_keyboard_keys {
        return Ok(None);
    }
    let events = device.supported_events();
    if events.contains(EventType::RELATIVE) || events.contains(EventType::ABSOLUTE) {
        warn!("skipping {name}: it reports pointer events as well as keys");
        return Ok(None);
    }
    Ok(Some(Keyboard { device, name }))
}

fn discover_keyboards() -> Result<Vec<Keyboard>, PlatformError> {
    let entries = fs::read_dir(INPUT_DIRECTORY).map_err(|source| PlatformError::Io {
        action: "list /dev/input",
        source,
    })?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("event"))
        })
        .collect();
    paths.sort();
    let mut keyboards = Vec::new();
    let mut permission_denied = false;
    for path in paths {
        match probe(&path) {
            Ok(Some(keyboard)) => keyboards.push(keyboard),
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                permission_denied = true;
            }
            Err(error) => debug!("cannot open {}: {error}", path.display()),
        }
    }
    if keyboards.is_empty() {
        return Err(if permission_denied {
            PlatformError::InputPermission
        } else {
            PlatformError::NoKeyboards
        });
    }
    Ok(keyboards)
}

fn grab_when_idle(keyboard: &mut Keyboard) -> Result<(), PlatformError> {
    let deadline = Instant::now() + GRAB_IDLE_TIMEOUT;
    loop {
        let keys_down = keyboard
            .device
            .get_key_state()
            .is_ok_and(|state| state.iter().next().is_some());
        if !keys_down {
            break;
        }
        if Instant::now() >= deadline {
            warn!(
                "{} still has keys held after {}s; grabbing anyway",
                keyboard.name,
                GRAB_IDLE_TIMEOUT.as_secs()
            );
            break;
        }
        thread::sleep(GRAB_IDLE_POLL);
    }
    keyboard
        .device
        .grab()
        .map_err(|source| PlatformError::Grab {
            device: keyboard.name.clone(),
            source,
        })?;
    keyboard
        .device
        .set_nonblocking(true)
        .map_err(|source| PlatformError::Io {
            action: "configure a keyboard device",
            source,
        })
}

pub(super) struct VirtualKeyboard {
    device: VirtualDevice,
}

impl VirtualKeyboard {
    fn create(keyboards: &[Keyboard]) -> Result<Self, PlatformError> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for keyboard in keyboards {
            if let Some(supported) = keyboard.device.supported_keys() {
                for key in supported {
                    keys.insert(key);
                }
            }
        }
        for key in keymap::emitted_by_padless() {
            keys.insert(key);
        }
        let device = VirtualDevice::builder()
            .and_then(|builder| builder.name(VIRTUAL_DEVICE_NAME).with_keys(&keys))
            .and_then(evdev::uinput::VirtualDeviceBuilder::build)
            .map_err(|error| match error.kind() {
                io::ErrorKind::PermissionDenied => PlatformError::UinputPermission,
                io::ErrorKind::NotFound => PlatformError::UinputMissing,
                _ => PlatformError::Io {
                    action: "create the uinput device",
                    source: error,
                },
            })?;
        Ok(Self { device })
    }

    pub(super) fn key(&mut self, code: KeyCode, action: KeyAction) -> io::Result<()> {
        let value = match action {
            KeyAction::Press => 1,
            KeyAction::Release => 0,
        };
        self.device
            .emit(&[InputEvent::new(EventType::KEY.0, code.code(), value)])
    }

    pub(super) fn tap(&mut self, code: KeyCode) -> io::Result<()> {
        self.key(code, KeyAction::Press)?;
        self.key(code, KeyAction::Release)
    }

    fn forward(&mut self, event: InputEvent) -> io::Result<()> {
        self.device.emit(&[event])
    }
}

pub struct LinuxBackend {
    keyboards: Vec<Keyboard>,
    output: VirtualKeyboard,
    text: TextOutput,
    wake: Arc<OwnedFd>,
}

pub fn open() -> Result<LinuxBackend, PlatformError> {
    let mut keyboards = discover_keyboards()?;
    let output = VirtualKeyboard::create(&keyboards)?;
    for keyboard in &mut keyboards {
        grab_when_idle(keyboard)?;
        info!("capturing {}", keyboard.name);
    }
    let wake = eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK).map_err(|errno| {
        PlatformError::Io {
            action: "create the shutdown eventfd",
            source: errno.into(),
        }
    })?;
    let text = TextOutput::detect();
    info!("typing text through {}", text.name());
    Ok(LinuxBackend {
        keyboards,
        output,
        text,
        wake: Arc::new(wake),
    })
}

#[must_use]
pub fn supports_trigger(_trigger: Trigger) -> bool {
    true
}

impl LinuxBackend {
    fn wait_for_input(&self) -> Result<Option<Vec<bool>>, PlatformError> {
        let mut fds = Vec::with_capacity(self.keyboards.len() + 1);
        fds.push(PollFd::new(&*self.wake, PollFlags::IN));
        for keyboard in &self.keyboards {
            fds.push(PollFd::new(&keyboard.device, PollFlags::IN));
        }
        loop {
            match poll(&mut fds, None) {
                Ok(_) => break,
                Err(rustix::io::Errno::INTR) => {}
                Err(errno) => {
                    return Err(PlatformError::Io {
                        action: "wait for keyboard input",
                        source: errno.into(),
                    });
                }
            }
        }
        if !fds[0].revents().is_empty() {
            return Ok(None);
        }
        Ok(Some(
            fds[1..].iter().map(|fd| !fd.revents().is_empty()).collect(),
        ))
    }

    fn drain(&mut self, index: usize, engine: &mut Engine) -> Result<(), PlatformError> {
        let fetched: io::Result<Vec<InputEvent>> = self.keyboards[index]
            .device
            .fetch_events()
            .map(Iterator::collect);
        let events = match fetched {
            Ok(events) => events,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) if error.raw_os_error() == Some(ENODEV) => {
                let keyboard = self.keyboards.remove(index);
                warn!("{} was disconnected", keyboard.name);
                return Ok(());
            }
            Err(source) => {
                return Err(PlatformError::Io {
                    action: "read keyboard events",
                    source,
                });
            }
        };
        for event in events {
            if event.event_type() != EventType::KEY {
                continue;
            }
            let key_event = KeyEvent {
                key: keymap::key_for_code(KeyCode::new(event.code())),
                action: if event.value() == 0 {
                    KeyAction::Release
                } else {
                    KeyAction::Press
                },
            };
            let result = match engine.handle(key_event, Instant::now()) {
                Response::Pass => self.output.forward(event),
                Response::Replace(outputs) => self.apply(&outputs),
            };
            result.map_err(|source| PlatformError::Io {
                action: "write to the uinput device",
                source,
            })?;
        }
        Ok(())
    }

    fn apply(&mut self, outputs: &[Output]) -> io::Result<()> {
        let mut after_key_events = false;
        for output in outputs {
            match output {
                Output::Key(event) => {
                    if let Some(code) = keymap::code_for_key(event.key) {
                        self.output.key(code, event.action)?;
                        after_key_events = true;
                    }
                }
                Output::Mask => {
                    self.output.tap(keymap::MASK)?;
                    after_key_events = true;
                }
                Output::Text(text) => {
                    self.text
                        .type_text(text, &mut self.output, after_key_events)?;
                }
            }
        }
        Ok(())
    }
}

impl KeyboardBackend for LinuxBackend {
    fn name(&self) -> &'static str {
        "evdev grab with uinput passthrough"
    }

    fn shutdown_handle(&self) -> ShutdownHandle {
        let wake = Arc::clone(&self.wake);
        ShutdownHandle::new(move || {
            let _ = rustix::io::write(&*wake, &1u64.to_ne_bytes());
        })
    }

    fn run(&mut self, mut engine: Engine) -> Result<(), PlatformError> {
        while let Some(ready) = self.wait_for_input()? {
            for index in (0..ready.len()).rev() {
                if ready[index] {
                    self.drain(index, &mut engine)?;
                }
            }
            if self.keyboards.is_empty() {
                return Err(PlatformError::NoKeyboards);
            }
        }
        Ok(())
    }
}
