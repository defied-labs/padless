use std::mem;

use super::{Context, DigitBuffer, Output, Response};
use crate::config::Trigger;
use crate::key::{Key, KeyAction, KeyEvent};

pub(super) struct Hold {
    trigger: Trigger,
    min_digits: usize,
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Armed(Episode),
    Bypassed,
}

#[derive(Default)]
struct Episode {
    held: Vec<Held>,
    digits: DigitBuffer,
}

#[derive(Clone, Copy)]
struct Held {
    event: KeyEvent,
    delivered_before: bool,
}

impl Episode {
    fn hold(&mut self, event: KeyEvent) {
        self.held.push(Held {
            event,
            delivered_before: false,
        });
    }

    fn hold_release_of_delivered(&mut self, event: KeyEvent) {
        self.held.push(Held {
            event,
            delivered_before: true,
        });
    }

    fn holds_press_of(&self, key: Key) -> bool {
        self.held
            .iter()
            .any(|held| !held.delivered_before && held.event == KeyEvent::press(key))
    }

    fn releases_of_delivered(&self) -> impl Iterator<Item = KeyEvent> + '_ {
        self.held
            .iter()
            .filter(|held| held.delivered_before)
            .map(|held| held.event)
    }
}

impl Hold {
    pub(super) fn new(trigger: Trigger, min_digits: usize) -> Self {
        Self {
            trigger,
            min_digits,
            state: State::Idle,
        }
    }

    pub(super) fn handle(&mut self, event: KeyEvent, context: Context<'_>) -> Response {
        match event.action {
            KeyAction::Press => self.press(event, context),
            KeyAction::Release => self.release(event, context),
        }
    }

    fn press(&mut self, event: KeyEvent, mut context: Context<'_>) -> Response {
        let is_trigger = event.key == self.trigger.key();
        match mem::take(&mut self.state) {
            State::Idle if is_trigger => {
                let mut episode = Episode::default();
                let response = if self.trigger.passes_through() {
                    Response::Pass
                } else {
                    episode.hold(event);
                    Response::swallow()
                };
                self.state = State::Armed(episode);
                response
            }
            State::Armed(episode) if is_trigger => {
                self.state = State::Armed(episode);
                self.trigger_repeat()
            }
            State::Armed(mut episode) => {
                if context.swallowed.contains(event.key) {
                    episode.hold(event);
                } else if let Some(digit) = event.key.digit()
                    && !context.repeat
                {
                    episode.digits.push(digit);
                    episode.hold(event);
                    context.swallowed.insert(event.key);
                } else {
                    self.state = State::Bypassed;
                    return replay(episode, event, &mut context);
                }
                self.state = State::Armed(episode);
                Response::swallow()
            }
            state => {
                self.state = state;
                if context.swallowed.contains(event.key) {
                    Response::swallow()
                } else {
                    Response::Pass
                }
            }
        }
    }

    fn release(&mut self, event: KeyEvent, mut context: Context<'_>) -> Response {
        let is_trigger = event.key == self.trigger.key();
        match mem::take(&mut self.state) {
            State::Armed(episode) if is_trigger => self.finish(episode, event, &mut context),
            State::Bypassed if is_trigger => Response::Pass,
            State::Armed(mut episode) => {
                let response = if context.swallowed.remove(event.key) {
                    if episode.holds_press_of(event.key) {
                        episode.hold(event);
                    }
                    Response::swallow()
                } else if episode.held.is_empty() {
                    Response::Pass
                } else {
                    episode.hold_release_of_delivered(event);
                    Response::swallow()
                };
                self.state = State::Armed(episode);
                response
            }
            state => {
                self.state = state;
                context.release_swallowed(event.key)
            }
        }
    }

    fn trigger_repeat(&self) -> Response {
        if self.trigger.passes_through() {
            Response::Pass
        } else {
            Response::swallow()
        }
    }

