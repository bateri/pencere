//! The synthesis the kit's palette is made with: layers of one sine tone or one burst of filtered
//! noise, each shaped by an attack and a decay, summed, put in a small room, and finished so that
//! it starts and ends in silence.
//!
//! The model of the materials is that of Cuelume (MIT, Copyright (c) 2026 Daniel Belyi): a struck
//! bar is a few sine modes at the bar's own ratios, a knock is a sine that drops to its pitch in
//! 18 ms, a click is a narrow ring over a wider body, air is noise whose filter moves. The ways of
//! making them are written here again in Rust, with the same constants; see `NOTICE`.
//!
//! Everything is made in 64-bit floats from a seed, with no clock and no system random, so that a
//! sound is the same on every machine that makes it.

use std::f64::consts::{PI, TAU};

use super::Sound;

/// The rate the palette is made at.
pub(crate) const RATE: u32 = 48_000;

/// How far a sound's end is faded, at most: the last 28 ms.
const FADE: f64 = 0.028;

/// What a layer is.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Source {
    /// A sine tone at `frequency` Hz.
    Tone { frequency: f64 },
    /// Noise through a lowpass filter whose corner is at `frequency`; `q_db` is its resonance in
    /// decibels (a flat corner is -3).
    Low { frequency: f64, q_db: f64 },
    /// Noise through a bandpass filter centred on `frequency`, `q` (linear) narrow.
    Band { frequency: f64, q: f64 },
}

/// One tone or one burst of filtered noise, with the way it swells and dies.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layer {
    pub source: Source,
    /// Seconds after the sound starts that this layer does.
    pub offset: f64,
    /// Seconds it takes to reach `peak`, rising straight from silence.
    pub attack: f64,
    /// Seconds it takes to fall 60 dB once the attack ends.
    pub decay: f64,
    /// The level it reaches at the end of the attack.
    pub peak: f64,
    /// Where its pitch (a tone's frequency, a noise's filter) goes, and in how many seconds: the way
    /// there is exponential, as a pitch is heard.
    pub glide: Option<(f64, f64)>,
}

impl Layer {
    pub(crate) fn tone(frequency: f64, attack: f64, decay: f64, peak: f64) -> Self {
        Self {
            source: Source::Tone { frequency },
            offset: 0.0,
            attack,
            decay,
            peak,
            glide: None,
        }
    }

    pub(crate) fn band(frequency: f64, q: f64, attack: f64, decay: f64, peak: f64) -> Self {
        Self {
            source: Source::Band { frequency, q },
            offset: 0.0,
            attack,
            decay,
            peak,
            glide: None,
        }
    }

    pub(crate) fn low(frequency: f64, q_db: f64, attack: f64, decay: f64, peak: f64) -> Self {
        Self {
            source: Source::Low { frequency, q_db },
            offset: 0.0,
            attack,
            decay,
            peak,
            glide: None,
        }
    }

    /// The same layer, its pitch going to `to` in `seconds`.
    pub(crate) fn gliding(self, to: f64, seconds: f64) -> Self {
        Self {
            glide: Some((to, seconds)),
            ..self
        }
    }

    /// The same layer, starting `seconds` later.
    pub(crate) fn at(self, seconds: f64) -> Self {
        Self {
            offset: self.offset + seconds,
            ..self
        }
    }
}

/// The same layers, starting `seconds` later.
pub(crate) fn shifted(layers: Vec<Layer>, seconds: f64) -> Vec<Layer> {
    layers.into_iter().map(|layer| layer.at(seconds)).collect()
}

/// What a sound is made of: its layers, and how much of it rings in the room (a multiple of the
/// faint send every sound has).
#[derive(Clone, Debug)]
pub(crate) struct Recipe {
    pub layers: Vec<Layer>,
    pub room: f64,
}

// --- The materials ------------------------------------------------------------------------------

/// How far sharp a knock is struck, and how fast it drops to its pitch.
const KNOCK_DROP: f64 = 1.6;
const KNOCK_DROP_TIME: f64 = 0.018;

/// A knock's body: a sine struck sharp that drops to `frequency` in 18 ms, the way a small hollow
/// part answers a tap. Heard as a soft "tok", never as a slide.
pub(crate) fn knock(frequency: f64, decay: f64, peak: f64) -> Layer {
    Layer::tone(frequency * KNOCK_DROP, 0.001, decay, peak).gliding(frequency, KNOCK_DROP_TIME)
}

