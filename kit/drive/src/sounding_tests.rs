//! What the drive does with the sounds a page asks for: the kit's own made at the pitch asked for,
//! a file given with the ask when it is decoded and after it when it is not, what is waited for no
//! longer than a page may, and what an end of every sound ends.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use pencere_sandbox::Assets;
use pencere_sandbox::sounds::{SoundLimits, palette, wav};
use pencere_view::sound::{Ask, Play, Source};

use crate::sounding::Desk;

/// A door that is shut until the test opens it.
#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    opened: Condvar,
}

impl Gate {
    fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.opened.wait(open).unwrap();
        }
    }

    fn open(&self) {
        *self.open.lock().unwrap() = true;
        self.opened.notify_all();
    }
}

/// A bundle of files that can keep the reading waiting.
struct Bundle {
    files: HashMap<String, Arc<[u8]>>,
    gate: Option<Arc<Gate>>,
}

impl Assets for Bundle {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        if let Some(gate) = &self.gate {
            gate.wait();
        }
        self.files.get(path).cloned()
    }

    fn media_type(&self, _path: &str) -> Option<String> {
        None
    }
}

/// The palette's `name` as a WAV file: a sound that is read and decoded as a page's own.
fn file_of(name: &str) -> Arc<[u8]> {
    wav(&palette(name).unwrap()).unwrap().into()
}

fn desk(gate: Option<Arc<Gate>>, wait: Duration) -> (Desk, Receiver<()>) {
    let (told, woke) = channel();
    let told = Mutex::new(told);
    let bundle = Bundle {
        files: HashMap::from([
            ("ding.wav".to_owned(), file_of("success")),
            ("bad.wav".to_owned(), Arc::from(&b"not a sound"[..])),
        ]),
        gate,
    };
    let limits = SoundLimits {
        wait,
        ..SoundLimits::default()
    };
    let desk = Desk::new(Some(Arc::new(bundle)), limits, move || {
        let _ = told.lock().unwrap().send(());
    });
    (desk, woke)
}

fn kit(name: &str, pitch: f32) -> Ask {
    Ask::Play(Play {
        source: Source::Kit(name.into()),
        volume: 1.0,
        pitch,
        samples: None,
    })
}

fn file(path: &str, volume: f32) -> Ask {
    Ask::Play(Play {
        source: Source::File(path.into()),
        volume,
        pitch: 1.0,
        samples: None,
    })
}

/// What the ask plays, if it plays.
fn played(ask: &Ask) -> Option<&Play> {
    match ask {
        Ask::Play(play) => Some(play),
        Ask::Stop => None,
    }
}

/// Takes what the decoder has done once it has woken the host.
fn taken(desk: &mut Desk, woke: &Receiver<()>, told: &mut Vec<String>) {
    woke.recv_timeout(Duration::from_secs(10)).unwrap();
    desk.take(told);
}

#[test]
fn a_sound_of_the_kit_is_given_with_its_samples_at_the_pitch_asked_for() {
    let (mut desk, _woke) = desk(None, Duration::from_secs(1));
    let mut told = Vec::new();
    let got = desk.resolve(
        vec![
            kit("success", 1.0),
            kit("success", 2.0),
            kit("success", 0.5),
        ],
        &mut told,
    );
    assert!(told.is_empty());
    let frames: Vec<usize> = got
        .iter()
        .map(|ask| played(ask).unwrap().samples.as_ref().unwrap().frames())
        .collect();
    let whole = palette("success").unwrap().frames();
    // Twice as fast is half as long, half as fast twice as long.
    assert_eq!(frames[0], whole);
    assert!(frames[1].abs_diff(whole / 2) <= 1, "{frames:?}");
    assert!(frames[2].abs_diff(whole * 2) <= 1, "{frames:?}");
    // What the page asked is as it asked: the sound carries it.
    assert_eq!(played(&got[1]).unwrap().pitch, 2.0);
    assert_eq!(
        played(&got[0]).unwrap().source,
        Source::Kit("success".into())
    );
}

