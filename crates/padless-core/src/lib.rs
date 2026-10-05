#![forbid(unsafe_code)]

mod code;
mod config;
mod engine;
mod key;
mod resolver;
mod table;

pub use code::{Code, InvalidCode, MAX_CODE_DIGITS};
pub use config::{
    Chord, Config, ConfigError, ConfigWarning, DEFAULT_LEADER_TIMEOUT, DEFAULT_MIN_DIGITS,
    InvalidChord, InvalidTrigger, LeaderConfig, MAX_LEADER_TIMEOUT, Mode, Trigger,
};
pub use engine::{Engine, Output, Response};
pub use key::{Digit, Key, KeyAction, KeyEvent, MAX_FUNCTION_KEY, Modifier, Modifiers, UnknownKey};
pub use resolver::{Resolution, Resolver};
pub use table::Table;
