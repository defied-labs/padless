use std::collections::HashSet;
use std::time::{Duration, Instant};

use padless_core::{
    Chord, Code, Config, Digit, Engine, Key, KeyAction, KeyEvent, LeaderConfig, Mode, Modifier,
    Modifiers, Output, Resolution, Response, Table, Trigger,
};
use proptest::prelude::*;

const ALPHABET: [Key; 22] = [
    Key::LeftAlt,
    Key::RightAlt,
    Key::LeftCtrl,
    Key::RightCtrl,
    Key::LeftShift,
    Key::LeftMeta,
    Key::CapsLock,
    Key::Enter,
    Key::Space,
    Key::Escape,
    Key::Letter(b'u'),
    Key::Letter(b'x'),
    Key::Function(13),
    Key::Other(0x5d),
    digit(0),
    digit(1),
    digit(2),
    digit(3),
    digit(5),
    digit(6),
    digit(8),
    digit(9),
];

const fn digit(value: u8) -> Key {
    match Digit::new(value) {
        Some(digit) => Key::Digit(digit),
        None => Key::Other(0),
    }
}

#[derive(Debug, Clone, Copy)]
struct Step {
    key: Key,
    repeat: bool,
    delay_ms: u64,
}

fn step() -> impl Strategy<Value = Step> {
    (0..ALPHABET.len(), any::<bool>(), 0u64..1500).prop_map(|(index, repeat, delay_ms)| Step {
        key: ALPHABET[index],
        repeat,
        delay_ms,
    })
}

fn physical_events(steps: &[Step]) -> Vec<(KeyEvent, Duration)> {
    let mut down: Vec<Key> = Vec::new();
    let mut events = Vec::new();
    for step in steps {
        let delay = Duration::from_millis(step.delay_ms);
        if let Some(position) = down.iter().position(|key| *key == step.key) {
            if step.repeat {
                events.push((KeyEvent::press(step.key), delay));
            } else {
                down.remove(position);
                events.push((KeyEvent::release(step.key), delay));
            }
        } else {
            down.push(step.key);
            events.push((KeyEvent::press(step.key), delay));
        }
    }
    for key in down.into_iter().rev() {
        events.push((KeyEvent::release(key), Duration::ZERO));
    }
    events
}

struct Run {
    input: Vec<KeyEvent>,
    responses: Vec<Response>,
}

impl Run {
    fn new(config: &Config, events: &[(KeyEvent, Duration)]) -> Self {
        let mut engine = Engine::new(config);
        let mut now = Instant::now();
        let mut responses = Vec::new();
        for (event, delay) in events {
            now += *delay;
            responses.push(engine.handle(*event, now));
        }
        Self {
            input: events.iter().map(|(event, _)| *event).collect(),
            responses,
        }
    }

    fn delivered_keys(&self) -> Vec<KeyEvent> {
        self.input
            .iter()
            .zip(&self.responses)
            .flat_map(|(event, response)| match response {
                Response::Pass => vec![*event],
                Response::Replace(outputs) => outputs
                    .iter()
                    .filter_map(|output| match output {
                        Output::Key(key_event) => Some(*key_event),
                        Output::Mask | Output::Text(_) => None,
                    })
                    .collect(),
            })
            .collect()
    }

    fn committed(&self) -> bool {
        self.responses.iter().any(|response| match response {
            Response::Pass => false,
            Response::Replace(outputs) => outputs
                .iter()
                .any(|output| matches!(output, Output::Mask | Output::Text(_))),
        })
    }

    fn texts(&self) -> Vec<String> {
        self.responses
            .iter()
            .flat_map(|response| match response {
                Response::Pass => Vec::new(),
                Response::Replace(outputs) => outputs
                    .iter()
                    .filter_map(|output| match output {
                        Output::Text(text) => Some(text.clone()),
                        Output::Key(_) | Output::Mask => None,
                    })
                    .collect(),
            })
            .collect()
    }
}

fn hold_config(trigger: Trigger, min_digits: usize, table: Table) -> Config {
    Config {
        mode: Mode::Hold,
        trigger,
        min_digits,
        table,
        ..Config::default()
    }
}

fn leader_config(chord: Chord) -> Config {
    Config {
        mode: Mode::Leader,
        leader: LeaderConfig {
            chord,
            timeout: Duration::from_millis(1000),
        },
        ..Config::default()
    }
}

const F13: Chord = Chord {
    modifiers: Modifiers::NONE,
    key: Key::Function(13),
};

fn chord(modifiers: &[Modifier], key: Key) -> Chord {
    Chord {
        modifiers: modifiers.iter().copied().collect(),
        key,
    }
}

