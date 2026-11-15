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

pub use reader::Reader;

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

/// Longest sound Rook decodes, in frames: an hour at 48 kHz. Bigger claims
/// are damaged headers, not music.
const MAX_FRAMES: usize = 48_000 * 3600;

/// What decoding all of a file once found.
struct Whole {
    layout: Layout,
    frames: usize,
    /// Its samples, while they took at most the bytes kept.
    samples: Option<Vec<i16>>,
    /// At least its first [`CHUNK`] frames' samples.
    head: Vec<i16>,
}

fn read_whole(format: AudioFormat, file: Arc<[u8]>, kept: usize) -> Result<Whole, AudioError> {
    let mut stream = Stream::open(format, file, None)?;
    let mut samples = Some(Vec::new());
    let mut head = Vec::new();
    let mut chunk = Vec::new();
    let mut frames = 0;
    while stream.next(&mut chunk)? {
        let layout = stream.layout.expect("set by the first packet");
        if frames < CHUNK {
            head.extend_from_slice(&chunk);
        }
        frames += chunk.len() / usize::from(layout.channels);
        if frames > MAX_FRAMES {
            return Err(AudioError::Damaged(
                format.name(),
                "longer than an hour".into(),
            ));
        }
        if let Some(kept_samples) = &mut samples {
            if (kept_samples.len() + chunk.len()) * 2 > kept {
                samples = None;
            } else {
                kept_samples.extend_from_slice(&chunk);
            }
        }
    }
    let layout = stream
        .layout
        .ok_or_else(|| AudioError::Damaged(format.name(), "no audio".into()))?;
    Ok(Whole {
        layout,
        frames,
        samples,
        head,
    })
}

