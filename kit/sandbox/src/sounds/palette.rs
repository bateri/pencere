//! The kit's own sounds: twenty, named for the interface job each does, made by synthesis
//! ([`super::synth`]) and not read from a file, so that they are the same on every system, take no
//! room in a bundle and need no decoder.
//!
//! The materials (struck bars, switch clicks, air, a small room) and the recipes of the sounds
//! that have one there are those of Cuelume (MIT, Copyright (c) 2026 Daniel Belyi, commit
//! `e1efd89`); the names, the lengths and the levels of the layers of a sound are those of uisfx
//! (MIT, Copyright (c) 2026 Yuki Capital, commit `9950fe6`): see `NOTICE`. The names are the
//! kit's interface and stay; a recipe is a way of making what the name says and may change.
//!
//! Each is made once per process, the first time it is asked for.

use std::io::Cursor;
use std::sync::{Arc, OnceLock};

use pencere_view::sound::NAMES;

use super::Sound;
use super::synth::{
    BAR, Layer, MALLET, Material, Recipe, air, click, contact, knock, render, shifted, struck, thud,
};

/// How loud a sound is at its largest, by what it is for: what a person does is quiet, what
/// happened is heard, and what needs the person is heard most. Every tier is under -1 dBFS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tier {
    Input,
    Result,
    Alert,
}

impl Tier {
    /// The largest sample of a sound of this tier.
    pub(crate) fn peak(self) -> f64 {
        match self {
            Self::Input => 0.30,
            Self::Result => 0.42,
            Self::Alert => 0.52,
        }
    }
}

/// What a name says of its sound besides how it is made.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Spec {
    pub(crate) name: &'static str,
    /// How long it plays, in seconds: the length of the same job's cue in uisfx.
    pub(crate) seconds: f64,
    pub(crate) tier: Tier,
    /// Whether it is played so often that every play is a little different (a tap is, a success is
    /// the same each time).
    pub(crate) varies: bool,
}

const fn spec(name: &'static str, seconds: f64, tier: Tier, varies: bool) -> Spec {
    Spec {
        name,
        seconds,
        tier,
        varies,
    }
}

/// The sounds, in the order of [`NAMES`].
pub(crate) const SPECS: [Spec; 20] = [
    spec("tap", 0.16, Tier::Input, true),
    spec("select", 0.28, Tier::Input, true),
    spec("toggle_on", 0.30, Tier::Input, true),
    spec("toggle_off", 0.29, Tier::Input, true),
    spec("type", 0.045, Tier::Input, true),
    spec("open", 0.37, Tier::Input, false),
    spec("close", 0.34, Tier::Input, false),
    spec("snap", 0.16, Tier::Input, true),
    spec("copy", 0.30, Tier::Input, false),
    spec("delete", 0.48, Tier::Result, false),
    spec("success", 0.72, Tier::Result, false),
    spec("warning", 0.68, Tier::Alert, false),
    spec("error", 0.62, Tier::Alert, false),
    spec("send", 0.42, Tier::Result, false),
    spec("receive", 0.47, Tier::Result, false),
    spec("notify", 0.58, Tier::Result, false),
    spec("attention", 0.64, Tier::Alert, false),
    spec("start", 0.46, Tier::Result, false),
    spec("complete", 0.80, Tier::Result, false),
    spec("celebrate", 1.16, Tier::Result, false),
];

/// The sound a name is, if the kit has one by it.
pub(crate) fn spec_of(name: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.name == name)
}

/// How much more than the faint send of every sound a result that landed rings in the room: it
/// lands in the space without sounding far.
const LANDED: f64 = 2.0;

fn plain(layers: Vec<Layer>) -> Recipe {
    Recipe { layers, room: 1.0 }
}

fn landed(layers: Vec<Layer>) -> Recipe {
    Recipe {
        layers,
        room: LANDED,
    }
}

/// Struck notes of one material, each (when it starts, its frequency, its decay, its peak).
fn notes(material: Material, parts: &[(f64, f64, f64, f64)]) -> Vec<Layer> {
    parts
        .iter()
        .flat_map(|&(at, frequency, decay, peak)| {
            shifted(struck(frequency, decay, peak, material), at)
        })
        .collect()
}