fn any_config() -> impl Strategy<Value = Config> {
    let hold = (
        prop::sample::select(Trigger::ALL.to_vec()),
        1usize..=4,
        prop::sample::select(vec![Table::Windows, Table::Unicode]),
    )
        .prop_map(|(trigger, min_digits, table)| hold_config(trigger, min_digits, table));
    let leader = prop::sample::select(vec![
        chord(&[Modifier::Ctrl], Key::Letter(b'u')),
        chord(&[Modifier::Ctrl, Modifier::Shift], Key::Letter(b'u')),
        chord(&[Modifier::Alt], digit(5)),
        chord(&[], Key::Letter(b'x')),
        F13,
    ])
    .prop_map(leader_config);
    prop_oneof![hold, leader]
}

fn assert_balanced(delivered: &[KeyEvent]) -> Result<(), TestCaseError> {
    let mut down = HashSet::new();
    for event in delivered {
        match event.action {
            KeyAction::Press => {
                down.insert(event.key);
            }
            KeyAction::Release => {
                prop_assert!(
                    down.remove(&event.key),
                    "release of {} delivered without a press",
                    event.key
                );
            }
        }
    }
    prop_assert!(down.is_empty(), "keys left pressed downstream: {down:?}");
    Ok(())
}

fn without_trigger_repeats(events: &[KeyEvent], trigger: Key) -> Vec<KeyEvent> {
    let mut trigger_down = false;
    events
        .iter()
        .copied()
        .filter(|event| {
            if event.key != trigger {
                return true;
            }
            let repeat = event.is_press() && trigger_down;
            trigger_down = event.is_press();
            !repeat
        })
        .collect()
}

fn tap_code(trigger: Key, code: &Code) -> Vec<(KeyEvent, Duration)> {
    let mut events = vec![(KeyEvent::press(trigger), Duration::ZERO)];
    for digit in code.digits() {
        let key = Key::Digit(*digit);
        events.push((KeyEvent::press(key), Duration::ZERO));
        events.push((KeyEvent::release(key), Duration::ZERO));
    }
    events.push((KeyEvent::release(trigger), Duration::ZERO));
    events
}

