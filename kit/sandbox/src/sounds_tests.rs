//! The guards of the sounds: that the palette is twenty sounds that are what their names say (the
//! length of the job, under the ceiling, quiet where they are the person's own and loud where they
//! are what happened, starting and ending in silence); that WAV and Ogg Vorbis are read and what is
//! not them is refused without a panic; that the limits hold before a sample is made; that a pitch
//! is a pitch; and that a file is read off the thread that asks, once, and waited for no longer
//! than a page may.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use pencere_view::sound::NAMES;

use crate::images::Assets;
use crate::sounds::decode::decode;
use crate::sounds::palette::{SPECS, Tier, make, spec_of};
use crate::sounds::{
    Ask, Failure, Sound, SoundLimits, Sounds, palette, palette_wavs, play_of, variation, wav,
};

// --- The palette --------------------------------------------------------------------------------

/// What each sound is for, and how long a sound for it is: the lengths of the same jobs' cues in
/// the catalogue of uisfx (MIT, Copyright (c) 2026 Yuki Capital), in seconds. A sound is made to
/// them within the tolerance below.
const JOBS: [(&str, f64); 20] = [
    ("tap", 0.16),
    ("select", 0.28),
    ("toggle_on", 0.30),
    ("toggle_off", 0.29),
    ("type", 0.045),
    ("open", 0.37),
    ("close", 0.34),
    ("snap", 0.16),
    ("copy", 0.30),
    ("delete", 0.48),
    ("success", 0.72),
    ("warning", 0.68),
    ("error", 0.62),
    ("send", 0.42),
    ("receive", 0.47),
    ("notify", 0.58),
    ("attention", 0.64),
    ("start", 0.46),
    ("complete", 0.80),
    ("celebrate", 1.16),
];

/// How far from the length of its job a sound may be: a tenth, or 2 ms.
fn within_job(seconds: f64, job: f64) -> bool {
    (seconds - job).abs() <= (job * 0.1).max(0.002)
}

/// -1 dBFS.
const CEILING: f32 = 0.891;

fn all() -> Vec<(&'static str, Arc<Sound>)> {
    NAMES
        .iter()
        .map(|&name| (name, palette(name).expect("a sound of the palette")))
        .collect()
}

fn frame(sound: &Sound, index: usize) -> (f32, f32) {
    let at = index * 2;
    (sound.samples()[at], sound.samples()[at + 1])
}

#[test]
fn the_palette_has_the_twenty_names_a_page_is_told_of_and_no_other() {
    assert_eq!(SPECS.map(|spec| spec.name), NAMES);
    assert_eq!(all().len(), 20);
    for name in ["", "nope", "Tap", "toggle", "sounds/tap.wav"] {
        assert!(palette(name).is_none(), "{name}");
        assert!(spec_of(name).is_none(), "{name}");
    }
}

#[test]
fn every_sound_is_stereo_at_48_khz_and_has_something_in_it() {
    for (name, sound) in all() {
        assert_eq!((sound.rate(), sound.channels()), (48_000, 2), "{name}");
        let mean_square = sound
            .samples()
            .iter()
            .map(|s| f64::from(*s) * f64::from(*s))
            .sum::<f64>()
            / sound.samples().len() as f64;
        assert!(mean_square.sqrt() > 0.005, "{name} is silent");
    }
}

#[test]
fn every_sound_is_the_length_of_its_job() {
    for (name, job) in JOBS {
        let sound = palette(name).expect("a sound");
        assert!(
            within_job(sound.seconds(), job),
            "{name} is {} s, its job is {job} s",
            sound.seconds()
        );
    }
}

#[test]
fn every_sound_is_under_minus_one_dbfs_and_as_loud_as_what_it_is_for() {
    for (name, sound) in all() {
        let spec = spec_of(name).expect("a spec");
        assert!(sound.peak() < CEILING, "{name}: {}", sound.peak());
        assert!(
            (f64::from(sound.peak()) - spec.tier.peak()).abs() < 0.005,
            "{name}: {} is not the {:?} tier's {}",
            sound.peak(),
            spec.tier,
            spec.tier.peak()
        );
    }
}

#[test]
fn what_a_person_does_is_quieter_than_what_happened_and_that_than_what_needs_them() {
    let peak_of = |tier: Tier| {
        all()
            .iter()
            .filter(|(name, _)| spec_of(name).expect("a spec").tier == tier)
            .map(|(_, sound)| sound.peak())
            .collect::<Vec<_>>()
    };
    let (input, result, alert) = (
        peak_of(Tier::Input),
        peak_of(Tier::Result),
        peak_of(Tier::Alert),
    );
    let most = |peaks: &[f32]| peaks.iter().copied().fold(0.0, f32::max);
    let least = |peaks: &[f32]| peaks.iter().copied().fold(f32::MAX, f32::min);
    assert!(!input.is_empty() && !result.is_empty() && !alert.is_empty());
    assert!(most(&input) < least(&result), "{input:?} {result:?}");
    assert!(most(&result) < least(&alert), "{result:?} {alert:?}");
    // The sounds that are the person's own are the ones of the tap, the key and the switch.
    for name in ["tap", "type", "select", "toggle_on", "toggle_off", "snap"] {
        assert_eq!(spec_of(name).expect("a spec").tier, Tier::Input, "{name}");
    }
    for name in ["error", "warning", "attention"] {
        assert_eq!(spec_of(name).expect("a spec").tier, Tier::Alert, "{name}");
    }
}

