use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use serde::Deserialize;
use thiserror::Error;

use crate::code::{Code, MAX_CODE_DIGITS};
use crate::key::{Key, Modifier, Modifiers};
use crate::resolver::Resolver;
use crate::table::Table;

pub const DEFAULT_MIN_DIGITS: usize = 2;
pub const DEFAULT_LEADER_TIMEOUT: Duration = Duration::from_secs(3);
pub const MAX_LEADER_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Hold,
    Leader,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Hold => "hold",
            Self::Leader => "leader",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize)]
#[serde(try_from = "String")]
pub enum Trigger {
    #[default]
    LeftAlt,
    RightAlt,
    LeftCtrl,
    RightCtrl,
    LeftMeta,
    RightMeta,
    CapsLock,
}

impl Trigger {
    pub const ALL: [Self; 7] = [
        Self::LeftAlt,
        Self::RightAlt,
        Self::LeftCtrl,
        Self::RightCtrl,
        Self::LeftMeta,
        Self::RightMeta,
        Self::CapsLock,
    ];

    #[must_use]
    pub const fn key(self) -> Key {
        match self {
            Self::LeftAlt => Key::LeftAlt,
            Self::RightAlt => Key::RightAlt,
            Self::LeftCtrl => Key::LeftCtrl,
            Self::RightCtrl => Key::RightCtrl,
            Self::LeftMeta => Key::LeftMeta,
            Self::RightMeta => Key::RightMeta,
            Self::CapsLock => Key::CapsLock,
        }
    }

    #[must_use]
    pub const fn passes_through(self) -> bool {
        !matches!(self, Self::CapsLock)
    }
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.key().fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("`{0}` cannot be a trigger; expected one of {names}", names = trigger_names())]
pub struct InvalidTrigger(String);

fn trigger_names() -> String {
    Trigger::ALL
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

impl FromStr for Trigger {
    type Err = InvalidTrigger;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|trigger| trigger.to_string() == name)
            .ok_or_else(|| InvalidTrigger(name.to_owned()))
    }
}

impl TryFrom<String> for Trigger {
    type Error = InvalidTrigger;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        name.parse()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(try_from = "String")]
pub struct Chord {
    pub modifiers: Modifiers,
    pub key: Key,
}

impl Chord {
    pub const DEFAULT: Self = Self {
        modifiers: Modifiers::NONE.with(Modifier::Ctrl).with(Modifier::Shift),
        key: Key::Letter(b'u'),
    };
}

impl Default for Chord {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in self.modifiers.iter() {
            write!(f, "{}+", modifier.name())?;
        }
        self.key.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InvalidChord {
    #[error("chord is empty")]
    Empty,
    #[error("unknown modifier `{0}` in chord; expected ctrl, alt, shift or meta")]
    UnknownModifier(String),
    #[error("modifier `{0}` appears more than once in chord")]
    DuplicateModifier(String),
    #[error("`{0}` cannot end a chord; expected a letter, a digit or f1-f24")]
    InvalidKey(String),
}

impl FromStr for Chord {
    type Err = InvalidChord;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut parts: Vec<&str> = text.split('+').collect();
        let key_name = parts
            .pop()
            .filter(|name| !name.is_empty())
            .ok_or(InvalidChord::Empty)?;
        let key = key_name
            .parse::<Key>()
            .ok()
            .filter(|key| matches!(key, Key::Letter(_) | Key::Digit(_) | Key::Function(_)))
            .ok_or_else(|| InvalidChord::InvalidKey(key_name.to_owned()))?;
        let mut modifiers = Modifiers::NONE;
        for name in parts {
            let modifier = Modifier::ALL
                .into_iter()
                .find(|modifier| modifier.name() == name)
                .ok_or_else(|| InvalidChord::UnknownModifier(name.to_owned()))?;
            if modifiers.contains(modifier) {
                return Err(InvalidChord::DuplicateModifier(name.to_owned()));
            }
            modifiers = modifiers.with(modifier);
        }
        Ok(Self { modifiers, key })
    }
}

