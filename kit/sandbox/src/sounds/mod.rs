//! The sounds of a page: the kit's own palette, made here, and the files a page names, read and
//! decoded here, so that every terminal plays the same sounds under the same limits and has
//! nothing to do but play the samples it is given.
//!
//! - **The palette** ([`palette`]) is twenty sounds made by synthesis — struck bars, switches,
//!   air, a small room — the same on every system, made once per process.
//! - **A page's own file** is told from its bytes (WAV or Ogg Vorbis, [`decode`]), read through
//!   [`Assets`](crate::Assets) as the images are (a bundle path, `sha256:` and a digest, `file:`
//!   and a handle), decoded on a thread of its own, held to what a file and a view may be
//!   ([`SoundLimits`]), and given pitched to the ask ([`Sounds`]).
//!
//! A sound is [`Sound`]: 32-bit float samples, interleaved, one channel or two, at the rate it was
//! made at. The bytes of a file are not trusted: a decoder that fails, or gives up by panicking,
//! leaves the file unreadable and nothing else harmed.

pub(crate) mod decode;
mod engine;
pub(crate) mod palette;
mod synth;

use std::sync::Arc;
use std::time::Duration;

pub use engine::{Arrival, Ask, Sounds};
pub use palette::{Variation, palette, palette_wavs, variation, wav};

/// The most channels a sound has: a sound is mono or stereo.
pub const MOST_CHANNELS: u16 = 2;

/// What the sounds of one view are held to.
#[derive(Clone, Debug)]
pub struct SoundLimits {
    /// Bytes the file of one sound may be: the most any file of a bundle may be.
    pub file_bytes: usize,
    /// Bytes of samples (4 a sample, a sound's samples as they are given to the host) the view may
    /// hold in all, over its sounds as they are kept. A sound whose own samples are more than this
    /// is refused. The kit's palette is not counted: it is made once for the process.
    pub decoded_bytes: usize,
    /// Different paths a view may name; the ones after that are refused without being read.
    pub paths: usize,
    /// How long a sound that is not decoded yet is waited for: a sound asked for and not decoded by
    /// then is dropped, and the page's author is told.
    pub wait: Duration,
}

impl Default for SoundLimits {
    fn default() -> Self {
        Self {
            file_bytes: 8 << 20,
            decoded_bytes: 64 << 20,
            paths: 1024,
            wait: Duration::from_secs(1),
        }
    }
}

/// A sound: samples a host plays as they are, at the rate they are at.
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    rate: u32,
    channels: u16,
    samples: Arc<[f32]>,
}

impl Sound {
    /// A sound of `channels` (one or two) interleaved `samples` at `rate` Hz, or `None` where there
    /// is no sound in them: no samples, a part of a frame, no rate, or a count of channels a sound
    /// does not have.
    #[must_use]
    pub fn new(rate: u32, channels: u16, samples: Vec<f32>) -> Option<Self> {
        let whole =
            !samples.is_empty() && samples.len().is_multiple_of(usize::from(channels.max(1)));
        (rate > 0 && (1..=MOST_CHANNELS).contains(&channels) && whole).then(|| Self {
            rate,
            channels,
            samples: samples.into(),
        })
    }

    /// Samples in a second of one channel.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// One channel or two.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// The samples, a frame (one for each channel) after another, each from -1 to 1.
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// How many frames it has.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// How long it plays, in seconds.
    #[must_use]
    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.rate)
    }

    /// Bytes its samples take.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.samples.len() * 4
    }

    /// The largest a sample is, whichever side it is on.
    #[must_use]
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0, |peak, s| peak.max(s.abs()))
    }

    /// The sound played `ratio` times as fast, which is a pitch `ratio` times as high and a length
    /// `1/ratio` times as long (cubic interpolation between the frames; what is above the half of the
    /// new rate is not filtered away, which a ratio near one does not make heard). The rate is the
    /// same: the host plays the new samples as it plays any other. A ratio that is no number above
    /// zero is the sound as it is.
    #[must_use]
    pub fn pitched(&self, ratio: f32) -> Sound {
        if !(ratio.is_finite() && ratio > 0.0) || (ratio - 1.0).abs() < f32::EPSILON {
            return self.clone();
        }
        let channels = usize::from(self.channels);
        let frames = self.frames();
        let out = ((frames as f64 / f64::from(ratio)).round() as usize).max(1);
        let mut samples = Vec::with_capacity(out * channels);
        let last = frames.saturating_sub(1);
        for frame in 0..out {
            let at = frame as f64 * f64::from(ratio);
            let whole = (at.floor() as usize).min(last);
            let t = (at - whole as f64) as f32;
            for channel in 0..channels {
                let tap = |offset: isize| -> f32 {
                    let index = whole.saturating_add_signed(offset).min(last);
                    self.samples
                        .get(index * channels + channel)
                        .copied()
                        .unwrap_or(0.0)
                };
                let (a, b, c, d) = (tap(-1), tap(0), tap(1), tap(2));
                // Catmull-Rom: through the frames, smooth between.
                let value = b + 0.5
                    * t
                    * (c - a + t * (2.0 * a - 5.0 * b + 4.0 * c - d + t * (3.0 * (b - c) + d - a)));
                samples.push(value.clamp(-1.0, 1.0));
            }
        }
        Sound {
            rate: self.rate,
            channels: self.channels,
            samples: samples.into(),
        }
    }
}

/// Why a file is not a sound the page can have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The bundle has no such file.
    Missing,
    /// The file is not in a format the kit reads.
    Unknown,
    /// The file is larger than a file may be.
    Source { bytes: usize, most: usize },
    /// The decoder could not read it, in its words.
    Broken(String),
    /// The sound has more channels than a sound may.
    Channels { count: u16 },
    /// The sound is at a rate no host plays.
    Rate { hertz: u32 },
    /// The sound is by itself more than the view may hold, at the pitch it is asked for.
    Whole { bytes: usize, most: usize },
    /// The view names more sounds than it may.
    Paths { most: usize },
    /// The sound was not decoded in the time a page may wait for one.
    Late { millis: u128 },
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => f.write_str("is not a file of the view's bundle"),
            Self::Unknown => f.write_str("is not a WAV or an Ogg Vorbis file"),
            Self::Source { bytes, most } => {
                write!(f, "is {bytes} bytes, past the {most} bytes a file may be")
            }
            Self::Broken(why) => write!(f, "could not be decoded: {why}"),
            Self::Channels { count } => write!(
                f,
                "has {count} channels, and a sound has one or two (mono or stereo)"
            ),
            Self::Rate { hertz } => write!(f, "is at {hertz} Hz, which no device plays"),
            Self::Whole { bytes, most } => write!(
                f,
                "is {} MiB of samples at that pitch, past the {} MiB a view may hold",
                bytes >> 20,
                most >> 20
            ),
            Self::Paths { most } => write!(f, "is past the {most} sounds a view may name"),
            Self::Late { millis } => write!(
                f,
                "was not decoded in {millis} ms, so what asked for it is dropped (it plays when it is asked for again)"
            ),
        }
    }
}

/// Why a sound is not played, for the person who wrote the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The path the page named.
    pub path: String,
    /// What is wrong with it, a sentence that goes after the path.
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sound {:?} {}", self.path, self.message)
    }
}