#[test]
fn every_sound_starts_and_ends_in_silence() {
    for (name, sound) in all() {
        let (first_l, first_r) = frame(&sound, 0);
        assert!(
            first_l.abs() < 1e-3 && first_r.abs() < 1e-3,
            "{name} starts with a click: {first_l} {first_r}"
        );
        let last = sound.frames() - 1;
        assert_eq!(
            frame(&sound, last),
            (0.0, 0.0),
            "{name} does not end in silence"
        );
        // And the last millisecond before the end is under half a percent of the peak.
        let tail = 48;
        for index in last - tail..=last {
            let (l, r) = frame(&sound, index);
            assert!(
                l.abs().max(r.abs()) <= 0.005 * sound.peak(),
                "{name} is {l} {r} at {} frames before its end",
                last - index
            );
        }
    }
}

/// How loud a sound is, 5 ms of it from `from` frames in, as the part of its peak.
fn level_at(sound: &Sound, from: usize) -> f32 {
    let window = 240;
    let sum: f32 = (from..from + window)
        .map(|index| {
            let (l, r) = frame(sound, index);
            l * l + r * r
        })
        .sum();
    (sum / (2 * window) as f32).sqrt() / sound.peak()
}

#[test]
fn a_sound_has_died_away_by_the_time_its_end_is_faded() {
    for (name, sound) in all() {
        let fade = ((0.028 * 48_000.0) as usize).min((sound.frames() as f64 * 0.4) as usize);
        let from = sound.frames() - fade - 240;
        // 34 dB under its peak at least, which a fade then takes the rest of the way: no
        // sound is cut off in the middle of what it was doing.
        assert!(
            level_at(&sound, from) < 0.02,
            "{name} is still at {} of its peak where it is faded",
            level_at(&sound, from)
        );
    }
}

#[test]
fn a_sound_is_the_same_every_time_it_is_made() {
    for spec in &SPECS {
        let (a, b) = (make(spec).expect("a sound"), make(spec).expect("a sound"));
        assert_eq!(a, b, "{}", spec.name);
        assert_eq!(*a, *palette(spec.name).expect("a sound"), "{}", spec.name);
    }
}

#[test]
fn no_two_sounds_are_alike_and_the_switch_is_one_way_on_and_another_off() {
    let sounds = all();
    for (i, (name, a)) in sounds.iter().enumerate() {
        for (other, b) in sounds.iter().skip(i + 1) {
            assert!(
                a.samples() != b.samples(),
                "{name} and {other} are one sound"
            );
        }
    }
}

#[test]
fn what_is_played_often_differs_a_little_every_time_and_what_happened_does_not() {
    let varying = ["tap", "select", "toggle_on", "toggle_off", "type", "snap"];
    for name in NAMES {
        if !varying.contains(&name) {
            assert_eq!(variation(name, 7), None, "{name}");
            assert!(!spec_of(name).expect("a spec").varies, "{name}");
        }
    }
    for name in varying {
        let mut pitches = std::collections::HashSet::new();
        for draw in 0..200 {
            let v = variation(name, draw).expect("a variation");
            assert!((v.pitch - 1.0).abs() <= 0.012, "{name}: {}", v.pitch);
            assert!((v.level - 1.0).abs() <= 0.12, "{name}: {}", v.level);
            assert_eq!(
                variation(name, draw),
                Some(v),
                "the same draw, the same play"
            );
            pitches.insert(v.pitch.to_bits());
        }
        assert!(
            pitches.len() > 100,
            "{name}: {} different pitches",
            pitches.len()
        );
    }
    assert_eq!(variation("nope", 1), None);
}

#[test]
fn a_sound_of_the_palette_is_a_wav_any_player_plays() {
    let files = palette_wavs();
    assert_eq!(files.len(), 20);
    for ((name, bytes), expected) in files.iter().zip(NAMES) {
        assert_eq!(*name, expected);
        assert!(bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"));
        let mut reader = hound::WavReader::new(Cursor::new(bytes)).expect("a WAV");
        let spec = reader.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (2, 48_000, 16)
        );
        let sound = palette(name).expect("a sound");
        let samples: Vec<i16> = reader
            .samples::<i16>()
            .map(|s| s.expect("a sample"))
            .collect();
        assert_eq!(samples.len(), sound.samples().len());
        // What is written is what was made, to the 16 bits it is written in.
        for (written, made) in samples.iter().zip(sound.samples()) {
            assert!(
                (f32::from(*written) / 32767.0 - made).abs() < 1.0 / 32767.0,
                "{name}"
            );
        }
    }
    let one = wav(&palette("tap").expect("a sound")).expect("a WAV");
    assert_eq!(one, files[0].1);
}

// --- The formats --------------------------------------------------------------------------------

const TONE_OGG: &[u8] = include_bytes!("fixtures/tone-stereo.ogg");

fn wav_ints(rate: u32, channels: u16, bits: u16, samples: &[i32]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: bits,
        sample_format: hound::SampleFormat::Int,
    };
    let mut out = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut out, spec).expect("a writer");
    for &s in samples {
        writer.write_sample(s).expect("a sample");
    }
    writer.finalize().expect("a WAV");
    out.into_inner()
}