impl TryFrom<String> for Chord {
    type Error = InvalidChord;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderConfig {
    pub chord: Chord,
    pub timeout: Duration,
}

impl Default for LeaderConfig {
    fn default() -> Self {
        Self {
            chord: Chord::DEFAULT,
            timeout: DEFAULT_LEADER_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub mode: Mode,
    pub table: Table,
    pub trigger: Trigger,
    pub min_digits: usize,
    pub leader: LeaderConfig,
    pub aliases: BTreeMap<Code, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Hold,
            table: Table::Windows,
            trigger: Trigger::LeftAlt,
            min_digits: DEFAULT_MIN_DIGITS,
            leader: LeaderConfig::default(),
            aliases: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Syntax(#[from] toml::de::Error),
    #[error("min_digits must be between 1 and {MAX_CODE_DIGITS}, got {0}")]
    MinDigits(usize),
    #[error("leader.timeout_ms must be between 1 and {max}, got {0}", max = MAX_LEADER_TIMEOUT.as_millis())]
    LeaderTimeout(u64),
    #[error("alias for code {0} is empty")]
    EmptyAlias(Code),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigWarning {
    TriggerIsAltGr,
    ChordMatchesAltGr,
    SingleDigitShortcutsCaptured,
}

impl fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TriggerIsAltGr => {
                "trigger right-alt is AltGr on many non-US layouts; \
                 holding it to type layout characters such as @ or { will start a code instead"
            }
            Self::ChordMatchesAltGr => {
                "Windows reports AltGr as ctrl+alt, \
                 so this leader chord also matches AltGr with the same key"
            }
            Self::SingleDigitShortcutsCaptured => {
                "min_digits = 1 captures single-digit shortcuts such as alt+1"
            }
        })
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ConfigFile {
    mode: Mode,
    table: Table,
    trigger: Trigger,
    min_digits: usize,
    leader: LeaderFile,
    aliases: BTreeMap<Code, String>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        let defaults = Config::default();
        Self {
            mode: defaults.mode,
            table: defaults.table,
            trigger: defaults.trigger,
            min_digits: defaults.min_digits,
            leader: LeaderFile::default(),
            aliases: defaults.aliases,
        }
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LeaderFile {
    chord: Chord,
    timeout_ms: u64,
}

impl Default for LeaderFile {
    fn default() -> Self {
        Self {
            chord: Chord::DEFAULT,
            timeout_ms: duration_millis(DEFAULT_LEADER_TIMEOUT),
        }
    }
}

fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let file: ConfigFile = toml::from_str(text)?;
        if !(1..=MAX_CODE_DIGITS).contains(&file.min_digits) {
            return Err(ConfigError::MinDigits(file.min_digits));
        }
        if file.leader.timeout_ms == 0
            || file.leader.timeout_ms > duration_millis(MAX_LEADER_TIMEOUT)
        {
            return Err(ConfigError::LeaderTimeout(file.leader.timeout_ms));
        }
        if let Some((code, _)) = file.aliases.iter().find(|(_, text)| text.is_empty()) {
            return Err(ConfigError::EmptyAlias(code.clone()));
        }
        Ok(Self {
            mode: file.mode,
            table: file.table,
            trigger: file.trigger,
            min_digits: file.min_digits,
            leader: LeaderConfig {
                chord: file.leader.chord,
                timeout: Duration::from_millis(file.leader.timeout_ms),
            },
            aliases: file.aliases,
        })
    }

    #[must_use]
    pub fn warnings(&self) -> Vec<ConfigWarning> {
        let mut warnings = Vec::new();
        match self.mode {
            Mode::Hold => {
                if self.trigger == Trigger::RightAlt {
                    warnings.push(ConfigWarning::TriggerIsAltGr);
                }
                if self.min_digits == 1 {
                    warnings.push(ConfigWarning::SingleDigitShortcutsCaptured);
                }
            }
            Mode::Leader => {
                let modifiers = self.leader.chord.modifiers;
                if modifiers.contains(Modifier::Ctrl) && modifiers.contains(Modifier::Alt) {
                    warnings.push(ConfigWarning::ChordMatchesAltGr);
                }
            }
        }
        warnings
    }

    #[must_use]
    pub fn resolver(&self) -> Resolver {
        Resolver::new(self.table, self.aliases.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Config {
        Config::from_toml(text).unwrap()
    }

    fn parse_error(text: &str) -> String {
        Config::from_toml(text).unwrap_err().to_string()
    }

    #[test]
    fn empty_file_yields_defaults() {
        assert_eq!(parse(""), Config::default());
    }

    #[test]
    fn parses_full_config() {
        let config = parse(
            r#"
            mode = "leader"
            table = "unicode"
            trigger = "right-ctrl"
            min_digits = 3

            [leader]
            chord = "ctrl+alt+f13"
            timeout_ms = 1500

            [aliases]
            "0" = "zero"
            "8594" = "->"
            "#,
        );
        assert_eq!(config.mode, Mode::Leader);
        assert_eq!(config.table, Table::Unicode);
        assert_eq!(config.trigger, Trigger::RightCtrl);
        assert_eq!(config.min_digits, 3);
        assert_eq!(config.leader.chord.to_string(), "ctrl+alt+f13");
        assert_eq!(config.leader.timeout, Duration::from_millis(1500));
        assert_eq!(config.aliases.len(), 2);
        assert_eq!(config.aliases[&"8594".parse().unwrap()], "->");
    }

    #[test]
    fn leader_section_fields_are_optional() {
        let config = parse("[leader]\ntimeout_ms = 500\n");
        assert_eq!(config.leader.chord, Chord::DEFAULT);
        assert_eq!(config.leader.timeout, Duration::from_millis(500));
    }

    #[test]
    fn rejects_unknown_fields() {
        assert!(parse_error("trigger_key = \"left-alt\"").contains("unknown field"));
        assert!(parse_error("[leader]\nkey = \"u\"").contains("unknown field"));
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(parse_error("mode = \"toggle\"").contains("unknown variant"));
        assert!(parse_error("table = \"cp850\"").contains("unknown variant"));
        assert!(parse_error("trigger = \"left-shift\"").contains("cannot be a trigger"));
        assert!(parse_error("min_digits = -1").contains("invalid value"));
        assert!(parse_error("[aliases]\n\"12a\" = \"x\"").contains("other than 0-9"));
        assert!(parse_error("[aliases]\n\"12345678901\" = \"x\"").contains("longer than"));
        assert!(parse_error("[leader]\nchord = \"ctrl+\"").contains("chord is empty"));
    }

    #[test]
    fn validates_ranges() {
        assert_eq!(
            parse_error("min_digits = 0"),
            "min_digits must be between 1 and 10, got 0"
        );
        assert!(parse_error("min_digits = 11").contains("got 11"));
        assert!(parse_error("[leader]\ntimeout_ms = 0").contains("timeout_ms"));
        assert!(parse_error("[leader]\ntimeout_ms = 60001").contains("timeout_ms"));
        assert_eq!(
            parse("[leader]\ntimeout_ms = 60000").leader.timeout,
            MAX_LEADER_TIMEOUT
        );
        assert_eq!(
            parse_error("[aliases]\n\"42\" = \"\""),
            "alias for code 42 is empty"
        );
    }

    #[test]
    fn parses_triggers() {
        for trigger in Trigger::ALL {
            assert_eq!(trigger.to_string().parse(), Ok(trigger));
        }
        assert_eq!("caps-lock".parse(), Ok(Trigger::CapsLock));
        assert!("escape".parse::<Trigger>().is_err());
    }

    #[test]
    fn only_lock_triggers_are_swallowed() {
        assert!(Trigger::LeftAlt.passes_through());
        assert!(Trigger::RightMeta.passes_through());
        assert!(!Trigger::CapsLock.passes_through());
    }

    #[test]
    fn parses_chords() {
        let chord: Chord = "shift+ctrl+u".parse().unwrap();
        assert_eq!(chord, Chord::DEFAULT);
        assert_eq!(chord.to_string(), "ctrl+shift+u");
        let bare: Chord = "f13".parse().unwrap();
        assert_eq!(bare.modifiers, Modifiers::NONE);
        assert_eq!(bare.key, Key::Function(13));
    }

    #[test]
    fn rejects_invalid_chords() {
        assert_eq!("".parse::<Chord>(), Err(InvalidChord::Empty));
        assert_eq!(
            "hyper+u".parse::<Chord>(),
            Err(InvalidChord::UnknownModifier("hyper".to_owned()))
        );
        assert_eq!(
            "ctrl+ctrl+u".parse::<Chord>(),
            Err(InvalidChord::DuplicateModifier("ctrl".to_owned()))
        );
        assert_eq!(
            "ctrl+left-alt".parse::<Chord>(),
            Err(InvalidChord::InvalidKey("left-alt".to_owned()))
        );
        assert_eq!(
            "ctrl+enter".parse::<Chord>(),
            Err(InvalidChord::InvalidKey("enter".to_owned()))
        );
    }

    #[test]
    fn warns_about_altgr_and_shortcuts() {
        assert!(Config::default().warnings().is_empty());
        assert_eq!(
            parse("trigger = \"right-alt\"\nmin_digits = 1").warnings(),
            [
                ConfigWarning::TriggerIsAltGr,
                ConfigWarning::SingleDigitShortcutsCaptured
            ]
        );
        assert_eq!(
            parse("mode = \"leader\"\n[leader]\nchord = \"ctrl+alt+u\"").warnings(),
            [ConfigWarning::ChordMatchesAltGr]
        );
        assert!(
            parse("mode = \"leader\"\ntrigger = \"right-alt\"")
                .warnings()
                .is_empty()
        );
    }

    #[test]
    fn builds_resolver_from_config() {
        let config = parse("table = \"unicode\"\n[aliases]\n\"1\" = \"one\"");
        let resolver = config.resolver();
        assert_eq!(resolver.table(), Table::Unicode);
        assert!(resolver.resolve(&"1".parse().unwrap()).is_some());
    }
}
