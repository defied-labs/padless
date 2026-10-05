#![forbid(unsafe_code)]

mod code;
mod key;
mod table;

pub use code::{Code, InvalidCode, MAX_CODE_DIGITS};
pub use key::{Digit, Key, KeyAction, KeyEvent, MAX_FUNCTION_KEY, Modifier, Modifiers, UnknownKey};
pub use table::Table;