fn wav_floats(rate: u32, channels: u16, samples: &[f32]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut out = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut out, spec).expect("a writer");
    for &s in samples {
        writer.write_sample(s).expect("a sample");
    }
    writer.finalize().expect("a WAV");
    out.into_inner()
}

const MOST: usize = 64 << 20;

#[test]
fn a_wav_of_any_depth_is_read_as_floats_from_minus_one_to_one() {
    // 8 bits, 16, 24 and 32: the same four values, from the lowest to the highest the depth has.
    for (bits, low, mid, high) in [
        (8u16, -128i32, 64i32, 127i32),
        (16, -32768, 16384, 32767),
        (24, -8_388_608, 4_194_304, 8_388_607),
        (32, i32::MIN, 1 << 30, i32::MAX),
    ] {
        let bytes = wav_ints(8000, 1, bits, &[low, 0, mid, high]);
        let sound = decode(&bytes, MOST).expect("a sound");
        assert_eq!(
            (sound.rate(), sound.channels(), sound.frames()),
            (8000, 1, 4),
            "{bits}"
        );
        let s = sound.samples();
        assert!((s[0] + 1.0).abs() < 1e-6, "{bits}: {}", s[0]);
        assert_eq!(s[1], 0.0, "{bits}");
        assert!((s[2] - 0.5).abs() < 1e-3, "{bits}: {}", s[2]);
        assert!((s[3] - 1.0).abs() < 1e-2, "{bits}: {}", s[3]);
    }
}

#[test]
fn a_wav_of_floats_is_read_as_it_is_and_what_is_no_number_is_silence() {
    let bytes = wav_floats(44_100, 2, &[0.25, -0.5, f32::NAN, 3.0, f32::INFINITY, -9.0]);
    let sound = decode(&bytes, MOST).expect("a sound");
    assert_eq!(
        (sound.rate(), sound.channels(), sound.frames()),
        (44_100, 2, 3)
    );
    assert_eq!(sound.samples(), [0.25, -0.5, 0.0, 1.0, 0.0, -1.0]);
}

#[test]
fn a_stereo_wav_keeps_its_two_sides_apart() {
    let bytes = wav_ints(22_050, 2, 16, &[1000, -1000, 2000, -2000]);
    let sound = decode(&bytes, MOST).expect("a sound");
    assert_eq!(sound.channels(), 2);
    let s = sound.samples();
    assert!(s[0] > 0.0 && s[1] < 0.0 && s[2] > s[0] && s[3] < s[1]);
}

#[test]
fn an_ogg_vorbis_file_is_read_by_its_bytes() {
    let sound = decode(TONE_OGG, MOST).expect("a sound");
    assert_eq!((sound.rate(), sound.channels()), (22_050, 2));
    // A tenth of a second, give or take the block the codec pads it with.
    assert!(
        (sound.seconds() - 0.1).abs() < 0.06,
        "{} s",
        sound.seconds()
    );
    assert!(
        sound.peak() > 0.05 && sound.peak() <= 1.0,
        "{}",
        sound.peak()
    );
}

#[test]
fn the_format_is_told_from_the_bytes_and_not_from_a_name() {
    // An Ogg under the name of a WAV, and the other way round: the bytes say.
    let bytes = wav_ints(8000, 1, 16, &[0, 1000, -1000, 500]);
    assert!(decode(&bytes, MOST).is_ok());
    assert!(decode(TONE_OGG, MOST).is_ok());
    assert_eq!(
        decode(b"\x89PNG\r\n\x1a\n and then some", MOST),
        Err(Failure::Unknown)
    );
    assert_eq!(
        decode(b"ID3\x04 an mp3 perhaps", MOST),
        Err(Failure::Unknown)
    );
    assert_eq!(decode(b"", MOST), Err(Failure::Unknown));
    assert_eq!(decode(b"RIFF", MOST), Err(Failure::Unknown));
}

#[test]
fn bytes_that_are_no_sound_are_refused_and_nothing_panics() {
    let good = wav_ints(
        8000,
        1,
        16,
        &(0..400).map(|i| i * 50 % 3000).collect::<Vec<_>>(),
    );
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    // Cut short anywhere: a header with no data, a header cut in two, data cut off.
    for cut in [12, 20, 36, 44, 100, good.len() - 1] {
        cases.push((format!("a WAV cut at {cut}"), good[..cut].to_vec()));
    }
    // Every byte of the header, changed.
    for at in 0..44 {
        let mut broken = good.clone();
        broken[at] ^= 0xFF;
        cases.push((format!("a WAV with byte {at} changed"), broken));
    }
    // An Ogg cut short and spoiled, and nothing but its first word.
    for cut in [4, 27, 58, 200, TONE_OGG.len() / 2] {
        cases.push((format!("an Ogg cut at {cut}"), TONE_OGG[..cut].to_vec()));
    }
    for at in (0..TONE_OGG.len()).step_by(37) {
        let mut broken = TONE_OGG.to_vec();
        broken[at] ^= 0x5A;
        cases.push((format!("an Ogg with byte {at} changed"), broken));
    }
    cases.push(("OggS and nothing".into(), b"OggS".to_vec()));
    cases.push((
        "OggS and noise".into(),
        b"OggS"
            .iter()
            .copied()
            .chain((0..500u32).map(|i| (i * 31 % 251) as u8))
            .collect(),
    ));
    cases.push((
        "RIFF WAVE and noise".into(),
        b"RIFF\0\0\0\0WAVEfmt "
            .iter()
            .copied()
            .chain((0..500u32).map(|i| (i * 17 % 241) as u8))
            .collect(),
    ));
    for (what, bytes) in cases {
        // Whatever it makes of it, it is a sound or a refusal, and the thread it ran on is here.
        if let Ok(sound) = decode(&bytes, MOST) {
            assert!(sound.frames() > 0, "{what}");
        }
    }
    // The ones that are certain to be refused.
    assert!(
        matches!(decode(&good[..44], MOST), Err(Failure::Broken(_))),
        "a header and no data"
    );
    assert!(
        matches!(decode(&good[..100], MOST), Err(Failure::Broken(_))),
        "data cut short"
    );
    assert!(matches!(decode(b"OggS", MOST), Err(Failure::Broken(_))));
}