fn code_strategy() -> impl Strategy<Value = Code> {
    "[0-9]{2,7}".prop_filter_map("decimal code", |text| text.parse().ok())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn delivered_stream_never_leaves_keys_stuck(
        config in any_config(),
        steps in prop::collection::vec(step(), 0..60),
    ) {
        let run = Run::new(&config, &physical_events(&steps));
        assert_balanced(&run.delivered_keys())?;
    }

    #[test]
    fn engine_survives_malformed_input(
        config in any_config(),
        events in prop::collection::vec(
            (0..ALPHABET.len(), any::<bool>(), 0u64..5000),
            0..80,
        ),
    ) {
        let events: Vec<_> = events
            .into_iter()
            .map(|(index, press, delay)| {
                let key = ALPHABET[index];
                let event = if press { KeyEvent::press(key) } else { KeyEvent::release(key) };
                (event, Duration::from_millis(delay))
            })
            .collect();
        let run = Run::new(&config, &events);
        prop_assert_eq!(run.responses.len(), events.len());
    }

    #[test]
    fn hold_mode_is_transparent_without_trigger(
        trigger in prop::sample::select(Trigger::ALL.to_vec()),
        min_digits in 1usize..=4,
        steps in prop::collection::vec(step(), 0..60),
    ) {
        let steps: Vec<_> = steps.into_iter().filter(|step| step.key != trigger.key()).collect();
        let run = Run::new(&hold_config(trigger, min_digits, Table::Windows), &physical_events(&steps));
        prop_assert!(run.responses.iter().all(|response| *response == Response::Pass));
    }

    #[test]
    fn hold_mode_without_commit_preserves_input(
        trigger in prop::sample::select(vec![
            Trigger::LeftAlt,
            Trigger::RightAlt,
            Trigger::LeftCtrl,
            Trigger::RightCtrl,
            Trigger::LeftMeta,
        ]),
        min_digits in 1usize..=4,
        steps in prop::collection::vec(step(), 0..60),
    ) {
        let run = Run::new(&hold_config(trigger, min_digits, Table::Windows), &physical_events(&steps));
        if !run.committed() {
            prop_assert_eq!(
                without_trigger_repeats(&run.delivered_keys(), trigger.key()),
                without_trigger_repeats(&run.input, trigger.key())
            );
        }
    }

    #[test]
    fn hold_mode_short_codes_replay_the_shortcut(
        trigger in prop::sample::select(Trigger::ALL.to_vec()),
        min_digits in 2usize..=5,
        digits in prop::collection::vec(0u8..=9, 0..5),
    ) {
        let digits: Vec<Digit> = digits
            .into_iter()
            .take(min_digits - 1)
            .filter_map(Digit::new)
            .collect();
        let mut events = vec![(KeyEvent::press(trigger.key()), Duration::ZERO)];
        for digit in &digits {
            events.push((KeyEvent::press(Key::Digit(*digit)), Duration::ZERO));
            events.push((KeyEvent::release(Key::Digit(*digit)), Duration::ZERO));
        }
        events.push((KeyEvent::release(trigger.key()), Duration::ZERO));
        let run = Run::new(&hold_config(trigger, min_digits, Table::Windows), &events);
        prop_assert_eq!(run.delivered_keys(), run.input.clone());
        prop_assert!(run.texts().is_empty());
    }

    #[test]
    fn hold_mode_commits_resolved_codes(
        trigger in prop::sample::select(Trigger::ALL.to_vec()),
        table in prop::sample::select(vec![Table::Windows, Table::Unicode]),
        code in code_strategy(),
    ) {
        let config = hold_config(trigger, 2, table);
        let run = Run::new(&config, &tap_code(trigger.key(), &code));
        let expected: Vec<String> = config
            .resolver()
            .resolve(&code)
            .map(Resolution::to_text)
            .into_iter()
            .collect();
        prop_assert_eq!(run.texts(), expected);
        let expected_keys = if trigger.passes_through() {
            vec![KeyEvent::press(trigger.key()), KeyEvent::release(trigger.key())]
        } else {
            Vec::new()
        };
        prop_assert_eq!(run.delivered_keys(), expected_keys);
    }

    #[test]
    fn hold_mode_commit_masks_before_releasing_modifier(
        trigger in prop::sample::select(Trigger::ALL.to_vec()),
        code in code_strategy(),
    ) {
        let run = Run::new(&hold_config(trigger, 2, Table::Windows), &tap_code(trigger.key(), &code));
        let last = run.responses.last().cloned().unwrap();
        let Response::Replace(outputs) = last else {
            return Err(TestCaseError::fail("commit must replace the trigger release"));
        };
        if trigger.passes_through() {
            prop_assert_eq!(&outputs[..2], &[Output::Mask, Output::Key(KeyEvent::release(trigger.key()))]);
        } else {
            prop_assert!(!outputs.contains(&Output::Mask));
        }
    }

    #[test]
    fn leader_mode_commits_resolved_codes(
        table in prop::sample::select(vec![Table::Windows, Table::Unicode]),
        code in code_strategy(),
        commit in prop::sample::select(vec![Key::Enter, Key::Space]),
    ) {
        let config = Config { table, ..leader_config(F13) };
        let mut events = vec![
            (KeyEvent::press(Key::Function(13)), Duration::ZERO),
            (KeyEvent::release(Key::Function(13)), Duration::ZERO),
        ];
        for digit in code.digits() {
            events.push((KeyEvent::press(Key::Digit(*digit)), Duration::from_millis(999)));
            events.push((KeyEvent::release(Key::Digit(*digit)), Duration::ZERO));
        }
        events.push((KeyEvent::press(commit), Duration::from_millis(999)));
        events.push((KeyEvent::release(commit), Duration::ZERO));
        let run = Run::new(&config, &events);
        let expected: Vec<String> = config
            .resolver()
            .resolve(&code)
            .map(Resolution::to_text)
            .into_iter()
            .collect();
        prop_assert_eq!(run.texts(), expected);
        prop_assert!(run.delivered_keys().is_empty());
    }

    #[test]
    fn leader_mode_never_types_after_timeout(
        code in code_strategy(),
        pause in 1000u64..10_000,
    ) {
        let mut events = vec![
            (KeyEvent::press(Key::Function(13)), Duration::ZERO),
            (KeyEvent::release(Key::Function(13)), Duration::ZERO),
        ];
        for digit in code.digits() {
            events.push((KeyEvent::press(Key::Digit(*digit)), Duration::ZERO));
            events.push((KeyEvent::release(Key::Digit(*digit)), Duration::ZERO));
        }
        events.push((KeyEvent::press(Key::Enter), Duration::from_millis(pause)));
        events.push((KeyEvent::release(Key::Enter), Duration::ZERO));
        let run = Run::new(&leader_config(F13), &events);
        prop_assert!(run.texts().is_empty());
        let tail: Vec<_> = run.responses[run.responses.len() - 2..].to_vec();
        prop_assert_eq!(tail, vec![Response::Pass, Response::Pass]);
    }
}