/// A struck object's modes, as (ratio to the fundamental, level): real bars are inharmonic, and
/// that is what tells the ear a bar from a synthesiser.
pub(crate) type Material = &'static [(f64, f64)];

/// A free bar, glass or metal: modes at 1, 2.76 and 5.40 times the fundamental.
pub(crate) const BAR: Material = &[(1.0, 1.0), (2.76, 0.3), (5.4, 0.1)];
/// A marimba bar, undercut so its second mode sits at 3.99 times: rounder than a free bar.
pub(crate) const MALLET: Material = &[(1.0, 1.0), (3.99, 0.2)];

/// Modes above this are left out: the register ceiling the palette keeps.
const MODE_CEILING: f64 = 5000.0;
/// A struck tone swells in over 3 ms: short enough to strike, long enough not to click.
const STRIKE_ATTACK: f64 = 0.003;
/// No mode dies faster than this, or it stops being heard as a pitch.
const SHORTEST_MODE: f64 = 0.028;
const MODE_DAMPING: f64 = 0.75;

/// A struck note: one sine for each mode of `material`, a higher mode dying sooner, as on any real
/// bar.
pub(crate) fn struck(frequency: f64, decay: f64, peak: f64, material: Material) -> Vec<Layer> {
    material
        .iter()
        .filter(|(ratio, _)| frequency * ratio <= MODE_CEILING)
        .map(|&(ratio, level)| {
            Layer::tone(
                frequency * ratio,
                STRIKE_ATTACK,
                (decay / ratio.powf(MODE_DAMPING)).max(SHORTEST_MODE),
                peak * level,
            )
        })
        .collect()
}

/// A lowpass Q, in decibels, that gives a flat, unresonant corner.
pub(crate) const FLAT_Q: f64 = -3.0;

/// The mallet meeting the bar: 3 ms of noise, low-passed at the mallet's hardness (a soft felt
/// mallet sits near 1 kHz, a hard one near 3 kHz). It makes a chord a struck object rather than a
/// test tone.
pub(crate) fn contact(hardness: f64, peak: f64) -> Layer {
    Layer::low(hardness, FLAT_Q, 0.001, 0.0035, peak)
}

/// One resonance of a switch's click: a narrow ring that dies in about 5 ms.
pub(crate) fn ring(frequency: f64, peak: f64) -> Layer {
    Layer::band(frequency, 8.0, 0.001, 0.0055, peak)
}

/// How far below its ring a click's body sits.
const CLICK_BODY: f64 = 0.675;

/// A click: a tight ring over a softer, wider body below it, so that the energy spreads the way a
/// real switch's does.
pub(crate) fn click(frequency: f64, peak: f64) -> Vec<Layer> {
    vec![
        ring(frequency, peak),
        Layer::band(frequency * CLICK_BODY, 2.0, 0.001, 0.0055, peak * 0.5),
    ]
}

/// Air moved by a part: noise whose band sweeps from `from` to `to` Hz over `decay` seconds.
pub(crate) fn air(from: f64, to: f64, decay: f64, peak: f64) -> Layer {
    Layer::band(from, 1.5, 0.008, decay, peak).gliding(to, decay)
}

/// A dull thud: a part hitting its stop.
pub(crate) fn thud(frequency: f64, decay: f64, peak: f64) -> Layer {
    Layer::low(frequency, 0.7, 0.001, decay, peak)
}

// --- Rendering ----------------------------------------------------------------------------------

