use std::time::{Duration, Instant};

use super::{Context, DigitBuffer, Response};
use crate::config::Chord;
use crate::key::{Key, KeyEvent, Modifiers};

pub(super) struct Leader {
    chord: Chord,
    timeout: Duration,
    state: State,
}

enum State {
    Idle,
    Capturing {
        digits: DigitBuffer,
        last_input: Instant,
    },
}

impl Leader {
    pub(super) fn new(chord: Chord, timeout: Duration) -> Self {
        Self {
            chord,
            timeout,
            state: State::Idle,
        }
    }

    pub(super) fn handle(
        &mut self,
        event: KeyEvent,
        now: Instant,
        modifiers: Modifiers,
        mut context: Context<'_>,
    ) -> Response {
        if let State::Capturing { last_input, .. } = self.state
            && now.saturating_duration_since(last_input) >= self.timeout
        {
            self.state = State::Idle;
        }
        if !event.is_press() {
            return context.release_swallowed(event.key);
        }
        if context.swallowed.contains(event.key) {
            return Response::swallow();
        }
        let key = event.key;
        match &mut self.state {
            State::Idle => {
                if !context.repeat && key == self.chord.key && modifiers == self.chord.modifiers {
                    context.swallowed.insert(key);
                    self.state = State::Capturing {
                        digits: DigitBuffer::default(),
                        last_input: now,
                    };
                    Response::swallow()
                } else {
                    Response::Pass
                }
            }
            State::Capturing { digits, last_input } => match key {
                _ if key.is_modifier() => Response::Pass,
                _ if context.repeat => {
                    self.state = State::Idle;
                    Response::Pass
                }
                Key::Digit(digit) => {
                    digits.push(digit);
                    *last_input = now;
                    context.swallowed.insert(key);
                    Response::swallow()
                }
                Key::Enter | Key::Space => {
                    let outputs = context.resolve(digits).into_iter().collect();
                    context.swallowed.insert(key);
                    self.state = State::Idle;
                    Response::Replace(outputs)
                }
                Key::Escape => {
                    context.swallowed.insert(key);
                    self.state = State::Idle;
                    Response::swallow()
                }
                _ => {
                    self.state = State::Idle;
                    Response::Pass
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::config::{Chord, Config, LeaderConfig, Mode};
    use crate::engine::{Engine, Output, Response};
    use crate::key::{Digit, Key, KeyEvent};

    const TIMEOUT: Duration = Duration::from_millis(1000);
    const JUST_UNDER_TIMEOUT: Duration = Duration::from_millis(999);

    struct Harness {
        engine: Engine,
        now: Instant,
    }

    impl Harness {
        fn new(chord: &str) -> Self {
            let config = Config {
                mode: Mode::Leader,
                leader: LeaderConfig {
                    chord: chord.parse::<Chord>().unwrap(),
                    timeout: TIMEOUT,
                },
                ..Config::default()
            };
            Self {
                engine: Engine::new(&config),
                now: Instant::now(),
            }
        }

        fn wait(&mut self, duration: Duration) {
            self.now += duration;
        }

        fn press(&mut self, key: Key) -> Response {
            self.engine.handle(KeyEvent::press(key), self.now)
        }

        fn release(&mut self, key: Key) -> Response {
            self.engine.handle(KeyEvent::release(key), self.now)
        }

        fn tap(&mut self, key: Key) -> Response {
            let response = self.press(key);
            assert_eq!(self.release(key), Response::swallow());
            response
        }

        fn chord(&mut self) {
            assert_eq!(self.press(Key::LeftCtrl), Response::Pass);
            assert_eq!(self.press(Key::RightShift), Response::Pass);
            assert_eq!(self.press(Key::Letter(b'u')), Response::swallow());
            assert_eq!(self.release(Key::RightShift), Response::Pass);
            assert_eq!(self.release(Key::LeftCtrl), Response::Pass);
            assert_eq!(self.release(Key::Letter(b'u')), Response::swallow());
        }

        fn digits(&mut self, code: &str) {
            for c in code.chars() {
                let key = Key::Digit(Digit::from_char(c).unwrap());
                assert_eq!(self.tap(key), Response::swallow());
            }
        }
    }

    fn text(value: &str) -> Response {
        Response::Replace(vec![Output::Text(value.to_owned())])
    }

    #[test]
    fn enter_commits_buffered_code() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        harness.digits("0169");
        assert_eq!(harness.tap(Key::Enter), text("\u{00A9}"));
        assert_eq!(
            harness.press(Key::Digit(Digit::new(1).unwrap())),
            Response::Pass
        );
    }

    #[test]
    fn space_commits_buffered_code() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        harness.digits("65");
        assert_eq!(harness.tap(Key::Space), text("A"));
    }

    #[test]
    fn escape_cancels_capture() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        harness.digits("65");
        assert_eq!(harness.tap(Key::Escape), Response::swallow());
        assert_eq!(harness.press(Key::Enter), Response::Pass);
    }

    #[test]
    fn empty_or_invalid_commit_types_nothing() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        assert_eq!(harness.tap(Key::Enter), Response::swallow());
        harness.chord();
        harness.digits("0");
        assert_eq!(harness.tap(Key::Enter), Response::swallow());
    }