#[test]
fn the_plays_of_a_sound_played_often_differ_and_the_others_are_the_same_sound_every_time() {
    let (mut desk, _woke) = desk(None, Duration::from_secs(1));
    let mut told = Vec::new();
    let got = desk.resolve(
        vec![
            kit("tap", 1.0),
            kit("tap", 1.0),
            kit("success", 1.0),
            kit("success", 1.0),
        ],
        &mut told,
    );
    let samples = |at: usize| played(&got[at]).unwrap().samples.clone().unwrap();
    assert_ne!(samples(0), samples(1), "two taps are one tap");
    assert!(
        Arc::ptr_eq(&samples(2), &samples(3)),
        "a sound that does not vary is the palette's own"
    );
    // The same numbered play is the same on another view.
    let (mut other, _woke) = self::desk(None, Duration::from_secs(1));
    let again = other.resolve(vec![kit("tap", 1.0), kit("tap", 1.0)], &mut told);
    assert_eq!(played(&again[0]), played(&got[0]));
    assert_eq!(played(&again[1]), played(&got[1]));
}

#[test]
fn a_file_that_is_not_decoded_yet_is_given_when_it_is_and_after_what_was_ready() {
    let (mut desk, woke) = desk(None, Duration::from_secs(10));
    let mut told = Vec::new();
    let got = desk.resolve(vec![file("ding.wav", 0.5), kit("tap", 1.0)], &mut told);
    // The kit's sound is not kept waiting for the file.
    assert_eq!(got.len(), 1);
    assert_eq!(played(&got[0]).unwrap().source, Source::Kit("tap".into()));
    assert!(desk.waiting());
    assert!(desk.late().is_empty());
    taken(&mut desk, &woke, &mut told);
    assert!(!desk.waiting());
    assert!(told.is_empty(), "{told:?}");
    let late = desk.late();
    assert_eq!(late.len(), 1);
    let play = played(&late[0]).unwrap();
    assert_eq!(play.source, Source::File("ding.wav".into()));
    assert_eq!(play.volume, 0.5);
    assert_eq!(
        play.samples.as_ref().unwrap().frames(),
        palette("success").unwrap().frames()
    );
    // What is taken is taken once.
    assert!(desk.late().is_empty());
    // A file that was decoded is given with the ask, whenever the page asks again.
    let again = desk.resolve(vec![file("ding.wav", 1.0)], &mut told);
    assert_eq!(again.len(), 1);
    assert!(played(&again[0]).unwrap().samples.is_some());
    assert!(!desk.waiting());
}

#[test]
fn files_decoded_later_are_given_in_the_order_they_were_asked_for() {
    let (mut desk, woke) = desk(None, Duration::from_secs(10));
    let mut told = Vec::new();
    desk.resolve(
        vec![
            file("ding.wav", 0.1),
            file("ding.wav", 0.2),
            file("ding.wav", 0.3),
        ],
        &mut told,
    );
    while desk.waiting() {
        taken(&mut desk, &woke, &mut told);
    }
    let volumes: Vec<f32> = desk
        .late()
        .iter()
        .map(|ask| played(ask).unwrap().volume)
        .collect();
    assert_eq!(volumes, [0.1, 0.2, 0.3]);
}

#[test]
fn the_end_of_every_sound_ends_the_ones_that_are_waiting() {
    let gate = Arc::new(Gate::default());
    let (mut desk, woke) = desk(Some(Arc::clone(&gate)), Duration::from_secs(10));
    let mut told = Vec::new();
    desk.resolve(vec![file("ding.wav", 1.0)], &mut told);
    assert!(desk.waiting());
    let got = desk.resolve(vec![Ask::Stop, kit("tap", 1.0)], &mut told);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], Ask::Stop);
    assert!(!desk.waiting());
    // The decoder is not waited for either, nor the author told it was late.
    assert!(desk.deadline().is_none());
    // It is decoded after, and nobody plays it.
    gate.open();
    woke.recv_timeout(Duration::from_secs(10)).unwrap();
    desk.take(&mut told);
    assert!(desk.late().is_empty());
    assert!(told.is_empty(), "{told:?}");
    // A file asked for after the end is not ended by it.
    let gate2 = Arc::new(Gate::default());
    let (mut desk, woke) = self::desk(Some(Arc::clone(&gate2)), Duration::from_secs(10));
    desk.resolve(vec![Ask::Stop, file("ding.wav", 1.0)], &mut told);
    gate2.open();
    taken(&mut desk, &woke, &mut told);
    assert_eq!(desk.late().len(), 1);
}

