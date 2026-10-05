use std::fmt;
use std::str::FromStr;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digit(u8);

impl Digit {
    pub const MAX: u8 = 9;

    #[must_use]
    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX {
            Some(Self(value))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn as_char(self) -> char {
        (b'0' + self.0) as char
    }

    #[must_use]
    pub fn from_char(c: char) -> Option<Self> {
        c.to_digit(10)
            .and_then(|value| u8::try_from(value).ok())
            .and_then(Self::new)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Digit(Digit),
    Letter(u8),
    Function(u8),
    LeftAlt,
    RightAlt,
    LeftCtrl,
    RightCtrl,
    LeftShift,
    RightShift,
    LeftMeta,
    RightMeta,
    CapsLock,
    Enter,
    Space,
    Escape,
    Other(u32),
}

pub const MAX_FUNCTION_KEY: u8 = 24;

impl Key {
    #[must_use]
    pub fn letter(c: char) -> Option<Self> {
        if c.is_ascii_lowercase() {
            u8::try_from(c).ok().map(Self::Letter)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn function(number: u8) -> Option<Self> {
        if number >= 1 && number <= MAX_FUNCTION_KEY {
            Some(Self::Function(number))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn digit(self) -> Option<Digit> {
        match self {
            Self::Digit(digit) => Some(digit),
            _ => None,
        }
    }

    #[must_use]
    pub const fn modifier(self) -> Option<Modifier> {
        match self {
            Self::LeftCtrl | Self::RightCtrl => Some(Modifier::Ctrl),
            Self::LeftAlt | Self::RightAlt => Some(Modifier::Alt),
            Self::LeftShift | Self::RightShift => Some(Modifier::Shift),
            Self::LeftMeta | Self::RightMeta => Some(Modifier::Meta),
            _ => None,
        }
    }

    #[must_use]
    pub const fn is_modifier(self) -> bool {
        self.modifier().is_some()
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Digit(digit) => write!(f, "{}", digit.as_char()),
            Self::Letter(letter) => write!(f, "{}", char::from(*letter)),
            Self::Function(number) => write!(f, "f{number}"),
            Self::LeftAlt => f.write_str("left-alt"),
            Self::RightAlt => f.write_str("right-alt"),
            Self::LeftCtrl => f.write_str("left-ctrl"),
            Self::RightCtrl => f.write_str("right-ctrl"),
            Self::LeftShift => f.write_str("left-shift"),
            Self::RightShift => f.write_str("right-shift"),
            Self::LeftMeta => f.write_str("left-meta"),
            Self::RightMeta => f.write_str("right-meta"),
            Self::CapsLock => f.write_str("caps-lock"),
            Self::Enter => f.write_str("enter"),
            Self::Space => f.write_str("space"),
            Self::Escape => f.write_str("escape"),
            Self::Other(code) => write!(f, "other-{code:#x}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown key name `{0}`")]
pub struct UnknownKey(pub String);

impl FromStr for Key {
    type Err = UnknownKey;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let named = match name {
            "left-alt" => Some(Self::LeftAlt),
            "right-alt" => Some(Self::RightAlt),
            "left-ctrl" => Some(Self::LeftCtrl),
            "right-ctrl" => Some(Self::RightCtrl),
            "left-shift" => Some(Self::LeftShift),
            "right-shift" => Some(Self::RightShift),
            "left-meta" => Some(Self::LeftMeta),
            "right-meta" => Some(Self::RightMeta),
            "caps-lock" => Some(Self::CapsLock),
            "enter" => Some(Self::Enter),
            "space" => Some(Self::Space),
            "escape" => Some(Self::Escape),
            _ => None,
        };
        named
            .or_else(|| single_char_key(name))
            .or_else(|| function_key(name))
            .ok_or_else(|| UnknownKey(name.to_owned()))
    }
}

fn single_char_key(name: &str) -> Option<Key> {
    let mut chars = name.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    Digit::from_char(c)
        .map(Key::Digit)
        .or_else(|| Key::letter(c))
}

fn function_key(name: &str) -> Option<Key> {
    let number = name.strip_prefix('f')?;
    if number.starts_with('0') {
        return None;
    }
    number.parse().ok().and_then(Key::function)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Meta,
}

impl Modifier {
    pub const ALL: [Self; 4] = [Self::Ctrl, Self::Alt, Self::Shift, Self::Meta];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ctrl => "ctrl",
            Self::Alt => "alt",
            Self::Shift => "shift",
            Self::Meta => "meta",
        }
    }

    const fn bit(self) -> u8 {
        match self {
            Self::Ctrl => 1,
            Self::Alt => 1 << 1,
            Self::Shift => 1 << 2,
            Self::Meta => 1 << 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);

    #[must_use]
    pub const fn with(self, modifier: Modifier) -> Self {
        Self(self.0 | modifier.bit())
    }

    #[must_use]
    pub const fn contains(self, modifier: Modifier) -> bool {
        self.0 & modifier.bit() != 0
    }

    pub fn iter(self) -> impl Iterator<Item = Modifier> {
        Modifier::ALL
            .into_iter()
            .filter(move |modifier| self.contains(*modifier))
    }
}

impl FromIterator<Modifier> for Modifiers {
    fn from_iter<I: IntoIterator<Item = Modifier>>(iter: I) -> Self {
        iter.into_iter().fold(Self::NONE, Self::with)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyAction {
    Press,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub key: Key,
    pub action: KeyAction,
}

impl KeyEvent {
    #[must_use]
    pub const fn press(key: Key) -> Self {
        Self {
            key,
            action: KeyAction::Press,
        }
    }

    #[must_use]
    pub const fn release(key: Key) -> Self {
        Self {
            key,
            action: KeyAction::Release,
        }
    }

    #[must_use]
    pub const fn is_press(self) -> bool {
        matches!(self.action, KeyAction::Press)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digit(value: u8) -> Digit {
        Digit::new(value).unwrap()
    }

    #[test]
    fn digit_rejects_values_above_nine() {
        assert!(Digit::new(9).is_some());
        assert!(Digit::new(10).is_none());
    }

    #[test]
    fn digit_round_trips_through_char() {
        for value in 0..=9 {
            assert_eq!(Digit::from_char(digit(value).as_char()), Some(digit(value)));
        }
        assert_eq!(Digit::from_char('a'), None);
        assert_eq!(Digit::from_char('\u{0663}'), None);
    }

    #[test]
    fn parses_named_keys() {
        assert_eq!("left-alt".parse(), Ok(Key::LeftAlt));
        assert_eq!("right-ctrl".parse(), Ok(Key::RightCtrl));
        assert_eq!("caps-lock".parse(), Ok(Key::CapsLock));
        assert_eq!("u".parse(), Ok(Key::Letter(b'u')));
        assert_eq!("7".parse(), Ok(Key::Digit(digit(7))));
        assert_eq!("f1".parse(), Ok(Key::Function(1)));
        assert_eq!("f24".parse(), Ok(Key::Function(24)));
    }

    #[test]
    fn rejects_invalid_key_names() {
        for name in [
            "",
            "U",
            "f0",
            "f25",
            "f01",
            "ab",
            "alt",
            "Left-Alt",
            "other-0x1",
        ] {
            assert_eq!(
                name.parse::<Key>(),
                Err(UnknownKey(name.to_owned())),
                "{name}"
            );
        }
    }

    #[test]
    fn display_round_trips_for_nameable_keys() {
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
            Key::Letter(b'q'),
            Key::Digit(digit(0)),
            Key::Function(12),
        ];
        for key in keys {
            assert_eq!(key.to_string().parse(), Ok(key));
        }
    }

    #[test]
    fn classifies_modifiers_regardless_of_side() {
        assert_eq!(Key::LeftCtrl.modifier(), Some(Modifier::Ctrl));
        assert_eq!(Key::RightCtrl.modifier(), Some(Modifier::Ctrl));
        assert_eq!(Key::RightMeta.modifier(), Some(Modifier::Meta));
        assert_eq!(Key::CapsLock.modifier(), None);
        assert!(!Key::Letter(b'a').is_modifier());
    }

    #[test]
    fn modifiers_collect_into_set() {
        let set: Modifiers = [Modifier::Ctrl, Modifier::Shift].into_iter().collect();
        assert!(set.contains(Modifier::Ctrl));
        assert!(set.contains(Modifier::Shift));
        assert!(!set.contains(Modifier::Alt));
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            [Modifier::Ctrl, Modifier::Shift]
        );
    }
}
