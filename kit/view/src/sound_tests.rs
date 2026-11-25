//! The names of the kit's sounds: each is once, in the words a script writes, and the list is the
//! one a page is told of.

use crate::sound::{MOST_VOICES, NAMES, Voices, is_name};

#[test]
fn the_names_are_each_once_and_in_the_words_a_script_writes() {
    let mut seen = std::collections::HashSet::new();
    for name in NAMES {
        assert!(seen.insert(name), "{name} twice");
        assert!(
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                && !name.starts_with('_')
                && !name.ends_with('_'),
            "{name}"
        );
    }
}

#[test]
fn a_name_is_one_of_the_twenty_and_nothing_else_is() {
    assert!(is_name("tap"));
    assert!(is_name("celebrate"));
    assert!(!is_name("Tap"));
    assert!(!is_name("toggle"));
    assert!(!is_name("sounds/tap.wav"));
    assert!(!is_name(""));
    assert_eq!(NAMES.iter().filter(|name| is_name(name)).count(), 20);
}

#[test]
fn the_names_are_the_ones_a_page_is_told_of() {
    assert_eq!(
        NAMES,
        [
            "tap",
            "select",
            "toggle_on",
            "toggle_off",
            "type",
            "open",
            "close",
            "snap",
            "copy",
            "delete",
            "success",
            "warning",
            "error",
            "send",
            "receive",
            "notify",
            "attention",
            "start",
            "complete",
            "celebrate",
        ]
    );
}

#[test]
fn the_ninth_sound_silences_the_one_that_has_played_longest() {
    let mut voices = Voices::new();
    for id in 0..MOST_VOICES as u64 {
        assert_eq!(voices.play(0.0 + id as f64 * 0.01, id, 5.0), None, "{id}");
    }
    assert_eq!(voices.playing(0.1), 8);
    // The ninth takes the place of the first, the tenth of the second.
    assert_eq!(voices.play(0.2, 8, 5.0), Some(0));
    assert_eq!(voices.play(0.3, 9, 5.0), Some(1));
    assert_eq!(voices.playing(0.4), 8);
}

#[test]
fn a_sound_that_has_ended_is_not_playing_and_leaves_room() {
    let mut voices = Voices::new();
    for id in 0..8u64 {
        voices.play(0.0, id, 0.5);
    }
    assert_eq!(voices.playing(0.4), 8);
    assert_eq!(voices.playing(0.5), 0);
    // They are over, so nothing is silenced for the ninth.
    assert_eq!(voices.play(1.0, 8, 0.5), None);
    assert_eq!(voices.playing(1.2), 1);
}

#[test]
fn ending_every_sound_names_those_that_were_playing() {
    let mut voices = Voices::new();
    voices.play(0.0, 1, 0.1);
    voices.play(0.0, 2, 3.0);
    voices.play(0.5, 3, 3.0);
    assert_eq!(voices.stop_all(1.0), [2, 3]);
    assert_eq!(voices.playing(1.0), 0);
    assert!(voices.stop_all(1.0).is_empty());
    assert_eq!(voices.play(1.0, 4, 1.0), None);
}