/// A mallet's touch at each of the times, in front of the notes struck at them.
fn touches(hardness: f64, peak: f64, times: &[f64]) -> Vec<Layer> {
    times
        .iter()
        .map(|&at| contact(hardness, peak).at(at))
        .collect()
}

fn recipe(name: &str) -> Option<Recipe> {
    Some(match name {
        // A small glassy tap: a nail's tick, a soft knock of body, then the glass ringing briefly,
        // its fundamental two near modes that beat slowly, as real glass shimmers.
        "tap" => plain(vec![
            Layer::band(4500.0, 1.2, 0.001, 0.00242, 0.03),
            Layer::tone(1174.66, 0.001, 0.18, 0.0135),
            Layer::tone(1180.0, 0.001, 0.164, 0.008),
            Layer::tone(3242.0, 0.001, 0.0827, 0.0055),
            knock(300.0, 0.0261, 0.02),
        ]),
        // A crisp detent over a small wooden knock.
        "select" => plain(vec![
            Layer::band(2800.0, 2.2, 0.001, 0.00749, 0.16),
            knock(415.0, 0.0194, 0.03),
        ]),
        // A switch snapping over: one crisp click with a small knock of body.
        "toggle_on" => plain(vec![
            Layer::band(1830.0, 1.6, 0.001, 0.0156, 0.12),
            knock(330.0, 0.0339, 0.045),
            Layer::band(3150.0, 1.6, 0.001, 0.0124, 0.08),
        ]),
        // Switching off knocks upward.
        "toggle_off" => plain(vec![
            Layer::band(2200.0, 1.6, 0.001, 0.0156, 0.12),
            knock(415.0, 0.0339, 0.045),
            Layer::band(3780.0, 1.6, 0.001, 0.0124, 0.08),
        ]),
        // One keystroke: the switch's click, the keycap's clack and the thock of bottoming out.
        "type" => plain(vec![
            Layer::band(3900.0, 0.8, 0.001, 0.00517, 0.08),
            Layer::band(1550.0, 3.5, 0.001, 0.0144, 0.22),
            Layer::band(310.0, 2.5, 0.002, 0.0254, 0.35),
        ]),
        // Air drawing upward with a light note swelling inside it: one breath as a panel opens.
        "open" => plain(vec![
            Layer::band(600.0, 1.4, 0.05, 0.0558, 0.168).gliding(1500.0, 0.1),
            Layer::tone(659.25, 0.05, 0.165, 0.015),
            Layer::tone(1318.51, 0.05, 0.0883, 0.005),
        ]),
        // The same air falling shut over a low, damped note.
        "close" => plain(vec![
            Layer::band(1400.0, 1.4, 0.02, 0.054, 0.216).gliding(540.0, 0.07),
            Layer::tone(261.63, 0.003, 0.104, 0.02),
            Layer::band(350.0, 2.0, 0.001, 0.0273, 0.2),
        ]),
        // An object locks into its place: a click, a knock and a little latch ringing.
        "snap" => plain(
            [
                click(3600.0, 0.12),
                vec![
                    knock(260.0, 0.03, 0.04),
                    Layer::tone(2093.0, 0.001, 0.05, 0.006),
                ],
            ]
            .concat(),
        ),
        // Two glass taps, the second an octave up and softer: what was said is said again.
        "copy" => plain(
            [
                vec![Layer::band(3500.0, 1.5, 0.001, 0.006, 0.06)],
                notes(BAR, &[(0.0, 1046.5, 0.14, 0.02), (0.1, 2093.0, 0.12, 0.01)]),
            ]
            .concat(),
        ),
        // Air going out, a dull thud and a low mallet note: something is gone.
        "delete" => plain(
            [
                vec![
                    air(1400.0, 450.0, 0.12, 0.15),
                    thud(220.0, 0.09, 0.25).at(0.02),
                ],
                touches(700.0, 0.04, &[0.05]),
                notes(MALLET, &[(0.05, 146.83, 0.2, 0.03)]),
            ]
            .concat(),
        ),
        // One soft mallet chord, C, G and E spread wide so no two notes rub, struck with felt.
        "success" => landed(
            [
                vec![contact(1500.0, 0.04)],
                notes(
                    MALLET,
                    &[
                        (0.0, 523.25, 0.333, 0.022),
                        (0.0, 783.99, 0.391, 0.02),
                        (0.0, 1318.51, 0.335, 0.014),
                    ],
                ),
            ]
            .concat(),
        ),
        // One mallet fifth, A and E, neither bright nor low: between success and error.
        "warning" => plain(
            [
                vec![contact(1200.0, 0.04)],
                notes(
                    MALLET,
                    &[(0.0, 440.0, 0.163, 0.038), (0.0, 659.25, 0.145, 0.03)],
                ),
            ]
            .concat(),
        ),
        // One muted low mallet chord, D with the F a tenth above, short: a calm, recoverable refusal.
        "error" => plain(
            [
                vec![contact(900.0, 0.05)],
                notes(
                    MALLET,
                    &[(0.0, 293.66, 0.138, 0.04), (0.0, 698.46, 0.106, 0.018)],
                ),
            ]
            .concat(),
        ),
        // A breath of air rising under two mallet notes going up: what was said is on its way.
        "send" => plain(
            [
                vec![air(500.0, 1800.0, 0.16, 0.15)],
                touches(1200.0, 0.02, &[0.04, 0.17]),
                notes(
                    MALLET,
                    &[(0.04, 523.25, 0.18, 0.02), (0.17, 1046.5, 0.2, 0.014)],
                ),
            ]
            .concat(),
        ),
        // Three soft glass notes coming down, with a little air going out of them: it arrived.
        "receive" => plain(
            [
                vec![air(1600.0, 700.0, 0.12, 0.06).at(0.1)],
                notes(
                    BAR,
                    &[
                        (0.0, 1174.66, 0.16, 0.012),
                        (0.1, 880.0, 0.24, 0.02),
                        (0.2, 659.25, 0.28, 0.016),
                    ],
                ),
            ]
            .concat(),
        ),
        // Two glass notes a fourth apart: there is something, and no hurry.
        "notify" => plain(
            [
                touches(2000.0, 0.02, &[0.0, 0.19]),
                notes(
                    BAR,
                    &[(0.0, 880.0, 0.5, 0.02), (0.19, 1174.66, 0.45, 0.018)],
                ),
            ]
            .concat(),
        ),
        // One high glass bell, A and E struck together, ringing longest. The A's twin sits 2 Hz
        // sharp, so that the bell swells once rather than warbling.
        "attention" => plain(
            [
                vec![contact(2500.0, 0.02), Layer::tone(882.0, 0.003, 0.57, 0.01)],
                notes(
                    BAR,
                    &[(0.0, 880.0, 0.599, 0.018), (0.0, 1318.51, 0.559, 0.014)],
                ),
            ]
            .concat(),
        ),
        // A little air going up under three mallet notes going up: a process begins.
        "start" => plain(
            [
                vec![air(400.0, 900.0, 0.1, 0.09)],
                touches(1000.0, 0.03, &[0.0, 0.1, 0.2]),
                notes(
                    MALLET,
                    &[
                        (0.0, 196.0, 0.2, 0.03),
                        (0.1, 261.63, 0.2, 0.028),
                        (0.2, 329.63, 0.22, 0.026),
                    ],
                ),
            ]
            .concat(),
        ),
        // Three glass notes going up and the last left to ring: the last step is done.
        "complete" => landed(
            [
                touches(1500.0, 0.02, &[0.0, 0.13, 0.26]),
                vec![Layer::tone(393.2, 0.003, 0.3, 0.012)],
                notes(
                    BAR,
                    &[
                        (0.0, 392.0, 0.3, 0.026),
                        (0.13, 587.33, 0.3, 0.02),
                        (0.26, 783.99, 0.34, 0.018),
                    ],
                ),
            ]
            .concat(),
        ),
        // A run of glass notes up the C major chord, each ringing into the next.
        "celebrate" => landed(
            [
                touches(2500.0, 0.03, &[0.0, 0.12, 0.24, 0.36, 0.48]),
                notes(
                    BAR,
                    &[
                        (0.0, 523.25, 0.4, 0.02),
                        (0.12, 659.25, 0.4, 0.02),
                        (0.24, 783.99, 0.4, 0.02),
                        (0.36, 1046.5, 0.4, 0.02),
                        (0.48, 1318.51, 0.45, 0.014),
                    ],
                ),
            ]
            .concat(),
        ),
        _ => return None,
    })
}

