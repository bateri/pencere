//! The names of the kit's sounds: each is once, in the words a script writes, and the list is the
//! one a page is told of.

use crate::sound::{NAMES, is_name};

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