#[test]
fn a_sound_has_one_channel_or_two_and_is_at_a_rate_a_device_plays() {
    let three = wav_ints(8000, 3, 16, &[1, 2, 3, 4, 5, 6]);
    assert_eq!(decode(&three, MOST), Err(Failure::Channels { count: 3 }));
    let slow = wav_ints(500, 1, 16, &[1, 2, 3]);
    assert_eq!(decode(&slow, MOST), Err(Failure::Rate { hertz: 500 }));
    let fast = wav_ints(1_000_000, 1, 16, &[1, 2, 3]);
    assert_eq!(decode(&fast, MOST), Err(Failure::Rate { hertz: 1_000_000 }));
    assert!(decode(&wav_ints(96_000, 2, 24, &[1, 2, 3, 4]), MOST).is_ok());
}

#[test]
fn a_sound_past_what_a_view_may_hold_is_refused_before_a_sample_is_made() {
    let samples = vec![100; 1000];
    let bytes = wav_ints(8000, 1, 16, &samples);
    // 1000 samples are 4000 bytes as floats.
    assert_eq!(
        decode(&bytes, 3999),
        Err(Failure::Whole {
            bytes: 4000,
            most: 3999
        })
    );
    assert!(decode(&bytes, 4000).is_ok());
    // A header that says the data is far more than it is, past what a view may hold at all: refused
    // by the header, with no room made for it.
    let mut lying = wav_ints(8000, 1, 16, &[1, 2, 3, 4]);
    let size: u32 = 40 << 20;
    lying[40..44].copy_from_slice(&size.to_le_bytes());
    assert!(matches!(
        decode(&lying, MOST),
        Err(Failure::Whole { most, .. }) if most == MOST
    ));
    // An Ogg is stopped where it comes to more than that, whatever its header said.
    assert!(matches!(
        decode(TONE_OGG, 1000),
        Err(Failure::Whole { most: 1000, .. })
    ));
}

#[test]
fn a_sound_played_faster_is_higher_and_shorter_and_slower_lower_and_longer() {
    // A second of 440 Hz, a sine, at 8000 Hz.
    let rate = 8000u32;
    let sine: Vec<f32> = (0..rate)
        .map(|n| (std::f32::consts::TAU * 440.0 * n as f32 / rate as f32).sin() * 0.5)
        .collect();
    let sound = Sound::new(rate, 1, sine).expect("a sound");
    let hertz = |s: &Sound| {
        let crossings = s
            .samples()
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        crossings as f64 / 2.0 / s.seconds()
    };
    assert!((hertz(&sound) - 440.0).abs() < 2.0, "{}", hertz(&sound));
    for (ratio, frames) in [
        (2.0f32, 4000usize),
        (0.5, 16_000),
        (1.5, 5333),
        (0.75, 10_667),
    ] {
        let pitched = sound.pitched(ratio);
        assert_eq!(pitched.rate(), rate);
        assert_eq!(pitched.channels(), 1);
        assert!(
            (pitched.frames() as i64 - frames as i64).abs() <= 1,
            "{ratio}: {}",
            pitched.frames()
        );
        assert!(
            (hertz(&pitched) - 440.0 * f64::from(ratio)).abs() < 5.0,
            "{ratio}: {}",
            hertz(&pitched)
        );
        assert!(pitched.peak() <= 0.51, "{ratio}: {}", pitched.peak());
    }
    assert_eq!(sound.pitched(1.0), sound);
    assert_eq!(sound.pitched(0.0), sound);
    assert_eq!(sound.pitched(-1.0), sound);
    assert_eq!(sound.pitched(f32::NAN), sound);
    // Two channels stay two, each on its own side.
    let stereo = Sound::new(
        rate,
        2,
        (0..2000)
            .flat_map(|n| [0.5, -0.25 * (n % 2) as f32])
            .collect(),
    )
    .expect("a sound");
    let faster = stereo.pitched(2.0);
    assert_eq!((faster.channels(), faster.frames()), (2, 1000));
    assert!(faster.samples().chunks(2).all(|f| f[0] > 0.4));
}

#[test]
fn what_is_no_sound_cannot_be_made_one() {
    assert!(Sound::new(8000, 1, vec![]).is_none());
    assert!(Sound::new(8000, 2, vec![0.0, 0.0, 0.0]).is_none());
    assert!(Sound::new(8000, 3, vec![0.0; 6]).is_none());
    assert!(Sound::new(8000, 0, vec![0.0; 6]).is_none());
    assert!(Sound::new(0, 1, vec![0.0; 6]).is_none());
    assert!(Sound::new(8000, 2, vec![0.0; 6]).is_some());
}

