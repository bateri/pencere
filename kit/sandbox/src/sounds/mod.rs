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

use std::time::Duration;

pub use pencere_view::sound::{MOST_CHANNELS, Sound};

pub use engine::{Arrival, Ask, Sounds};
pub use palette::{Variation, palette, palette_wavs, play_of, variation, wav};

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