/// What a name seeds the noise of its sound with, so that no two sounds hiss alike.
fn seed_of(name: &str) -> u64 {
    name.bytes().fold(0xCBF2_9CE4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01B3)
    })
}

pub(crate) fn make(spec: &Spec) -> Option<Arc<Sound>> {
    let recipe = recipe(spec.name)?;
    render(&recipe, spec.seconds, spec.tier.peak(), seed_of(spec.name)).map(Arc::new)
}

/// The palette's sound by this name, or `None` where the kit has none. Made the first time it is
/// asked for and kept: it is the same sound, the same samples, every time and on every system.
#[must_use]
pub fn palette(name: &str) -> Option<Arc<Sound>> {
    static MADE: [OnceLock<Option<Arc<Sound>>>; 20] = [const { OnceLock::new() }; 20];
    let index = NAMES.iter().position(|&known| known == name)?;
    let spec = SPECS.get(index).filter(|spec| spec.name == name)?;
    MADE.get(index)?.get_or_init(|| make(spec)).clone()
}

/// How much one play of a sound that is played often differs from the last.
const PITCH_SPREAD: f32 = 0.012;
const LEVEL_SPREAD: f32 = 0.12;

/// What makes one play of a sound that is played often a little different from the others, as a
/// real key is never struck the same way twice: a pitch (the ratio to play it at, within 1.2 % of
/// one) and a level (the volume to multiply, within 12 % of one).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variation {
    pub pitch: f32,
    pub level: f32,
}

