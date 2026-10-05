mod hold;
mod leader;

use std::time::Instant;

use crate::code::{Code, MAX_CODE_DIGITS};
use crate::config::{Config, Mode};
use crate::key::{Digit, Key, KeyAction, KeyEvent, Modifiers};
use crate::resolver::Resolver;

use hold::Hold;
use leader::Leader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Key(KeyEvent),
    Mask,
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Pass,
    Replace(Vec<Output>),
}

impl Response {
    #[must_use]
    pub const fn swallow() -> Self {
        Self::Replace(Vec::new())
    }

    fn replacing(event: KeyEvent, outputs: Vec<Output>) -> Self {
        if let [Output::Key(only)] = outputs.as_slice()
            && *only == event
        {
            Self::Pass
        } else {
            Self::Replace(outputs)
        }
    }
}

pub struct Engine {
    machine: Machine,
    resolver: Resolver,
    swallowed: KeySet,
    held_modifiers: KeySet,
}

enum Machine {
    Hold(Hold),
    Leader(Leader),
}

impl Engine {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let machine = match config.mode {
            Mode::Hold => Machine::Hold(Hold::new(config.trigger, config.min_digits)),
            Mode::Leader => {
                Machine::Leader(Leader::new(config.leader.chord, config.leader.timeout))
            }
        };
        Self {
            machine,
            resolver: config.resolver(),
            swallowed: KeySet::default(),
            held_modifiers: KeySet::default(),
        }
    }

    pub fn handle(&mut self, event: KeyEvent, now: Instant) -> Response {
        if event.key.is_modifier() {
            match event.action {
                KeyAction::Press => self.held_modifiers.insert(event.key),
                KeyAction::Release => {
                    self.held_modifiers.remove(event.key);
                }
            }
        }
        let modifiers: Modifiers = self
            .held_modifiers
            .iter()
            .filter_map(Key::modifier)
            .collect();
        let context = Context {
            swallowed: &mut self.swallowed,
            resolver: &self.resolver,
        };
        match &mut self.machine {
            Machine::Hold(hold) => hold.handle(event, context),
            Machine::Leader(leader) => leader.handle(event, now, modifiers, context),
        }
    }
}

struct Context<'a> {
    swallowed: &'a mut KeySet,
    resolver: &'a Resolver,
}

impl Context<'_> {
    fn resolve(&self, digits: &DigitBuffer) -> Option<Output> {
        let code = digits.code()?;
        let resolution = self.resolver.resolve(&code)?;
        Some(Output::Text(resolution.to_text()))
    }

    fn release_swallowed(&mut self, key: Key) -> Response {
        if self.swallowed.remove(key) {
            Response::swallow()
        } else {
            Response::Pass
        }
    }
}

#[derive(Default)]
struct KeySet(Vec<Key>);

impl KeySet {
    fn contains(&self, key: Key) -> bool {
        self.0.contains(&key)
    }

    fn insert(&mut self, key: Key) {
        if !self.contains(key) {
            self.0.push(key);
        }
    }

    fn remove(&mut self, key: Key) -> bool {
        let before = self.0.len();
        self.0.retain(|held| *held != key);
        self.0.len() != before
    }

    fn iter(&self) -> impl Iterator<Item = Key> + '_ {
        self.0.iter().copied()
    }
}

#[derive(Default)]
struct DigitBuffer {
    digits: Vec<Digit>,
    count: usize,
}

impl DigitBuffer {
    fn push(&mut self, digit: Digit) {
        self.count = self.count.saturating_add(1);
        if self.digits.len() < MAX_CODE_DIGITS {
            self.digits.push(digit);
        }
    }

    fn count(&self) -> usize {
        self.count
    }

    fn code(&self) -> Option<Code> {
        if self.count > MAX_CODE_DIGITS {
            return None;
        }
        Code::from_digits(&self.digits).ok()
    }
}