    fn finish(&self, episode: Episode, release: KeyEvent, context: &mut Context<'_>) -> Response {
        if episode.digits.count() < self.min_digits {
            return replay(episode, release, context);
        }
        let mut outputs: Vec<Output> = episode.releases_of_delivered().map(Output::Key).collect();
        if self.trigger.passes_through() {
            outputs.push(Output::Mask);
            outputs.push(Output::Key(release));
        }
        outputs.extend(context.resolve(&episode.digits));
        Response::Replace(outputs)
    }
}

fn replay(episode: Episode, current: KeyEvent, context: &mut Context<'_>) -> Response {
    for held in &episode.held {
        if !held.delivered_before {
            context.swallowed.remove(held.event.key);
        }
    }
    let outputs = episode
        .held
        .into_iter()
        .map(|held| held.event)
        .chain([current])
        .map(Output::Key)
        .collect();
    Response::replacing(current, outputs)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::config::{Config, Trigger};
    use crate::engine::{Engine, Output, Response};
    use crate::key::{Digit, Key, KeyEvent};

    fn digit(value: u8) -> Key {
        Key::Digit(Digit::new(value).unwrap())
    }

    fn press(key: Key) -> KeyEvent {
        KeyEvent::press(key)
    }

    fn release(key: Key) -> KeyEvent {
        KeyEvent::release(key)
    }

    fn engine(trigger: Trigger, min_digits: usize) -> Engine {
        Engine::new(&Config {
            trigger,
            min_digits,
            ..Config::default()
        })
    }

    fn feed(engine: &mut Engine, events: &[KeyEvent]) -> Vec<Response> {
        let now = Instant::now();
        events
            .iter()
            .map(|event| engine.handle(*event, now))
            .collect()
    }

    fn type_code(trigger: Key, code: &str) -> Vec<KeyEvent> {
        let mut events = vec![press(trigger)];
        for c in code.chars() {
            let key = Key::Digit(Digit::from_char(c).unwrap());
            events.push(press(key));
            events.push(release(key));
        }
        events.push(release(trigger));
        events
    }

    fn keys(events: &[KeyEvent]) -> Vec<Output> {
        events.iter().copied().map(Output::Key).collect()
    }

    fn text(value: &str) -> Output {
        Output::Text(value.to_owned())
    }

    #[test]
    fn keys_pass_through_while_idle() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(
            &mut engine,
            &[press(digit(1)), release(digit(1)), press(Key::Letter(b'a'))],
        );
        assert!(responses.iter().all(|response| *response == Response::Pass));
    }

    #[test]
    fn commits_code_on_trigger_release() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(&mut engine, &type_code(Key::LeftAlt, "0233"));
        assert_eq!(responses[0], Response::Pass);
        assert!(
            responses[1..9]
                .iter()
                .all(|response| *response == Response::swallow())
        );
        assert_eq!(
            responses[9],
            Response::Replace(vec![
                Output::Mask,
                Output::Key(release(Key::LeftAlt)),
                text("\u{00E9}"),
            ])
        );
    }

    #[test]
    fn replays_short_codes_as_shortcuts() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(&mut engine, &type_code(Key::LeftAlt, "1"));
        assert_eq!(
            responses,
            [
                Response::Pass,
                Response::swallow(),
                Response::swallow(),
                Response::Replace(keys(&[
                    press(digit(1)),
                    release(digit(1)),
                    release(Key::LeftAlt)
                ])),
            ]
        );
    }

    #[test]
    fn lone_trigger_tap_passes_through() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(&mut engine, &[press(Key::LeftAlt), release(Key::LeftAlt)]);
        assert_eq!(responses, [Response::Pass, Response::Pass]);
    }

    #[test]
    fn non_digit_cancels_and_replays_in_order() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(
            &mut engine,
            &[
                press(Key::LeftAlt),
                press(digit(4)),
                release(digit(4)),
                press(digit(2)),
                press(Key::Letter(b'x')),
                release(digit(2)),
                press(digit(3)),
                release(Key::LeftAlt),
            ],
        );
        assert_eq!(
            responses[4],
            Response::Replace(keys(&[
                press(digit(4)),
                release(digit(4)),
                press(digit(2)),
                press(Key::Letter(b'x')),
            ]))
        );
        assert_eq!(
            responses[5..],
            [Response::Pass, Response::Pass, Response::Pass]
        );
    }

    #[test]
    fn trigger_autorepeat_passes_through() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(
            &mut engine,
            &[press(Key::LeftAlt), press(Key::LeftAlt), press(digit(6))],
        );
        assert_eq!(responses[1], Response::Pass);
        assert_eq!(responses[2], Response::swallow());
    }

    #[test]
    fn digit_autorepeat_is_swallowed_but_not_counted() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(
            &mut engine,
            &[
                press(Key::LeftAlt),
                press(digit(6)),
                press(digit(6)),
                press(digit(6)),
                release(digit(6)),
                release(Key::LeftAlt),
            ],
        );
        assert_eq!(responses[2], Response::swallow());
        assert_eq!(
            responses[5],
            Response::Replace(keys(&[
                press(digit(6)),
                press(digit(6)),
                press(digit(6)),
                release(digit(6)),
                release(Key::LeftAlt)
            ]))
        );
    }

    #[test]
    fn digits_held_past_commit_have_their_release_swallowed() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(
            &mut engine,
            &[
                press(Key::LeftAlt),
                press(digit(6)),
                release(digit(6)),
                press(digit(5)),
                release(Key::LeftAlt),
                press(digit(5)),
                release(digit(5)),
                press(digit(5)),
            ],
        );
        assert_eq!(
            responses[4],
            Response::Replace(vec![
                Output::Mask,
                Output::Key(release(Key::LeftAlt)),
                text("A")
            ])
        );
        assert_eq!(responses[5], Response::swallow());
        assert_eq!(responses[6], Response::swallow());
        assert_eq!(responses[7], Response::Pass);
    }

    #[test]
    fn autorepeat_of_digit_held_before_trigger_cancels() {
        let mut engine = engine(Trigger::LeftAlt, 1);
        let responses = feed(
            &mut engine,
            &[
                press(digit(8)),
                press(Key::LeftAlt),
                press(digit(8)),
                release(Key::LeftAlt),
                release(digit(8)),
            ],
        );
        assert!(responses.iter().all(|response| *response == Response::Pass));
    }

    #[test]
    fn releases_of_delivered_keys_keep_their_order() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(
            &mut engine,
            &[
                press(Key::LeftShift),
                press(Key::LeftAlt),
                press(digit(0)),
                release(Key::LeftShift),
                press(Key::Letter(b'x')),
            ],
        );
        assert_eq!(responses[3], Response::swallow());
        assert_eq!(
            responses[4],
            Response::Replace(keys(&[
                press(digit(0)),
                release(Key::LeftShift),
                press(Key::Letter(b'x')),
            ]))
        );
    }

    #[test]
    fn releases_of_delivered_keys_are_flushed_before_commit() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(
            &mut engine,
            &[
                press(Key::LeftShift),
                press(Key::LeftAlt),
                press(digit(6)),
                release(Key::LeftShift),
                release(digit(6)),
                press(digit(5)),
                release(digit(5)),
                release(Key::LeftAlt),
            ],
        );
        assert_eq!(
            responses[7],
            Response::Replace(vec![
                Output::Key(release(Key::LeftShift)),
                Output::Mask,
                Output::Key(release(Key::LeftAlt)),
                text("A"),
            ])
        );
    }

    #[test]
    fn digits_held_before_trigger_are_not_captured_on_release() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(
            &mut engine,
            &[press(digit(7)), press(Key::LeftAlt), release(digit(7))],
        );
        assert_eq!(responses, [Response::Pass, Response::Pass, Response::Pass]);
    }

    #[test]
    fn replay_keeps_digits_still_held_pressed() {
        let mut engine = engine(Trigger::LeftAlt, 3);
        let responses = feed(
            &mut engine,
            &[
                press(Key::LeftAlt),
                press(digit(1)),
                release(Key::LeftAlt),
                release(digit(1)),
            ],
        );
        assert_eq!(
            responses[2],
            Response::Replace(keys(&[press(digit(1)), release(Key::LeftAlt)]))
        );
        assert_eq!(responses[3], Response::Pass);
    }

    #[test]
    fn unresolvable_code_still_consumes_digits() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(&mut engine, &type_code(Key::LeftAlt, "256"));
        assert_eq!(
            responses.last(),
            Some(&Response::Replace(vec![
                Output::Mask,
                Output::Key(release(Key::LeftAlt))
            ]))
        );
    }

    #[test]
    fn overlong_code_resolves_to_nothing() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let responses = feed(&mut engine, &type_code(Key::LeftAlt, "00000000065"));
        assert_eq!(
            responses.last(),
            Some(&Response::Replace(vec![
                Output::Mask,
                Output::Key(release(Key::LeftAlt))
            ]))
        );
    }

    #[test]
    fn lock_trigger_is_swallowed_and_commits_without_release() {
        let mut engine = engine(Trigger::CapsLock, 2);
        let responses = feed(&mut engine, &type_code(Key::CapsLock, "65"));
        assert_eq!(responses[0], Response::swallow());
        assert_eq!(responses.last(), Some(&Response::Replace(vec![text("A")])));
    }

    #[test]
    fn lock_trigger_tap_is_replayed() {
        let mut engine = engine(Trigger::CapsLock, 2);
        let responses = feed(&mut engine, &[press(Key::CapsLock), release(Key::CapsLock)]);
        assert_eq!(
            responses,
            [
                Response::swallow(),
                Response::Replace(keys(&[press(Key::CapsLock), release(Key::CapsLock)])),
            ]
        );
    }

    #[test]
    fn lock_trigger_cancel_replays_trigger_press() {
        let mut engine = engine(Trigger::CapsLock, 2);
        let responses = feed(
            &mut engine,
            &[
                press(Key::CapsLock),
                press(Key::CapsLock),
                press(Key::Letter(b'q')),
                release(Key::CapsLock),
            ],
        );
        assert_eq!(responses[1], Response::swallow());
        assert_eq!(
            responses[2],
            Response::Replace(keys(&[press(Key::CapsLock), press(Key::Letter(b'q'))]))
        );
        assert_eq!(responses[3], Response::Pass);
    }

    #[test]
    fn aliases_override_table() {
        let mut config = Config::default();
        config
            .aliases
            .insert("42".parse().unwrap(), "forty-two".to_owned());
        let mut engine = Engine::new(&config);
        let responses = feed(&mut engine, &type_code(Key::LeftAlt, "42"));
        assert_eq!(
            responses.last(),
            Some(&Response::Replace(vec![
                Output::Mask,
                Output::Key(release(Key::LeftAlt)),
                text("forty-two"),
            ]))
        );
    }

    #[test]
    fn other_trigger_side_does_not_arm() {
        let mut engine = engine(Trigger::RightCtrl, 2);
        let responses = feed(&mut engine, &type_code(Key::LeftCtrl, "65"));
        assert!(responses.iter().all(|response| *response == Response::Pass));
    }

    #[test]
    fn episodes_are_independent() {
        let mut engine = engine(Trigger::LeftAlt, 2);
        let mut events = type_code(Key::LeftAlt, "65");
        events.extend(type_code(Key::LeftAlt, "1"));
        events.extend(type_code(Key::LeftAlt, "66"));
        let responses = feed(&mut engine, &events);
        assert_eq!(
            responses[5],
            Response::Replace(vec![
                Output::Mask,
                Output::Key(release(Key::LeftAlt)),
                text("A")
            ])
        );
        assert_eq!(
            responses[9],
            Response::Replace(keys(&[
                press(digit(1)),
                release(digit(1)),
                release(Key::LeftAlt)
            ]))
        );
        assert_eq!(
            responses[15],
            Response::Replace(vec![
                Output::Mask,
                Output::Key(release(Key::LeftAlt)),
                text("B")
            ])
        );
    }
}