    #[test]
    fn chord_requires_exact_modifiers() {
        let mut harness = Harness::new("ctrl+shift+u");
        assert_eq!(harness.press(Key::Letter(b'u')), Response::Pass);
        assert_eq!(harness.press(Key::LeftCtrl), Response::Pass);
        assert_eq!(harness.press(Key::Letter(b'u')), Response::Pass);
        assert_eq!(harness.press(Key::LeftShift), Response::Pass);
        assert_eq!(harness.press(Key::LeftAlt), Response::Pass);
        assert_eq!(harness.press(Key::Letter(b'u')), Response::Pass);
    }

    #[test]
    fn chord_without_modifiers() {
        let mut harness = Harness::new("f13");
        assert_eq!(harness.tap(Key::Function(13)), Response::swallow());
        harness.digits("66");
        assert_eq!(harness.tap(Key::Enter), text("B"));
    }

    #[test]
    fn timeout_cancels_capture() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        harness.digits("6");
        harness.wait(TIMEOUT);
        assert_eq!(
            harness.press(Key::Digit(Digit::new(5).unwrap())),
            Response::Pass
        );
        assert_eq!(harness.press(Key::Enter), Response::Pass);
    }

    #[test]
    fn timeout_restarts_with_each_digit() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        harness.wait(JUST_UNDER_TIMEOUT);
        harness.digits("6");
        harness.wait(JUST_UNDER_TIMEOUT);
        harness.digits("5");
        harness.wait(JUST_UNDER_TIMEOUT);
        assert_eq!(harness.tap(Key::Enter), text("A"));
    }

    #[test]
    fn other_keys_cancel_and_pass_through() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        harness.digits("65");
        assert_eq!(harness.press(Key::Letter(b'k')), Response::Pass);
        assert_eq!(harness.press(Key::Enter), Response::Pass);
    }

    #[test]
    fn modifiers_do_not_cancel_capture() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        harness.digits("6");
        assert_eq!(harness.press(Key::LeftShift), Response::Pass);
        assert_eq!(harness.release(Key::LeftShift), Response::Pass);
        harness.digits("5");
        assert_eq!(harness.tap(Key::Enter), text("A"));
    }

    #[test]
    fn autorepeat_never_starts_or_feeds_a_capture() {
        let mut harness = Harness::new("ctrl+shift+u");
        assert_eq!(harness.press(Key::Letter(b'u')), Response::Pass);
        assert_eq!(harness.press(Key::LeftCtrl), Response::Pass);
        assert_eq!(harness.press(Key::LeftShift), Response::Pass);
        assert_eq!(harness.press(Key::Letter(b'u')), Response::Pass);
        assert_eq!(harness.release(Key::Letter(b'u')), Response::Pass);
        assert_eq!(harness.press(Key::Enter), Response::Pass);
        assert_eq!(harness.press(Key::Letter(b'u')), Response::swallow());
        assert_eq!(harness.press(Key::Enter), Response::Pass);
    }

    #[test]
    fn swallowed_keys_held_after_commit_stay_swallowed() {
        let mut harness = Harness::new("ctrl+shift+u");
        harness.chord();
        harness.digits("65");
        assert_eq!(harness.press(Key::Enter), text("A"));
        assert_eq!(harness.press(Key::Enter), Response::swallow());
        assert_eq!(harness.release(Key::Enter), Response::swallow());
        assert_eq!(harness.press(Key::Enter), Response::Pass);
    }
}