#[test]
fn the_end_of_every_sound_ends_the_ones_decoded_since_the_host_last_looked() {
    let (mut desk, woke) = desk(None, Duration::from_secs(10));
    let mut told = Vec::new();
    desk.resolve(vec![file("ding.wav", 1.0)], &mut told);
    taken(&mut desk, &woke, &mut told);
    // Decoded, not given yet; the page has said the end of them all.
    desk.resolve(vec![Ask::Stop], &mut told);
    assert!(desk.late().is_empty());
}

#[test]
fn a_file_that_is_not_decoded_when_the_page_may_wait_no_longer_is_dropped_and_the_author_told() {
    let gate = Arc::new(Gate::default());
    let (mut desk, woke) = desk(Some(Arc::clone(&gate)), Duration::from_millis(40));
    let mut told = Vec::new();
    desk.resolve(vec![file("ding.wav", 1.0)], &mut told);
    assert!(desk.waiting());
    let deadline = desk.deadline().expect("a sound waits");
    std::thread::sleep(
        deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(30),
    );
    desk.take(&mut told);
    assert!(!desk.waiting());
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("\"ding.wav\""), "{told:?}");
    assert!(told[0].contains("40 ms"), "{told:?}");
    assert!(desk.deadline().is_none());
    // It is decoded after that and is not played; it is there for the next ask.
    gate.open();
    woke.recv_timeout(Duration::from_secs(10)).unwrap();
    desk.take(&mut told);
    assert!(desk.late().is_empty());
    assert_eq!(told.len(), 1, "told once: {told:?}");
    let again = desk.resolve(vec![file("ding.wav", 1.0)], &mut told);
    assert_eq!(again.len(), 1);
}

#[test]
fn a_file_that_is_no_sound_or_not_there_is_told_once_and_asks_nothing() {
    let (mut desk, woke) = desk(None, Duration::from_secs(10));
    let mut told = Vec::new();
    desk.resolve(vec![file("bad.wav", 1.0), file("gone.wav", 1.0)], &mut told);
    while desk.waiting() {
        taken(&mut desk, &woke, &mut told);
    }
    assert!(desk.late().is_empty());
    assert_eq!(told.len(), 2, "{told:?}");
    assert!(told.iter().any(|said| said.contains("\"bad.wav\"")));
    assert!(told.iter().any(|said| said.contains("\"gone.wav\"")));
    // Again: nothing more is said of it, and nothing played.
    let got = desk.resolve(vec![file("bad.wav", 1.0)], &mut told);
    assert!(got.is_empty());
    assert_eq!(told.len(), 2, "{told:?}");
}

#[test]
fn a_page_with_no_bundle_has_the_kits_sounds_and_no_files() {
    let mut desk = Desk::new(None, SoundLimits::default(), || {});
    let mut told = Vec::new();
    let got = desk.resolve(vec![file("ding.wav", 1.0), kit("tap", 1.0)], &mut told);
    assert_eq!(got.len(), 1);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].contains("\"ding.wav\""), "{told:?}");
    assert!(!desk.waiting());
    assert!(desk.deadline().is_none());
}

#[test]
fn a_sound_the_page_ended_is_not_told_late() {
    let gate = Arc::new(Gate::default());
    let (mut desk, _woke) = desk(Some(Arc::clone(&gate)), Duration::from_millis(30));
    let mut told = Vec::new();
    desk.resolve(vec![file("ding.wav", 1.0)], &mut told);
    desk.resolve(vec![Ask::Stop], &mut told);
    std::thread::sleep(Duration::from_millis(80));
    desk.take(&mut told);
    assert!(told.is_empty(), "{told:?}");
    gate.open();
}

#[test]
fn a_page_with_no_bundle_is_told_of_a_file_once() {
    let mut desk = Desk::new(None, SoundLimits::default(), || {});
    let mut told = Vec::new();
    for _ in 0..5 {
        desk.resolve(vec![file("ding.wav", 1.0)], &mut told);
    }
    desk.resolve(vec![file("other.wav", 1.0)], &mut told);
    assert_eq!(told.len(), 2, "{told:?}");
}