/// An Ogg stream's length in frames: the granule position of its last page.
fn ogg_end(bytes: &[u8]) -> Option<usize> {
    let at = bytes.windows(4).rposition(|w| w == b"OggS")?;
    let granule = i64::from_le_bytes(bytes.get(at + 6..at + 14)?.try_into().ok()?);
    usize::try_from(granule).ok().filter(|&n| n > 0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 16-bit PCM WAV of `frames` frames.
    pub(crate) fn wav(rate: u32, channels: u16, frames: u32) -> Vec<u8> {
        let data_len = frames * u32::from(channels) * 2;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        b.extend_from_slice(&(channels * 2).to_le_bytes());
        b.extend_from_slice(&16u16.to_le_bytes());
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        for i in 0..frames * u32::from(channels) {
            b.extend_from_slice(&((i % 200) as i16 * 100).to_le_bytes());
        }
        b
    }

    #[test]
    fn decodes_a_wav() {
        let audio = Audio::decode(&wav(22050, 1, 22050)).unwrap();
        assert_eq!(audio.format, Some(AudioFormat::Wav));
        assert_eq!((audio.sample_rate, audio.channels), (22050, 1));
        assert_eq!(audio.frames(), 22050);
        assert!((audio.seconds() - 1.0).abs() < 1e-9);
        assert!(!audio.is_encoded());
        assert_eq!(Arc::new(audio).reader().frame(3), [300; 2]);
    }

    /// `reader`'s sound once its chunks have all decoded.
    fn decoded(reader: &Reader) -> &Reader {
        let start = std::time::Instant::now();
        while !reader.decoded.as_ref().is_none_or(|d| d.is_done()) {
            assert!(start.elapsed().as_secs() < 10, "the sound never decoded");
            std::thread::yield_now();
        }
        reader
    }

    /// A sound kept as its file reads the samples it decoded to when it
    /// loaded, in any order, across a chunk's end, and to its last frame.
    #[test]
    fn a_sound_kept_encoded_reads_what_it_decoded_to() {
        let bytes = wav(8000, 2, CHUNK as u32 + 3_000);
        let pcm = Arc::new(Audio::decode(&bytes).unwrap());
        let encoded = Arc::new(Audio::decode_keeping(&bytes, 0).unwrap());
        assert!(encoded.is_encoded() && !pcm.is_encoded());
        assert_eq!(encoded.frames(), pcm.frames());
        let (want, got) = (pcm.reader(), encoded.reader());
        let got = decoded(&got);
        for at in 0..=pcm.frames() {
            assert_eq!(got.frame(at), want.frame(at), "frame {at}");
        }
        for at in [CHUNK, 3, CHUNK - 1, 0, CHUNK + 2_999, 7_777] {
            assert_eq!(got.frame(at), want.frame(at), "frame {at}");
        }
    }

    /// Every voice of a sound reads the one decoded copy, which goes once
    /// nothing reads it.
    #[test]
    fn voices_share_a_decode_that_goes_with_them() {
        let bytes = wav(8000, 1, 3 * CHUNK as u32);
        let audio = Arc::new(Audio::decode_keeping(&bytes, 0).unwrap());
        let (a, b) = (audio.reader(), audio.reader());
        let shared = a.decoded.clone().unwrap();
        assert!(Arc::ptr_eq(&shared, b.decoded.as_ref().unwrap()));
        let held = Arc::downgrade(&shared);
        drop((a, b, shared));
        let Data::Encoded(e) = &audio.data else {
            unreachable!()
        };
        // The decode thread lets go of it between chunks.
        let start = std::time::Instant::now();
        while held.strong_count() > 0 {
            assert!(start.elapsed().as_secs() < 10, "the decode was kept");
            std::thread::yield_now();
        }
        assert!(e.decoded.lock().unwrap().upgrade().is_none());
        // Playing again decodes again.
        let again = audio.reader();
        let at = 2 * CHUNK + 5;
        let want = (at % 200) as i16 * 100;
        assert_eq!(decoded(&again).frame(at), [want; 2]);
    }

    /// A long Ogg or MP3 keeps its bytes and its head, not its samples.
    #[test]
    fn long_sounds_keep_their_file() {
        let bytes = wav(8000, 1, 1000);
        let kept = Audio::decode_keeping(&bytes, 1000 * 2).unwrap();
        let encoded = Audio::decode_keeping(&bytes, 1000 * 2 - 1).unwrap();
        assert!(!kept.is_encoded() && encoded.is_encoded());
        assert_eq!(encoded.byte_len(), bytes.len() + 2000);
        assert_eq!(kept.byte_len(), 2000);
    }

    #[test]
    fn sniffs_formats() {
        assert_eq!(AudioFormat::sniff(b"OggS\0\x02"), Some(AudioFormat::Ogg));
        assert_eq!(AudioFormat::sniff(b"ID3\x03\0"), Some(AudioFormat::Mp3));
        assert_eq!(
            AudioFormat::sniff(&[0xFF, 0xFB, 0x90]),
            Some(AudioFormat::Mp3)
        );
        assert_eq!(AudioFormat::sniff(b"\x89PNG"), None);
    }

    #[test]
    fn rejects_damaged_files() {
        assert_eq!(Audio::decode(b"\x89PNG\r\n"), Err(AudioError::NotAudio));
        for bytes in [
            b"OggS\0\x02 garbage garbage garbage".to_vec(),
            b"ID3\x03\0\0\0\0\0\x05xxxxx".to_vec(),
            [0xFF, 0xFB, 0x90, 0x00, 1, 2, 3, 4].to_vec(),
            wav(22050, 1, 100)[..30].to_vec(),
            b"RIFF\0\0\0\0WAVE".to_vec(),
        ] {
            assert!(matches!(
                Audio::decode(&bytes),
                Err(AudioError::Damaged(..))
            ));
        }
        // A WAV cut halfway through its data keeps what's there.
        let full = wav(8000, 2, 1000);
        let cut = Audio::decode(&full[..full.len() / 2]).unwrap();
        assert!(cut.frames() > 0 && cut.frames() < 1000);
    }
}
