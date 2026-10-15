//! Decoding sound files (WAV, Ogg Vorbis, MP3) into the one PCM layout
//! Rook mixes.
//!
//! Every format goes through Symphonia (pure Rust), with only the decoders
//! the archive needs turned on: WAV (PCM and ADPCM), Ogg Vorbis and MP3
//! (`docs/behavior/audio.md` has the counts and why). The format is picked
//! by the file's first bytes, not by its name or URL: the archive's
//! `sounds/*.mp3` paths and asset ids say nothing reliable about what they
//! hold.
//!
//! Damaged files fail with an [`AudioError`]; a decoder that panics on
//! hostile bytes is caught and reported the same way, so a file can never
//! take down a content worker.

use std::io::Cursor;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// The sound file formats Rook reads.
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

/// A decoded sound: 16-bit samples, interleaved, one or two channels (more
/// are folded down to two). 16 bits keep a long music track at half the
/// memory of floats, with nothing audible lost.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Audio {
    pub format: Option<AudioFormat>,
    pub sample_rate: u32,
    /// 1 or 2.
    pub channels: u16,
    pub samples: Vec<i16>,
}

impl Audio {
    /// Frames (one sample per channel).
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels.max(1))
    }

    /// Length in seconds (`Sound.TimeLength`).
    pub fn seconds(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    /// Decode a WAV, Ogg Vorbis or MP3 file.
    pub fn decode(bytes: &[u8]) -> Result<Self, AudioError> {
        let format = AudioFormat::sniff(bytes).ok_or(AudioError::NotAudio)?;
        let owned = bytes.to_vec();
        let decoded =
            std::panic::catch_unwind(move || decode_with(format, owned)).unwrap_or_else(|_| {
                Err(AudioError::Damaged(
                    format.name(),
                    "decoder panicked".into(),
                ))
            });
        let mut audio = decoded?;
        audio.format = Some(format);
        if format == AudioFormat::Ogg
            && let Some(end) = ogg_end(bytes)
        {
            // The decoder hands out whole blocks; the stream's last granule
            // position says where it really ends (Studio's TimeLength
            // agrees: `docs/behavior/audio.md`).
            let keep = end.saturating_mul(usize::from(audio.channels));
            audio.samples.truncate(keep);
        }
        if audio.samples.is_empty() {
            return Err(AudioError::Damaged(format.name(), "no audio".into()));
        }
        Ok(audio)
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

fn decode_with(format: AudioFormat, bytes: Vec<u8>) -> Result<Audio, AudioError> {
    let damaged = |e: &dyn std::fmt::Display| AudioError::Damaged(format.name(), e.to_string());
    let stream = MediaSourceStream::new(
        Box::new(Cursor::new(bytes)),
        MediaSourceStreamOptions::default(),
    );
    let mut hint = Hint::new();
    hint.with_extension(format.extension());
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| damaged(&e))?;
    let mut reader = probed.format;
    let track = reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| damaged(&"no audio track"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| damaged(&e))?;

    let mut out = Audio::default();
    let mut buffer: Option<SampleBuffer<i16>> = None;
    loop {
        let packet = match reader.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            // A cut-off file keeps what decoded before the cut, as players
            // do; one that decodes nothing fails below.
            Err(_) if !out.samples.is_empty() => break,
            Err(e) => return Err(damaged(&e)),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // One bad frame is skipped, as players do.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(damaged(&e)),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count();
        if channels == 0 || spec.rate == 0 {
            return Err(damaged(&"no channels"));
        }
        if out.sample_rate == 0 {
            out.sample_rate = spec.rate;
            out.channels = channels.min(2) as u16;
        } else if out.sample_rate != spec.rate {
            // Rare in MP3s; the rest of the file plays at the first rate.
            continue;
        }
        let buf = match &mut buffer {
            Some(b) if b.capacity() >= decoded.capacity() * channels => b,
            _ => buffer.insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
        };
        buf.copy_interleaved_ref(decoded);
        push_frames(&mut out, buf.samples(), channels);
        if out.frames() > MAX_FRAMES {
            return Err(damaged(&"longer than an hour"));
        }
    }
    Ok(out)
}

/// An Ogg stream's length in frames: the granule position of its last page.
fn ogg_end(bytes: &[u8]) -> Option<usize> {
    let at = bytes.windows(4).rposition(|w| w == b"OggS")?;
    let granule = i64::from_le_bytes(bytes.get(at + 6..at + 14)?.try_into().ok()?);
    usize::try_from(granule).ok().filter(|&n| n > 0)
}

/// Append interleaved frames of `channels`, folding more than two channels
/// into the first two and keeping a mono or stereo layout as it is.
fn push_frames(out: &mut Audio, samples: &[i16], channels: usize) {
    let keep = usize::from(out.channels);
    if channels == keep {
        out.samples.extend_from_slice(samples);
        return;
    }
    for frame in samples.chunks_exact(channels) {
        if keep == 1 {
            out.samples.push(frame[0]);
        } else {
            out.samples.extend_from_slice(&frame[..2]);
        }
    }
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
        assert_eq!(audio.samples[3], 300);
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

    /// Damaged files fail with an error, never a panic.
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
