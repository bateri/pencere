//! The sounds of a view: the names of the sounds the kit has of its own, what a page asks of
//! the sounds of its view, the samples a host is given to play, and the rule of how many play at
//! once.
//!
//! The names are the interface jobs a sound does (a tap, a toggle, a result that landed), not how
//! it is made, so that a recipe may change and a name stays what it says. The crate holds the
//! names, the asks and the samples' type and plays nothing: the sounds themselves are made, read
//! and decoded by the sandbox, which holds the synthesis and the decoders, and are the same on
//! every system.

use std::sync::Arc;

/// The names of the sounds the kit has, in the order a page is told of them.
pub const NAMES: [&str; 20] = [
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

/// Whether the kit has a sound by this name.
#[must_use]
pub fn is_name(name: &str) -> bool {
    NAMES.contains(&name)
}

/// The most sounds a view plays at once: the ninth silences the one that has been playing longest.
pub const MOST_VOICES: usize = 8;
/// The most asks one reply makes (a list of sounds, with the ends of them among them).
pub const MOST_ASKS: usize = 64;
/// The slowest and the fastest a sound is asked to play at: half as fast and twice as fast.
pub const LOWEST_PITCH: f32 = 0.5;
pub const HIGHEST_PITCH: f32 = 2.0;

/// Where a sound a page asks for comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// One of the kit's own, by its name ([`NAMES`]).
    Kit(String),
    /// A file: a path of the view's bundle, `sha256:` and a digest, or `file:` and a handle.
    File(String),
}

/// A sound a page asks to play.
#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    pub source: Source,
    /// How loud, from 0 to 1, as the page asks: what the host multiplies [`Play::samples`] by, and
    /// nothing else (the little difference of each play of a sound played often is in the samples).
    pub volume: f32,
    /// How fast, from [`LOWEST_PITCH`] to [`HIGHEST_PITCH`], as the page asks: a sound at 2 is
    /// twice as fast, and an octave higher. It is told to the host as it was asked; the samples are
    /// already at it, and the host does not pitch them again.
    pub pitch: f32,
    /// The sound, ready to play, at the pitch asked for, once the kit has made or decoded it; `None`
    /// in what a page says, and in what a host is given never, as a sound that is not ready is
    /// given when it is.
    pub samples: Option<Arc<Sound>>,
}

/// What a page asks of the sounds of its view.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    /// Play this, over what is playing.
    Play(Play),
    /// End every sound the view is playing.
    Stop,
}

/// Which sounds a view is playing, for the rule that it plays [`MOST_VOICES`] at a time: the one
/// that is started when that many are playing silences the one that started first. A host that
/// plays the sounds keeps one for each view, and stops what it says to stop.
#[derive(Clone, Debug, Default)]
pub struct Voices {
    /// What plays, oldest first: its number and the second it ends at.
    playing: Vec<(u64, f64)>,
}

impl Voices {
    /// A view that plays nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A sound numbered `id` of `seconds` starts at `now` (seconds, on any one clock): the number
    /// of the sound it silences, if the view was playing as many as it may.
    pub fn play(&mut self, now: f64, id: u64, seconds: f64) -> Option<u64> {
        self.playing.retain(|&(_, ends)| ends > now);
        self.playing.push((id, now + seconds));
        (self.playing.len() > MOST_VOICES).then(|| self.playing.remove(0).0)
    }

    /// Every sound is ended: the numbers of those that were playing at `now`.
    pub fn stop_all(&mut self, now: f64) -> Vec<u64> {
        self.playing
            .drain(..)
            .filter(|&(_, ends)| ends > now)
            .map(|(id, _)| id)
            .collect()
    }

    /// How many are playing at `now`.
    #[must_use]
    pub fn playing(&self, now: f64) -> usize {
        self.playing.iter().filter(|&&(_, ends)| ends > now).count()
    }
}

/// The most channels a sound has: a sound is mono or stereo.
pub const MOST_CHANNELS: u16 = 2;

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
