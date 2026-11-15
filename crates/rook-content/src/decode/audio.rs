//! Decoding sound files (WAV, Ogg Vorbis, MP3) into the one PCM layout
//! Rook mixes.
//!
//! Every format goes through Symphonia (pure Rust), with only the decoders
//! the archive needs turned on: WAV (PCM and ADPCM), Ogg Vorbis and MP3
//! (`docs/behavior/audio/playback.md` has the counts and why). The format is picked
//! by the file's first bytes, not by its name or URL: the archive's
//! `sounds/*.mp3` paths and asset ids say nothing reliable about what they
//! hold.
//!
//! Damaged files fail with an [`AudioError`]; a decoder that panics on
//! hostile bytes is caught and reported the same way, so a file can never
//! take down a content worker.

mod decoded;
mod reader;
mod stream;
mod whole;

pub use reader::Reader;
use whole::*;

use std::sync::{Arc, Mutex, PoisonError, Weak};

use decoded::Decoded;

use stream::{Layout, Stream};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AudioFormat {
    Wav,
    Ogg,
    Mp3,
}

impl AudioFormat {
    /// The format by its first bytes: `RIFF....WAVE`, `OggS`, or an MP3's
    /// ID3 tag or frame sync.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
            Some(Self::Wav)
        } else if bytes.starts_with(b"OggS") {
            Some(Self::Ogg)
        } else if bytes.starts_with(b"ID3")
            || (bytes.len() > 1 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0)
        {
            Some(Self::Mp3)
        } else {
            None
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Wav => "WAV",
            Self::Ogg => "Ogg",
            Self::Mp3 => "MP3",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Ogg => "ogg",
            Self::Mp3 => "mp3",
        }
    }
}

/// A sound ready to mix: 16-bit samples, interleaved, one or two channels
/// (more are folded down to two). 16 bits keep a long music track at half
/// the memory of floats, with nothing audible lost.
///
/// A short sound is kept decoded. A longer Ogg or MP3 keeps its file's
/// bytes, a tenth of its samples or less, and its first chunk of samples;
/// the rest is decoded while something plays it, once for all its voices,
/// and let go after ([`decoded`]). A place's music and ambience decoded
/// whole came to gigabytes, though only a few of its sounds play at once.
/// A WAV is kept decoded whatever its length: its bytes are its samples
/// already.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub format: Option<AudioFormat>,
    pub sample_rate: u32,
    /// 1 or 2.
    pub channels: u16,
    frames: usize,
    data: Data,
}

#[derive(Debug, Clone, PartialEq)]
enum Data {
    Pcm(Vec<i16>),
    Encoded(Encoded),
}

#[derive(Debug)]
struct Encoded {
    file: Arc<[u8]>,
    /// The first [`CHUNK`] frames, interleaved: a sound starts, and a loop
    /// starts over, before anything decodes.
    head: Box<[i16]>,
    /// The rest while something plays it.
    decoded: Mutex<Weak<Decoded>>,
}

impl Clone for Encoded {
    fn clone(&self) -> Self {
        Self {
            file: self.file.clone(),
            head: self.head.clone(),
            decoded: Mutex::default(),
        }
    }
}

impl PartialEq for Encoded {
    fn eq(&self, other: &Self) -> bool {
        self.file == other.file
    }
}

/// Frames in a decoded chunk: about a third of a second at 44.1 kHz, which
/// the decode thread decodes in a few milliseconds.
const CHUNK: usize = 1 << 14;

/// The most a sound's samples take for it to be kept decoded. Unity keeps
/// small sounds decompressed and larger ones compressed; this is about a
/// second and a half of stereo at 44.1 kHz.
const PCM_KEPT: usize = 256 << 10;

impl Audio {
    /// A sound from interleaved samples (`channels` 1 or 2).
    pub fn from_pcm(sample_rate: u32, channels: u16, samples: Vec<i16>) -> Self {
        let channels = channels.clamp(1, 2);
        Self {
            format: None,
            sample_rate,
            channels,
            frames: samples.len() / usize::from(channels),
            data: Data::Pcm(samples),
        }
    }

    /// Frames (one sample per channel).
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Length in seconds (`Sound.TimeLength`).
    pub fn seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    /// Whether it's kept as its file's bytes, decoded while it plays.
    pub fn is_encoded(&self) -> bool {
        matches!(self.data, Data::Encoded(..))
    }

    /// Bytes its samples or its file take.
    pub fn byte_len(&self) -> usize {
        match &self.data {
            Data::Pcm(samples) => samples.len() * 2,
            Data::Encoded(e) => e.file.len() + e.head.len() * 2,
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let wav = AudioFormat::sniff(bytes) == Some(AudioFormat::Wav);
        Self::decode_keeping(bytes, if wav { usize::MAX } else { PCM_KEPT })
    }

    /// [`Audio::decode`], keeping it decoded if its samples take at most
    /// `kept` bytes.
    fn decode_keeping(bytes: &[u8], kept: usize) -> Result<Self, AudioError> {
        let format = AudioFormat::sniff(bytes).ok_or(AudioError::NotAudio)?;
        let file: Arc<[u8]> = Arc::from(bytes);
        let read = file.clone();
        let decoded = std::panic::catch_unwind(move || read_whole(format, read, kept))
            .unwrap_or_else(|_| {
                Err(AudioError::Damaged(
                    format.name(),
                    "decoder panicked".into(),
                ))
            });
        let Whole {
            layout,
            mut frames,
            samples,
            mut head,
        } = decoded?;
        if format == AudioFormat::Ogg
            && let Some(end) = ogg_end(bytes)
        {
            // The decoder hands out whole blocks; the stream's last granule
            // position says where it really ends (Studio's TimeLength
            // agrees: `docs/behavior/audio/playback.md`).
            frames = frames.min(end);
        }
        if frames == 0 {
            return Err(AudioError::Damaged(format.name(), "no audio".into()));
        }
        let data = match samples {
            Some(mut samples) => {
                samples.truncate(frames * usize::from(layout.channels));
                samples.shrink_to_fit();
                Data::Pcm(samples)
            }
            None => {
                head.truncate(frames.min(CHUNK) * usize::from(layout.channels));
                Data::Encoded(Encoded {
                    file,
                    head: head.into(),
                    decoded: Mutex::default(),
                })
            }
        };
        Ok(Self {
            format: Some(format),
            sample_rate: layout.rate,
            channels: layout.channels,
            frames,
            data,
        })
    }

    /// A reader of its samples from the start.
    pub fn reader(self: &Arc<Self>) -> Reader {
        Reader::new(self.clone())
    }

    /// An encoded sound's decoded chunks, started decoding unless
    /// something plays it already.
    fn decoded(&self) -> Option<Arc<Decoded>> {
        let Data::Encoded(e) = &self.data else {
            return None;
        };
        let mut slot = e.decoded.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(decoded) = slot.upgrade() {
            return Some(decoded);
        }
        let decoded = decoded::start(decoded::Source {
            format: self.format.expect("an encoded sound has a format"),
            file: e.file.clone(),
            layout: Layout {
                rate: self.sample_rate,
                channels: self.channels,
            },
            frames: self.frames,
        });
        *slot = Arc::downgrade(&decoded);
        Some(decoded)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AudioError {
    #[error("not a WAV, Ogg or MP3 sound")]
    NotAudio,
    #[error("{0} sound is damaged: {1}")]
    Damaged(&'static str, String),
}

#[cfg(test)]
pub(crate) mod tests;
