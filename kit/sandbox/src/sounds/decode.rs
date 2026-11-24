//! Reading the formats the files of a page's sounds come in: WAV and Ogg Vorbis.
//!
//! The bytes are not trusted: they are a file a program sent, and what is in them is whatever its
//! author wrote. So the format is told from the bytes and not from a name, the size of a WAV is
//! read from its header and approved **before** a sample is made, an Ogg is read packet by packet
//! and stopped where it would pass what a view may hold, and a decoder that fails, or gives up by
//! panicking (which is caught here), leaves the file unreadable and nothing else harmed.
//!
//! Samples come out as 32-bit floats from -1 to 1, interleaved, of one channel or two.

use std::io::Cursor;
use std::panic::{AssertUnwindSafe, catch_unwind};

use super::{Failure, MOST_CHANNELS, Sound};

/// The formats a sound can be in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Format {
    Wav,
    Ogg,
}

/// The format of the bytes, told from the bytes.
pub(crate) fn sniff(bytes: &[u8]) -> Option<Format> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        Some(Format::Wav)
    } else if bytes.starts_with(b"OggS") {
        Some(Format::Ogg)
    } else {
        None
    }
}

/// The lowest and the highest rate of samples a sound may be at.
const RATES: std::ops::RangeInclusive<u32> = 1_000..=768_000;

/// The sound in `bytes`, whose samples take at most `most` bytes.
pub(crate) fn decode(bytes: &[u8], most: usize) -> Result<Sound, Failure> {
    let format = sniff(bytes).ok_or(Failure::Unknown)?;
    // A decoder that gives up by panicking costs the sound and not the thread that asked.
    catch_unwind(AssertUnwindSafe(|| match format {
        Format::Wav => wav(bytes, most),
        Format::Ogg => ogg(bytes, most),
    }))
    .unwrap_or_else(|_| Err(Failure::Broken("the decoder gave up".into())))
}

fn broken(why: impl std::fmt::Display) -> Failure {
    Failure::Broken(why.to_string())
}

/// Whether a file at this rate and this many channels is one a host plays.
fn approve(rate: u32, channels: u16) -> Result<(), Failure> {
    if !(1..=MOST_CHANNELS).contains(&channels) {
        return Err(Failure::Channels { count: channels });
    }
    if !RATES.contains(&rate) {
        return Err(Failure::Rate { hertz: rate });
    }
    Ok(())
}

fn finish(rate: u32, channels: u16, samples: Vec<f32>) -> Result<Sound, Failure> {
    Sound::new(rate, channels, samples).ok_or_else(|| broken("it has no sound in it"))
}

fn wav(bytes: &[u8], most: usize) -> Result<Sound, Failure> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes)).map_err(broken)?;
    let spec = reader.spec();
    approve(spec.sample_rate, spec.channels)?;
    // The header says how many samples there are: it is held to what a view may hold before one is
    // read, and a header that says more than the file has is a file that is cut short.
    let wanted = (reader.len() as usize).saturating_mul(4);
    if wanted > most {
        return Err(Failure::Whole {
            bytes: wanted,
            most,
        });
    }
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|sample| {
                sample.map(|s| {
                    if s.is_finite() {
                        s.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    }
                })
            })
            .collect::<Result<_, _>>()
            .map_err(broken)?,
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample.clamp(1, 32);
            let scale = 1.0 / (1u64 << (bits - 1)) as f64;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|s| (f64::from(s) * scale).clamp(-1.0, 1.0) as f32))
                .collect::<Result<_, _>>()
                .map_err(broken)?
        }
    };
    finish(spec.sample_rate, spec.channels, samples)
}

fn ogg(bytes: &[u8], most: usize) -> Result<Sound, Failure> {
    let mut reader =
        lewton::inside_ogg::OggStreamReader::new(Cursor::new(bytes)).map_err(broken)?;
    let channels = u16::from(reader.ident_hdr.audio_channels);
    let rate = reader.ident_hdr.audio_sample_rate;
    approve(rate, channels)?;
    let mut samples: Vec<f32> = Vec::new();
    while let Some(packet) = reader.read_dec_packet_itl().map_err(broken)? {
        let wanted = samples.len().saturating_add(packet.len()).saturating_mul(4);
        if wanted > most {
            return Err(Failure::Whole {
                bytes: wanted,
                most,
            });
        }
        samples.extend(packet.iter().map(|&s| f32::from(s) / 32768.0));
    }
    finish(rate, channels, samples)
}
