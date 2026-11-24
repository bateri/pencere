//! The pattern of a vibration and the names a page may ask for it by: what one is made of, the
//! floors it is held to, and that every name is a pattern inside them.

use crate::haptic::{Event, MOST_EVENTS, MOST_SECONDS, NAMES, Pattern, Refusal, preset};

fn tap(at: f32, intensity: f32, sharpness: f32) -> Event {
    Event::tap(at, intensity, sharpness)
}

#[test]
fn a_pattern_keeps_its_events_in_the_order_of_their_times() {
    let pattern = Pattern::new([tap(0.3, 1.0, 0.5), tap(0.0, 0.5, 0.5), tap(0.1, 0.2, 0.5)])
        .expect("a pattern");
    let times: Vec<f32> = pattern.events().iter().map(|event| event.at).collect();
    assert_eq!(times, [0.0, 0.1, 0.3]);
}

#[test]
fn events_at_one_time_keep_the_order_they_were_given_in() {
    let pattern = Pattern::new([tap(0.0, 0.1, 0.5), tap(0.0, 0.2, 0.5), tap(0.0, 0.3, 0.5)])
        .expect("a pattern");
    let powers: Vec<f32> = pattern
        .events()
        .iter()
        .map(|event| event.intensity)
        .collect();
    assert_eq!(powers, [0.1, 0.2, 0.3]);
}

#[test]
fn numbers_past_their_range_are_held_to_it() {
    let pattern = Pattern::new([Event {
        at: -1.0,
        seconds: -2.0,
        intensity: 3.0,
        intensity_end: -3.0,
        sharpness: 9.0,
        sharpness_end: -9.0,
    }])
    .expect("a pattern");
    assert_eq!(
        pattern.events(),
        [Event {
            at: 0.0,
            seconds: 0.0,
            intensity: 1.0,
            intensity_end: 0.0,
            sharpness: 1.0,
            sharpness_end: 0.0,
        }]
    );
}

#[test]
fn a_number_that_is_not_one_is_refused() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for event in [
            Event {
                at: bad,
                ..tap(0.0, 0.5, 0.5)
            },
            Event {
                seconds: bad,
                ..tap(0.0, 0.5, 0.5)
            },
            Event {
                intensity: bad,
                ..tap(0.0, 0.5, 0.5)
            },
            Event {
                intensity_end: bad,
                ..tap(0.0, 0.5, 0.5)
            },
            Event {
                sharpness: bad,
                ..tap(0.0, 0.5, 0.5)
            },
            Event {
                sharpness_end: bad,
                ..tap(0.0, 0.5, 0.5)
            },
        ] {
            assert_eq!(Pattern::new([event]), Err(Refusal::NotANumber));
        }
    }
}

#[test]
fn a_pattern_has_at_most_so_many_events() {
    let events = |count: usize| (0..count).map(|index| tap(index as f32 * 0.01, 0.5, 0.5));
    assert!(Pattern::new(events(MOST_EVENTS)).is_ok());
    assert_eq!(Pattern::new(events(MOST_EVENTS + 1)), Err(Refusal::TooMany));
}

#[test]
fn a_pattern_lasts_at_most_so_many_seconds() {
    let held = |at: f32, seconds: f32| Event {
        seconds,
        ..tap(at, 0.5, 0.5)
    };
    assert!(Pattern::new([held(0.0, MOST_SECONDS)]).is_ok());
    assert!(Pattern::new([held(MOST_SECONDS - 1.0, 1.0)]).is_ok());
    assert_eq!(
        Pattern::new([held(1.0, MOST_SECONDS)]),
        Err(Refusal::TooLong)
    );
    assert_eq!(
        Pattern::new([tap(MOST_SECONDS + 1.0, 0.5, 0.5)]),
        Err(Refusal::TooLong)
    );
}

#[test]
fn a_pattern_with_no_events_is_a_pattern_and_has_no_length() {
    let none = Pattern::new([]).expect("a pattern");
    assert!(none.is_empty());
    assert_eq!(none.seconds(), 0.0);
}

#[test]
fn a_pattern_scaled_is_as_strong_as_it_was_by_that_much() {
    let pattern = Pattern::new([
        Event {
            seconds: 0.4,
            intensity: 1.0,
            intensity_end: 0.5,
            ..tap(0.0, 1.0, 0.7)
        },
        tap(0.5, 0.8, 0.2),
    ])
    .expect("a pattern");
    let half = pattern.scaled(0.5);
    assert_eq!(half.events()[0].intensity, 0.5);
    assert_eq!(half.events()[0].intensity_end, 0.25);
    assert_eq!(half.events()[1].intensity, 0.4);
    // How sharp it is, and when, are not its strength.
    assert_eq!(half.events()[0].sharpness, 0.7);
    assert_eq!(half.events()[1].at, 0.5);
    // A number past 0 to 1 is held to it, and the pattern it was made from is as it was.
    assert_eq!(pattern.scaled(7.0).events()[1].intensity, 0.8);
    assert_eq!(pattern.scaled(-1.0).events()[1].intensity, 0.0);
    assert_eq!(pattern.events()[0].intensity, 1.0);
}

#[test]
fn how_long_a_pattern_is_comes_of_the_last_thing_that_ends() {
    let pattern = Pattern::new([
        Event {
            seconds: 2.0,
            ..tap(0.5, 0.5, 0.5)
        },
        tap(1.0, 1.0, 1.0),
    ])
    .expect("a pattern");
    assert_eq!(pattern.seconds(), 2.5);
}

#[test]
fn every_name_is_a_pattern_that_is_inside_the_floors_and_has_something_in_it() {
    assert!(!NAMES.is_empty());
    for name in NAMES {
        let pattern = preset(name).unwrap_or_else(|| panic!("{name} is a name with no pattern"));
        assert!(!pattern.is_empty(), "{name} has no events");
        assert!(pattern.events().len() <= MOST_EVENTS, "{name}");
        assert!(pattern.seconds() <= MOST_SECONDS, "{name}");
        // Nothing to be held to: what it is made of is what it asks for.
        assert_eq!(
            Pattern::new(pattern.events().iter().copied()),
            Ok(pattern.clone()),
            "{name}"
        );
        for event in pattern.events() {
            assert!(event.at >= 0.0 && event.seconds >= 0.0, "{name}");
            for number in [
                event.intensity,
                event.intensity_end,
                event.sharpness,
                event.sharpness_end,
            ] {
                assert!((0.0..=1.0).contains(&number), "{name}: {number}");
            }
            assert!(
                event.intensity.max(event.intensity_end) > 0.0,
                "{name} is silent"
            );
        }
    }
}

#[test]
fn the_names_are_each_once_and_a_name_that_is_not_one_has_no_pattern() {
    let mut names: Vec<&str> = NAMES.to_vec();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), NAMES.len());
    assert!(preset("nope").is_none());
    assert!(preset("").is_none());
    assert!(preset("Tick").is_none());
}

#[test]
fn the_names_are_the_ones_a_page_is_told_of() {
    assert_eq!(
        NAMES,
        [
            "tick",
            "tap",
            "thud",
            "snap",
            "bump",
            "double",
            "success",
            "warning",
            "error",
            "rise",
            "fall",
            "rumble",
            "heartbeat",
        ]
    );
}
