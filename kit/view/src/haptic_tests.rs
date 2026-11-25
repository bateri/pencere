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
fn the_peak_of_a_pattern_is_the_strongest_it_gets_where_an_event_begins_or_ends() {
    let pattern = Pattern::new([
        Event {
            seconds: 0.4,
            intensity: 0.2,
            intensity_end: 0.9,
            ..tap(0.0, 0.2, 0.7)
        },
        tap(0.5, 0.8, 0.2),
    ])
    .expect("a pattern");
    assert_eq!(pattern.peak(), 0.9);
    assert_eq!(Pattern::default().peak(), 0.0);
}

#[test]
fn a_pattern_with_its_peak_at_a_strength_has_its_strongest_moment_there_and_keeps_its_shape() {
    let pattern = Pattern::new([
        Event {
            seconds: 0.4,
            intensity: 0.3,
            intensity_end: 0.15,
            ..tap(0.0, 0.3, 0.7)
        },
        tap(0.5, 0.2, 0.2),
    ])
    .expect("a pattern");
    // Weaker: the strongest moment is what was said, and the others are in the same proportion.
    let half = pattern.with_peak(0.15);
    assert_eq!(half.peak(), 0.15);
    assert_eq!(half.events()[0].intensity, 0.15);
    assert_eq!(half.events()[0].intensity_end, 0.075);
    assert!((half.events()[1].intensity - 0.1).abs() < 1e-6);
    // Stronger too: a ready pattern is as light as it is made, and a page can ask for more of it.
    let more = pattern.with_peak(1.0);
    assert_eq!(more.peak(), 1.0);
    assert_eq!(more.events()[0].intensity, 1.0);
    assert_eq!(more.events()[0].intensity_end, 0.5);
    assert!((more.events()[1].intensity - 2.0 / 3.0).abs() < 1e-6);
    // How sharp it is, and when, are not its strength.
    for changed in [&half, &more] {
        assert_eq!(changed.events()[0].sharpness, 0.7);
        assert_eq!(changed.events()[0].seconds, 0.4);
        assert_eq!(changed.events()[1].at, 0.5);
        assert_eq!(changed.events()[1].sharpness, 0.2);
    }
    // The pattern it was made from is as it was.
    assert_eq!(pattern.peak(), 0.3);
}

#[test]
fn a_peak_is_exactly_what_was_said_and_never_more_than_one_whatever_the_rounding() {
    for name in NAMES {
        let ready = preset(name).expect("a name");
        for peak in [0.0, 0.01, 0.09, 0.1, 0.18, 0.3, 0.33, 0.5, 0.7, 0.99, 1.0] {
            let changed = ready.with_peak(peak);
            assert_eq!(changed.peak(), peak, "{name} at {peak}");
            assert_eq!(changed.events().len(), ready.events().len(), "{name}");
            for event in changed.events() {
                assert!(
                    (0.0..=1.0).contains(&event.intensity)
                        && (0.0..=1.0).contains(&event.intensity_end),
                    "{name} at {peak}"
                );
            }
        }
    }
}

#[test]
fn a_peak_past_zero_to_one_is_the_nearest_and_one_that_is_no_number_leaves_the_pattern() {
    let pattern = preset("thud").expect("a name");
    assert_eq!(pattern.with_peak(7.0).peak(), 1.0);
    assert_eq!(pattern.with_peak(-1.0).peak(), 0.0);
    // Zero is still a pattern, a silent one: it is not the end of what plays.
    assert_eq!(pattern.with_peak(0.0).events().len(), 2);
    assert_eq!(pattern.with_peak(f32::NAN), pattern);
    assert_eq!(pattern.with_peak(f32::INFINITY).peak(), 1.0);
}

#[test]
fn a_pattern_that_is_silent_has_no_shape_to_keep_and_is_as_it_was() {
    let silent = Pattern::new([tap(0.0, 0.0, 0.5), tap(0.1, 0.0, 0.9)]).expect("a pattern");
    assert_eq!(silent.with_peak(0.5), silent);
    assert_eq!(Pattern::default().with_peak(0.5), Pattern::default());
}

#[test]
fn the_ready_patterns_are_the_strengths_they_are_designed_at_and_none_gets_past_a_share_of_three_in_ten()
 {
    // Each name with the strength of every event (a held event's from its beginning to its end).
    let designed: [(&str, &[(f32, f32)]); 13] = [
        ("tick", &[(0.18, 0.18)]),
        ("tap", &[(0.225, 0.225)]),
        ("thud", &[(0.3, 0.3), (0.21, 0.0)]),
        ("snap", &[(0.3, 0.3)]),
        ("bump", &[(0.21, 0.21)]),
        ("double", &[(0.21, 0.21), (0.21, 0.21)]),
        ("success", &[(0.18, 0.18), (0.3, 0.3)]),
        ("warning", &[(0.27, 0.27), (0.18, 0.18)]),
        ("error", &[(0.3, 0.3), (0.3, 0.3), (0.3, 0.3)]),
        ("rise", &[(0.09, 0.3)]),
        ("fall", &[(0.3, 0.09)]),
        ("rumble", &[(0.21, 0.21)]),
        (
            "heartbeat",
            &[(0.27, 0.27), (0.18, 0.18), (0.27, 0.27), (0.18, 0.18)],
        ),
    ];
    for (name, strengths) in designed {
        let pattern = preset(name).expect("a name");
        let made: Vec<(f32, f32)> = pattern
            .events()
            .iter()
            .map(|event| (event.intensity, event.intensity_end))
            .collect();
        assert_eq!(made, strengths, "{name}");
        assert!(pattern.peak() <= 0.3, "{name}");
    }
    assert_eq!(designed.len(), NAMES.len());
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