// --- The files and the host ---------------------------------------------------------------------

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

/// The files of a bundle, that remembers which it was asked for, can keep the asking waiting, and
/// panics for a path it is told to.
struct Bundle {
    files: HashMap<String, Arc<[u8]>>,
    reads: Mutex<Vec<String>>,
    gate: Option<Arc<Gate>>,
    panics: Option<String>,
}

impl Assets for Bundle {
    fn read(&self, path: &str) -> Option<Arc<[u8]>> {
        if let Some(gate) = &self.gate {
            gate.wait();
        }
        self.reads.lock().unwrap().push(path.to_owned());
        if self.panics.as_deref() == Some(path) {
            panic!("a read that gives up");
        }
        self.files.get(path).cloned()
    }

    fn media_type(&self, path: &str) -> Option<String> {
        self.files
            .contains_key(path)
            .then(|| "application/octet-stream".to_owned())
    }
}

struct Rig {
    sounds: Sounds,
    woke: Receiver<()>,
    bundle: Arc<Bundle>,
}

impl Rig {
    fn new(files: Vec<(&str, Vec<u8>)>, limits: SoundLimits) -> Self {
        Self::with(files, limits, None, None)
    }

    fn with(
        files: Vec<(&str, Vec<u8>)>,
        limits: SoundLimits,
        gate: Option<Arc<Gate>>,
        panics: Option<&str>,
    ) -> Self {
        let bundle = Arc::new(Bundle {
            files: files
                .into_iter()
                .map(|(path, bytes)| (path.to_owned(), Arc::from(bytes)))
                .collect(),
            reads: Mutex::new(Vec::new()),
            gate,
            panics: panics.map(str::to_owned),
        });
        let (woken, woke) = channel();
        let woken = Mutex::new(woken);
        let sounds = Sounds::new(Arc::clone(&bundle) as Arc<dyn Assets>, limits, move || {
            let _ = woken.lock().unwrap().send(());
        });
        Self {
            sounds,
            woke,
            bundle,
        }
    }

    /// Waits for the host to be woken, and gives what came.
    fn arrival(&self) -> crate::sounds::Arrival {
        self.woke
            .recv_timeout(Duration::from_secs(20))
            .expect("the host is woken");
        self.sounds.poll()
    }

    /// Waits until `count` sounds have come, and gives them.
    fn ready(&self, count: usize) -> Vec<(u64, Arc<Sound>)> {
        let mut ready = Vec::new();
        while ready.len() < count {
            ready.extend(self.arrival().ready);
        }
        ready
    }

    fn reads(&self) -> Vec<String> {
        self.bundle.reads.lock().unwrap().clone()
    }
}

fn beep(frames: usize) -> Vec<u8> {
    wav_ints(
        8000,
        1,
        16,
        &(0..frames as i32)
            .map(|n| (n % 100) * 100)
            .collect::<Vec<_>>(),
    )
}

fn pending(ask: Ask) -> u64 {
    match ask {
        Ask::Pending(ticket) => ticket,
        other => panic!("not pending: {other:?}"),
    }
}

#[test]
fn a_file_is_read_and_decoded_off_the_thread_that_asks_and_once() {
    let gate = Arc::new(Gate::default());
    let rig = Rig::with(
        vec![("sounds/beep.wav", beep(800))],
        SoundLimits::default(),
        Some(Arc::clone(&gate)),
        None,
    );
    // The door is shut and the ask returns: the read is not on this thread.
    let ticket = pending(rig.sounds.request("sounds/beep.wav", 1.0));
    assert!(rig.sounds.poll().is_empty());
    assert!(rig.sounds.deadline().is_some());
    gate.open();
    let arrival = rig.arrival();
    assert!(arrival.failed.is_empty() && arrival.diagnostics.is_empty());
    let [(arrived, sound)] = &arrival.ready[..] else {
        panic!("{arrival:?}")
    };
    assert_eq!(*arrived, ticket);
    assert_eq!(
        (sound.rate(), sound.channels(), sound.frames()),
        (8000, 1, 800)
    );
    assert!(rig.sounds.deadline().is_none());
    assert_eq!(rig.sounds.held(), 3200);
    // Asked again, it is there at once, and the file is not read again.
    let Ask::Ready(again) = rig.sounds.request("sounds/beep.wav", 1.0) else {
        panic!("not ready")
    };
    assert!(Arc::ptr_eq(&again, sound));
    assert_eq!(rig.reads(), ["sounds/beep.wav"]);
    // Nothing more is told, and nothing wakes the host at rest.
    assert!(rig.sounds.poll().is_empty());
    assert!(rig.woke.try_recv().is_err());
}

#[test]
fn a_sound_is_given_at_the_pitch_it_is_asked_for_and_kept_as_it_is() {
    let rig = Rig::new(vec![("beep.wav", beep(800))], SoundLimits::default());
    let ticket = pending(rig.sounds.request("beep.wav", 2.0));
    let arrival = rig.arrival();
    let [(arrived, sound)] = &arrival.ready[..] else {
        panic!("{arrival:?}")
    };
    assert_eq!((*arrived, sound.frames()), (ticket, 400));
    // What is kept is the file's own sounds, and the other pitches are made from it.
    assert_eq!(rig.sounds.held(), 3200);
    let Ask::Ready(plain) = rig.sounds.request("beep.wav", 1.0) else {
        panic!("not ready")
    };
    assert_eq!(plain.frames(), 800);
    let ticket = pending(rig.sounds.request("beep.wav", 0.5));
    let arrival = rig.arrival();
    assert_eq!(arrival.ready.len(), 1);
    assert_eq!(
        (arrival.ready[0].0, arrival.ready[0].1.frames()),
        (ticket, 1600)
    );
    assert_eq!(rig.reads(), ["beep.wav"]);
}