/// A filter of two poles and two zeros (the coefficients are Robert Bristow-Johnson's).
#[derive(Clone, Copy, Debug, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn band(frequency: f64, q: f64) -> Self {
        let (cos, alpha) = Self::angle(frequency, 2.0 * q.max(0.01));
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    fn low(frequency: f64, q_db: f64) -> Self {
        let (cos, alpha) = Self::angle(frequency, 2.0 * 10f64.powf(q_db / 20.0));
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    fn high(frequency: f64, q: f64) -> Self {
        let (cos, alpha) = Self::angle(frequency, 2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + cos) / 2.0 / a0,
            b1: -(1.0 + cos) / a0,
            b2: (1.0 + cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// The cosine of the filter's frequency and its `alpha` for `two_q` (twice its Q).
    fn angle(frequency: f64, two_q: f64) -> (f64, f64) {
        let top = f64::from(RATE) / 2.0 - 1.0;
        let w = TAU * frequency.clamp(1.0, top) / f64::from(RATE);
        (w.cos(), w.sin() / two_q)
    }
}

/// What a filter holds between samples.
#[derive(Clone, Copy, Debug, Default)]
struct Memory {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Memory {
    fn run(&mut self, k: &Biquad, x: f64) -> f64 {
        let y = k.b0 * x + k.b1 * self.x1 + k.b2 * self.x2 - k.a1 * self.y1 - k.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// White noise from a seed: the same numbers on every machine.
struct Noise(u64);

impl Noise {
    fn new(seed: u64) -> Self {
        // SplitMix64 once, so that a small seed is no weak start.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Self((z ^ (z >> 31)) | 1)
    }

    /// The next number, from -1 up to 1.
    fn next(&mut self) -> f64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let bits = self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        bits as f64 / (1u64 << 52) as f64 - 1.0
    }
}

/// How often a gliding filter's coefficients are made again, in samples.
const REFRESH: usize = 16;

/// The layer's own level at `t` seconds: a straight rise, then 60 dB over `decay`.
fn level(layer: &Layer, t: f64) -> f64 {
    if t < layer.attack {
        layer.peak * t / layer.attack.max(1e-6)
    } else {
        layer.peak * 10f64.powf(-3.0 * (t - layer.attack) / layer.decay.max(1e-6))
    }
}

/// The pitch of `layer` at `t` seconds, from `base`.
fn pitch(layer: &Layer, base: f64, t: f64) -> f64 {
    match layer.glide {
        Some((to, seconds)) => base * (to / base).powf((t / seconds.max(1e-6)).min(1.0)),
        None => base,
    }
}

/// Adds `layer` to `mix`, a sound that is `mix.len()` frames long.
fn add(mix: &mut [f64], layer: &Layer, seed: u64) {
    let rate = f64::from(RATE);
    let start = (layer.offset * rate).round() as usize;
    // Until it is 75 dB down, which is silence.
    let length = ((layer.attack + layer.decay * 1.25 + 0.005) * rate).ceil() as usize;
    let out = mix.iter_mut().skip(start).take(length);
    let mut noise = Noise::new(seed);
    let mut memory = Memory::default();
    let mut phase = 0.0;
    let mut kernel = Biquad::default();
    for (n, slot) in out.enumerate() {
        let t = n as f64 / rate;
        let gain = level(layer, t);
        match layer.source {
            Source::Tone { frequency } => {
                phase += TAU * pitch(layer, frequency, t) / rate;
                *slot += gain * phase.sin();
            }
            Source::Low { frequency, q_db } => {
                if n % REFRESH == 0 && (n == 0 || layer.glide.is_some()) {
                    kernel = Biquad::low(pitch(layer, frequency, t), q_db);
                }
                *slot += gain * memory.run(&kernel, noise.next());
            }
            Source::Band { frequency, q } => {
                if n % REFRESH == 0 && (n == 0 || layer.glide.is_some()) {
                    kernel = Biquad::band(pitch(layer, frequency, t), q);
                }
                *slot += gain * memory.run(&kernel, noise.next());
            }
        }
    }
}

/// A comb filter: a delay that feeds back on itself, losing its highs as it goes.
struct Comb {
    line: Vec<f64>,
    at: usize,
    feedback: f64,
    held: f64,
}

/// How much of the highs a comb loses on each trip round.
const DAMPING: f64 = 0.35;

impl Comb {
    fn new(seconds: f64, reverberation: f64) -> Self {
        Self {
            line: vec![0.0; ((seconds * f64::from(RATE)).round() as usize).max(1)],
            at: 0,
            // 60 dB down in `reverberation` seconds.
            feedback: 10f64.powf(-3.0 * seconds / reverberation),
            held: 0.0,
        }
    }

    fn run(&mut self, x: f64) -> f64 {
        let out = self.line.get(self.at).copied().unwrap_or(0.0);
        self.held = out * (1.0 - DAMPING) + self.held * DAMPING;
        if let Some(slot) = self.line.get_mut(self.at) {
            *slot = x + self.held * self.feedback;
        }
        self.at = (self.at + 1) % self.line.len();
        out
    }
}

/// An allpass filter: a delay that smears an echo without changing its colour.
struct Allpass {
    line: Vec<f64>,
    at: usize,
}

const ALLPASS_FEEDBACK: f64 = 0.6;

impl Allpass {
    fn new(seconds: f64) -> Self {
        Self {
            line: vec![0.0; ((seconds * f64::from(RATE)).round() as usize).max(1)],
            at: 0,
        }
    }

    fn run(&mut self, x: f64) -> f64 {
        let delayed = self.line.get(self.at).copied().unwrap_or(0.0);
        let held = x + ALLPASS_FEEDBACK * delayed;
        if let Some(slot) = self.line.get_mut(self.at) {
            *slot = held;
        }
        self.at = (self.at + 1) % self.line.len();
        delayed - ALLPASS_FEEDBACK * held
    }
}

/// How long the room rings, 60 dB down: short enough to be heard as space and not as echo.
const REVERBERATION: f64 = 0.18;
/// The send every sound has into the room, as a fraction of its own level; a sound's `room` is a
/// multiple of it.
const SEND: f64 = 0.08;

/// What a room makes of `dry` for one ear: the delays of the two ears differ, so that the room
/// has a width.
fn room(dry: &[f64], combs: [f64; 4], allpasses: [f64; 2]) -> Vec<f64> {
    let mut combs = combs.map(|seconds| Comb::new(seconds, REVERBERATION));
    let mut allpasses = allpasses.map(Allpass::new);
    dry.iter()
        .map(|&x| {
            let input = x * 0.25;
            let sum: f64 = combs.iter_mut().map(|comb| comb.run(input)).sum();
            allpasses.iter_mut().fold(sum, |y, pass| pass.run(y))
        })
        .collect()
}

/// A sound made of `recipe`, `seconds` long, stereo, its largest sample `target` (from 0 to 1), from
/// `seed`. It starts and ends in silence: every layer rises from nothing, and the end is faded.
pub(crate) fn render(recipe: &Recipe, seconds: f64, target: f64, seed: u64) -> Option<Sound> {
    let rate = f64::from(RATE);
    let frames = (seconds * rate).round() as usize;
    let mut dry = vec![0.0; frames];
    for (index, layer) in recipe.layers.iter().enumerate() {
        add(
            &mut dry,
            layer,
            seed ^ (index as u64 + 1).wrapping_mul(0xA24B_AED4_963E_E407),
        );
    }
    let send = SEND * recipe.room;
    let wet = [
        room(&dry, [0.0113, 0.0147, 0.0189, 0.0223], [0.0043, 0.0017]),
        room(&dry, [0.0121, 0.0159, 0.0197, 0.0235], [0.0047, 0.0019]),
    ];
    let mut channels: Vec<Vec<f64>> = wet
        .iter()
        .map(|wet| {
            // Under 30 Hz is no sound, and a mix that is not centred on zero is no use to a speaker.
            let kernel = Biquad::high(30.0, 0.707);
            let mut memory = Memory::default();
            dry.iter()
                .zip(wet)
                .map(|(dry, wet)| memory.run(&kernel, dry + wet * send))
                .collect()
        })
        .collect();
    // The end: the last 28 ms, or the last 40 % of a sound shorter than 70 ms, fall to nothing.
    let fade = ((FADE * rate) as usize).min((frames as f64 * 0.4) as usize);
    for channel in &mut channels {
        let from = channel.len().saturating_sub(fade);
        for (n, sample) in channel.iter_mut().skip(from).enumerate() {
            let left = 1.0 - (n + 1) as f64 / fade.max(1) as f64;
            let gain = (left * PI / 2.0).sin().powi(2);
            *sample *= gain;
        }
    }
    let largest = channels
        .iter()
        .flatten()
        .fold(0.0_f64, |peak, sample| peak.max(sample.abs()));
    let scale = if largest > 0.0 { target / largest } else { 0.0 };
    let (left, right) = (channels.first()?, channels.get(1)?);
    let samples: Vec<f32> = left
        .iter()
        .zip(right)
        .flat_map(|(l, r)| [ceiling(l * scale) as f32, ceiling(r * scale) as f32])
        .collect();
    Sound::new(RATE, 2, samples)
}

/// What nothing is allowed to pass: -1 dBFS, which the sample is bent to rather than clipped at.
const CEILING: f64 = 0.89;
/// Where the bend starts.
const KNEE: f64 = 0.7;

fn ceiling(x: f64) -> f64 {
    let size = x.abs();
    if size <= KNEE {
        x
    } else {
        x.signum() * (KNEE + (CEILING - KNEE) * ((size - KNEE) / (CEILING - KNEE)).tanh())
    }
}