/// The variation of the play numbered `draw` of the sound by this name, for a name whose sound
/// varies (`tap`, `select`, `type`, `snap` and the toggles); `None` for the others, which are the
/// same every time. The same `draw` gives the same variation.
#[must_use]
pub fn variation(name: &str, draw: u64) -> Option<Variation> {
    if !spec_of(name)?.varies {
        return None;
    }
    // SplitMix64, twice: one number for the pitch and one for the level.
    let next = |state: u64| {
        let mut z = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let unit = |bits: u64| (bits >> 40) as f32 / (1u64 << 23) as f32 - 1.0;
    let first = next(draw);
    let second = next(first);
    Some(Variation {
        pitch: 1.0 + PITCH_SPREAD * unit(first),
        level: 1.0 + LEVEL_SPREAD * unit(second),
    })
}

/// The palette's sound by this name for the play numbered `draw`, `pitch` times as fast as the page
/// asks (from 0.5 to 2): the samples a host plays, with what makes the plays of a sound played often
/// differ a little ([`variation`]) in them, so that the host multiplies them by the page's volume
/// and nothing else. The same name, pitch and draw give the same samples. `None` where the kit has no
/// sound by the name.
#[must_use]
pub fn play_of(name: &str, pitch: f32, draw: u64) -> Option<Arc<Sound>> {
    let sound = palette(name)?;
    let (ratio, level) = match variation(name, draw) {
        Some(Variation {
            pitch: drift,
            level,
        }) => (pitch * drift, level),
        None => (pitch, 1.0),
    };
    if (ratio - 1.0).abs() < f32::EPSILON && (level - 1.0).abs() < f32::EPSILON {
        return Some(sound);
    }
    let pitched = sound.pitched(ratio);
    let samples = pitched
        .samples()
        .iter()
        .map(|sample| (sample * level).clamp(-1.0, 1.0))
        .collect();
    Sound::new(pitched.rate(), pitched.channels(), samples).map(Arc::new)
}

/// The sound as a WAV file of 16-bit samples: what a person may listen to in any player.
#[must_use]
pub fn wav(sound: &Sound) -> Option<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: sound.channels(),
        sample_rate: sound.rate(),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut out = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut out, spec).ok()?;
        for &sample in sound.samples() {
            let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            writer.write_sample(value).ok()?;
        }
        writer.finalize().ok()?;
    }
    Some(out.into_inner())
}

/// Every sound of the palette, with its name, as a WAV file: what `pencere sounds` writes.
#[must_use]
pub fn palette_wavs() -> Vec<(&'static str, Vec<u8>)> {
    NAMES
        .iter()
        .filter_map(|&name| {
            let sound = palette(name)?;
            Some((name, wav(&sound)?))
        })
        .collect()
}