#[test]
fn a_file_that_is_not_there_is_told_once_and_may_be_there_later() {
    let rig = Rig::new(vec![], SoundLimits::default());
    let first = pending(rig.sounds.request("gone.wav", 1.0));
    let arrival = rig.arrival();
    assert_eq!(arrival.failed, [first]);
    assert_eq!(arrival.diagnostics.len(), 1);
    assert_eq!(arrival.diagnostics[0].path, "gone.wav");
    assert_eq!(
        arrival.diagnostics[0].to_string(),
        "sound \"gone.wav\" is not a file of the view's bundle"
    );
    // Asked again, it is looked for again (a file the person gives may come), and not told again.
    let second = pending(rig.sounds.request("gone.wav", 1.0));
    let arrival = rig.arrival();
    assert_eq!(arrival.failed, [second]);
    assert!(arrival.diagnostics.is_empty());
    assert_eq!(rig.reads(), ["gone.wav", "gone.wav"]);
}

#[test]
fn a_file_that_is_no_sound_is_told_once_and_is_not_read_again() {
    let rig = Rig::new(
        vec![("page.txt", b"hello, this is no sound".to_vec())],
        SoundLimits::default(),
    );
    let ticket = pending(rig.sounds.request("page.txt", 1.0));
    let arrival = rig.arrival();
    assert_eq!(arrival.failed, [ticket]);
    assert_eq!(
        arrival
            .diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["sound \"page.txt\" is not a WAV or an Ogg Vorbis file"]
    );
    assert!(matches!(rig.sounds.request("page.txt", 1.0), Ask::Refused));
    assert!(rig.sounds.poll().is_empty());
    assert_eq!(rig.reads(), ["page.txt"]);
}

#[test]
fn a_file_past_what_a_file_may_be_is_refused_by_its_size() {
    let limits = SoundLimits {
        file_bytes: 100,
        ..SoundLimits::default()
    };
    let rig = Rig::new(
        vec![("big.wav", beep(800)), ("small.wav", beep(10))],
        limits,
    );
    pending(rig.sounds.request("big.wav", 1.0));
    let arrival = rig.arrival();
    assert_eq!(arrival.diagnostics.len(), 1);
    assert!(
        arrival.diagnostics[0]
            .message
            .contains("past the 100 bytes a file may be"),
        "{}",
        arrival.diagnostics[0]
    );
    pending(rig.sounds.request("small.wav", 1.0));
    assert_eq!(rig.arrival().ready.len(), 1);
    // The default is the most any file of a bundle may be.
    assert_eq!(SoundLimits::default().file_bytes, 8 << 20);
    assert_eq!(SoundLimits::default().decoded_bytes, 64 << 20);
    assert_eq!(SoundLimits::default().paths, 1024);
    assert_eq!(SoundLimits::default().wait, Duration::from_secs(1));
}

#[test]
fn a_view_names_so_many_files_and_no_more() {
    let limits = SoundLimits {
        paths: 2,
        ..SoundLimits::default()
    };
    let rig = Rig::new(
        vec![
            ("a.wav", beep(10)),
            ("b.wav", beep(10)),
            ("c.wav", beep(10)),
        ],
        limits,
    );
    pending(rig.sounds.request("a.wav", 1.0));
    pending(rig.sounds.request("b.wav", 1.0));
    assert_eq!(rig.ready(2).len(), 2);
    assert!(matches!(rig.sounds.request("c.wav", 1.0), Ask::Refused));
    let arrival = rig.sounds.poll();
    assert_eq!(arrival.diagnostics.len(), 1);
    assert!(
        arrival.diagnostics[0]
            .message
            .contains("past the 2 sounds a view may name")
    );
    // Told once, and the two it has are still its own.
    assert!(matches!(rig.sounds.request("c.wav", 1.0), Ask::Refused));
    assert!(rig.sounds.poll().is_empty());
    assert!(matches!(rig.sounds.request("a.wav", 1.0), Ask::Ready(_)));
    assert!(!rig.reads().contains(&"c.wav".to_owned()));
}

#[test]
fn the_sounds_a_view_holds_are_kept_to_what_it_may_and_the_least_used_goes_first() {
    // Room for two of the three: 400 frames of 16 bits are 1600 bytes as floats.
    let limits = SoundLimits {
        decoded_bytes: 3300,
        ..SoundLimits::default()
    };
    let rig = Rig::new(
        vec![
            ("a.wav", beep(400)),
            ("b.wav", beep(400)),
            ("c.wav", beep(400)),
        ],
        limits,
    );
    for name in ["a.wav", "b.wav"] {
        pending(rig.sounds.request(name, 1.0));
        rig.arrival();
    }
    assert_eq!(rig.sounds.held(), 3200);
    // `a` is used, so `b` is the one that goes when `c` comes.
    assert!(matches!(rig.sounds.request("a.wav", 1.0), Ask::Ready(_)));
    pending(rig.sounds.request("c.wav", 1.0));
    rig.arrival();
    assert_eq!(rig.sounds.held(), 3200);
    assert!(matches!(rig.sounds.request("a.wav", 1.0), Ask::Ready(_)));
    assert!(matches!(rig.sounds.request("c.wav", 1.0), Ask::Ready(_)));
    // `b` has to be read again, and goes to the host as any.
    pending(rig.sounds.request("b.wav", 1.0));
    assert_eq!(rig.arrival().ready.len(), 1);
    assert_eq!(rig.reads(), ["a.wav", "b.wav", "c.wav", "b.wav"]);
    assert!(rig.sounds.held() <= 3300);
}

