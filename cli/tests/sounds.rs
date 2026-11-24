//! `pencere sounds`: the kit's own sounds written as WAV files, twenty of them, each one a player
//! plays; and what the command says to what it cannot do.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// A directory of the test's own, not there.
fn fresh(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pencere-sounds-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// `pencere` with these arguments: its status and what it said, to its output and to its errors.
fn pencere(arguments: &[&str]) -> (Option<i32>, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_pencere"))
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("pencere runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The names a page asks the kit's sounds for.
#[cfg(feature = "terminal")]
const NAMES: [&str; 20] = [
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
];

#[cfg(feature = "terminal")]
#[test]
fn the_kits_sounds_are_written_as_twenty_wav_files_a_player_plays() {
    let dir = fresh("palette").join("not").join("there");
    let (code, said, errors) = pencere(&["sounds", dir.to_str().expect("a path")]);
    assert_eq!(code, Some(0), "{errors}");
    assert!(said.contains("wrote 20 sounds"), "{said}");
    let mut files: Vec<String> = fs::read_dir(&dir)
        .expect("the directory is made")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .into_string()
                .expect("a name")
        })
        .collect();
    files.sort();
    let mut expected: Vec<String> = NAMES.iter().map(|name| format!("{name}.wav")).collect();
    expected.sort();
    assert_eq!(files, expected);
    for name in NAMES {
        let bytes = fs::read(dir.join(format!("{name}.wav"))).expect("a file");
        assert_eq!(&bytes[0..4], b"RIFF", "{name}");
        assert_eq!(&bytes[8..16], b"WAVEfmt ", "{name}");
        let word = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        let long = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        // PCM, stereo, 48 kHz, 16 bits.
        assert_eq!(
            (word(20), word(22), long(24), word(34)),
            (1, 2, 48_000, 16),
            "{name}"
        );
        assert_eq!(&bytes[36..40], b"data", "{name}");
        let data = long(40) as usize;
        assert_eq!(bytes.len(), 44 + data, "{name}");
        assert_eq!(long(4) as usize, 36 + data, "{name}");
        let seconds = data as f64 / 4.0 / 48_000.0;
        assert!((0.04..=1.2).contains(&seconds), "{name} is {seconds} s");
        // There is sound in it, and it is not clipped.
        let samples: Vec<i16> = bytes[44..]
            .chunks(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let peak = samples
            .iter()
            .map(|s| i32::from(*s).abs())
            .max()
            .expect("samples");
        assert!((1000..30_000).contains(&peak), "{name}: {peak}");
    }
    // Asked again, the files are written over, not refused.
    let (code, _, errors) = pencere(&["sounds", dir.to_str().expect("a path")]);
    assert_eq!(code, Some(0), "{errors}");
    let _ = fs::remove_dir_all(dir.parent().and_then(|p| p.parent()).expect("a parent"));
}

#[cfg(not(feature = "terminal"))]
#[test]
fn the_command_with_no_terminal_says_it_holds_no_sounds() {
    let dir = fresh("no-terminal");
    let (code, _, errors) = pencere(&["sounds", dir.to_str().expect("a path")]);
    assert_eq!(code, Some(1));
    assert!(errors.contains("built with no terminal"), "{errors}");
    assert!(!dir.exists());
}

#[test]
fn it_wants_one_directory() {
    for arguments in [&["sounds"][..], &["sounds", "a", "b"]] {
        let (code, _, errors) = pencere(arguments);
        assert_eq!(code, Some(64), "{arguments:?}");
        assert!(errors.contains("sounds takes the directory"), "{errors}");
    }
}

#[cfg(feature = "terminal")]
#[test]
fn a_directory_that_cannot_be_made_is_said_and_fails() {
    let dir = fresh("blocked");
    fs::create_dir_all(&dir).expect("a directory");
    let blocker = dir.join("file");
    fs::write(&blocker, "not a directory").expect("a file");
    let target = blocker.join("sounds");
    let (code, _, errors) = pencere(&["sounds", target.to_str().expect("a path")]);
    assert_eq!(code, Some(1), "{errors}");
    assert!(errors.contains("could not be made"), "{errors}");
    let _ = fs::remove_dir_all(&dir);
}