/// Copies of one file made at other pitches are copies of their own: what waits to be taken is held
/// to what a view may hold, so that a page cannot make more by asking again at another pitch.
#[test]
fn the_copies_made_at_a_pitch_are_held_to_what_a_view_may_hold_until_they_are_taken() {
    // 400 frames of 4 bytes is 1600 bytes; the view may hold 4000.
    let limits = SoundLimits {
        decoded_bytes: 4000,
        ..SoundLimits::default()
    };
    let rig = Rig::new(vec![("a.wav", beep(400))], limits);
    for pitch in [1.01, 1.02, 1.03, 1.04] {
        pending(rig.sounds.request("a.wav", pitch));
    }
    // Nothing is taken until the four are made or refused: the first read decodes it (held, 1600
    // bytes) and each copy is about as much again.
    for _ in 0..4 {
        rig.woke
            .recv_timeout(Duration::from_secs(20))
            .expect("the host is woken");
    }
    let arrival = rig.sounds.poll();
    assert_eq!(arrival.ready.len(), 2, "{arrival:?}");
    assert_eq!(arrival.failed.len(), 2, "{arrival:?}");
    assert_eq!(arrival.diagnostics.len(), 1, "{arrival:?}");
    assert!(arrival.diagnostics[0].message.contains("may hold"));
    // What was taken is let go of: a copy is made again.
    pending(rig.sounds.request("a.wav", 1.05));
    assert_eq!(rig.ready(1).len(), 1);
}

#[test]
fn a_sound_that_is_by_itself_more_than_a_view_may_hold_is_refused() {
    let limits = SoundLimits {
        decoded_bytes: 1000,
        ..SoundLimits::default()
    };
    let rig = Rig::new(
        vec![("long.wav", beep(400)), ("short.wav", beep(200))],
        limits,
    );
    pending(rig.sounds.request("long.wav", 1.0));
    let arrival = rig.arrival();
    assert!(
        arrival.diagnostics[0]
            .message
            .contains("past the 0 MiB a view may hold"),
        "{}",
        arrival.diagnostics[0]
    );
    // And at a pitch that makes it more than it may be, though the file itself fits.
    pending(rig.sounds.request("short.wav", 0.5));
    let arrival = rig.arrival();
    assert_eq!(arrival.failed.len(), 1);
    assert_eq!(arrival.diagnostics.len(), 1);
    assert!(matches!(
        rig.sounds.request("short.wav", 1.0),
        Ask::Ready(_)
    ));
}

/// A file that is late and then turns out to be no sound is told to be what it is, as well as late:
/// the author is not left to ask again for what will never play.
#[test]
fn a_file_that_is_late_and_then_is_no_sound_is_told_to_be_late_and_then_what_it_is() {
    let gate = Arc::new(Gate::default());
    let limits = SoundLimits {
        wait: Duration::from_millis(40),
        ..SoundLimits::default()
    };
    let rig = Rig::with(
        vec![("bad.wav", b"not a sound".to_vec())],
        limits,
        Some(Arc::clone(&gate)),
        None,
    );
    pending(rig.sounds.request("bad.wav", 1.0));
    std::thread::sleep(Duration::from_millis(120));
    let late = rig.sounds.poll();
    assert_eq!(late.diagnostics.len(), 1);
    assert!(
        late.diagnostics[0]
            .message
            .contains("was not decoded in 40 ms")
    );
    gate.open();
    let after = rig.arrival();
    assert_eq!(after.diagnostics.len(), 1, "{after:?}");
    assert!(
        after.diagnostics[0]
            .message
            .contains("is not a WAV or an Ogg Vorbis file"),
        "{}",
        after.diagnostics[0]
    );
    // And each once: asked again, it is refused and nothing more is said.
    assert!(matches!(rig.sounds.request("bad.wav", 1.0), Ask::Refused));
    assert!(rig.sounds.poll().diagnostics.is_empty());
}

/// A request the page no longer wants is not waited for, not answered and not told to be late.
#[test]
fn a_request_that_is_cancelled_is_not_waited_for_answered_or_told_late() {
    let gate = Arc::new(Gate::default());
    let limits = SoundLimits {
        wait: Duration::from_millis(40),
        ..SoundLimits::default()
    };
    let rig = Rig::with(
        vec![("slow.wav", beep(100))],
        limits,
        Some(Arc::clone(&gate)),
        None,
    );
    let ticket = pending(rig.sounds.request("slow.wav", 1.0));
    let other = pending(rig.sounds.request("slow.wav", 0.5));
    rig.sounds.cancel(&[ticket]);
    // The other is still waited for; the cancelled one is gone.
    assert!(rig.sounds.deadline().is_some());
    rig.sounds.cancel(&[other]);
    assert!(rig.sounds.deadline().is_none());
    std::thread::sleep(Duration::from_millis(120));
    assert!(rig.sounds.poll().is_empty());
    gate.open();
    rig.woke
        .recv_timeout(Duration::from_secs(20))
        .expect("woken");
    rig.woke
        .recv_timeout(Duration::from_secs(20))
        .expect("woken");
    let after = rig.sounds.poll();
    assert!(after.is_empty(), "{after:?}");
    // What was decoded is kept for the next ask.
    assert!(matches!(rig.sounds.request("slow.wav", 1.0), Ask::Ready(_)));
}

#[test]
fn a_sound_not_decoded_in_the_time_a_page_may_wait_is_dropped_and_the_author_told() {
    let gate = Arc::new(Gate::default());
    let limits = SoundLimits {
        wait: Duration::from_millis(40),
        ..SoundLimits::default()
    };
    let rig = Rig::with(
        vec![("slow.wav", beep(100))],
        limits,
        Some(Arc::clone(&gate)),
        None,
    );
    let ticket = pending(rig.sounds.request("slow.wav", 1.0));
    assert!(rig.sounds.poll().is_empty(), "not yet late");
    std::thread::sleep(Duration::from_millis(120));
    let late = rig.sounds.poll();
    assert_eq!(late.failed, [ticket]);
    assert_eq!(late.diagnostics.len(), 1);
    assert_eq!(late.diagnostics[0].path, "slow.wav");
    assert!(
        late.diagnostics[0]
            .message
            .contains("was not decoded in 40 ms"),
        "{}",
        late.diagnostics[0]
    );
    assert!(rig.sounds.deadline().is_none());
    // The door opens: what was decoded is kept, and is not handed to what is gone.
    gate.open();
    rig.woke
        .recv_timeout(Duration::from_secs(20))
        .expect("woken");
    let after = rig.sounds.poll();
    assert!(
        after.ready.is_empty() && after.failed.is_empty(),
        "{after:?}"
    );
    assert!(matches!(rig.sounds.request("slow.wav", 1.0), Ask::Ready(_)));
}

#[test]
fn a_read_that_gives_up_costs_the_sound_and_not_the_thread() {
    let rig = Rig::with(
        vec![("fine.wav", beep(100)), ("bad.wav", beep(100))],
        SoundLimits::default(),
        None,
        Some("bad.wav"),
    );
    let ticket = pending(rig.sounds.request("bad.wav", 1.0));
    let arrival = rig.arrival();
    assert_eq!(arrival.failed, [ticket]);
    assert_eq!(arrival.diagnostics.len(), 1);
    assert!(
        arrival.diagnostics[0]
            .message
            .contains("could not be decoded"),
        "{}",
        arrival.diagnostics[0]
    );
    pending(rig.sounds.request("fine.wav", 1.0));
    assert_eq!(rig.arrival().ready.len(), 1);
}

#[test]
fn a_wav_of_the_most_a_file_may_be_with_a_header_that_lies_is_not_made() {
    // A file of the size a file may be whose header says it has more samples than a view may hold:
    // refused by the header, before any room is made for it.
    let mut lying = wav_ints(8000, 1, 16, &[1, 2, 3, 4]);
    let size: u32 = 40 << 20;
    lying[40..44].copy_from_slice(&size.to_le_bytes());
    let rig = Rig::new(vec![("lie.wav", lying)], SoundLimits::default());
    pending(rig.sounds.request("lie.wav", 1.0));
    let arrival = rig.arrival();
    assert!(
        arrival.diagnostics[0].message.contains("MiB of samples"),
        "{}",
        arrival.diagnostics[0]
    );
}

// --- What a play of a palette sound is -----------------------------------------------------------

#[test]
fn a_play_of_a_sound_that_does_not_vary_is_the_palettes_own_and_at_a_pitch_it_is_that_much_faster()
{
    let own = palette("success").unwrap();
    assert!(Arc::ptr_eq(&play_of("success", 1.0, 0).unwrap(), &own));
    assert!(Arc::ptr_eq(&play_of("success", 1.0, 99).unwrap(), &own));
    let fast = play_of("success", 2.0, 0).unwrap();
    assert_eq!(fast.rate(), own.rate());
    assert!(fast.frames().abs_diff(own.frames() / 2) <= 1);
    let slow = play_of("success", 0.5, 0).unwrap();
    assert!(slow.frames().abs_diff(own.frames() * 2) <= 1);
    assert!(play_of("nope", 1.0, 0).is_none());
}

#[test]
fn the_plays_of_a_sound_that_varies_are_different_the_same_play_is_the_same_and_none_is_louder_than_a_sound_may_be()
 {
    let first = play_of("tap", 1.0, 0).unwrap();
    assert_eq!(first, play_of("tap", 1.0, 0).unwrap());
    assert_ne!(first, play_of("tap", 1.0, 1).unwrap());
    for draw in 0..200 {
        let play = play_of("tap", 1.0, draw).unwrap();
        assert!(play.peak() <= 1.0, "{draw}");
        // Within what a person does not hear as another sound: a few percent of the length.
        let own = palette("tap").unwrap();
        let ratio = play.frames() as f64 / own.frames() as f64;
        assert!((0.98..=1.02).contains(&ratio), "{draw}: {ratio}");
    }
}
