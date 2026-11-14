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

mod reader;
mod stream;

pub use reader::Reader;

use std::sync::Arc;

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
/// bytes, a tenth of its samples or less, and is decoded as it plays
/// ([`Reader`]): Unity's "Decompress On Load" and "Compressed In Memory",
/// FMOD's samples and compressed samples. A place's music and ambience
/// decoded whole came to gigabytes, and decoding Vorbis or MP3 runs at
/// hundreds of times real time. A WAV is kept decoded whatever its length:
/// its bytes are its samples already.
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
    /// The file, decoded from its start the way [`Audio::decode`] read it.
    Encoded(Arc<[u8]>, Stamps),
}

/// Where the decoder's packet timestamps are in the sound's frames. They
/// mostly count frames from the start, but some Ogg files' run ahead or
/// behind by part of a block, all through or from some packet on, and a
/// seek goes by timestamps alone. So the load, which decodes every packet
/// anyway, notes each packet whose timestamp doesn't follow from the one
/// before, with the frame it starts at. (A few files' packets are stamped
/// differently after a seek than read from the start: a seek in those
/// lands up to a block, some 10 ms, off.)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Stamps(Box<[(u64, usize)]>);

impl Stamps {
    /// The frame a packet stamped `ts` starts at.
    pub(crate) fn frame(&self, ts: u64) -> usize {
        let i = self.0.partition_point(|&(t, _)| t <= ts);
        let (t, frame) = i.checked_sub(1).map_or((0, 0), |i| self.0[i]);
        frame.saturating_add(usize::try_from(ts - t).unwrap_or(usize::MAX))
    }

    /// The timestamp of the packet frame `frame` is in, or about.
    pub(crate) fn ts(&self, frame: usize) -> u64 {
        let i = self.0.partition_point(|&(_, f)| f <= frame);
        let (t, f) = i.checked_sub(1).map_or((0, 0), |i| self.0[i]);
        t.saturating_add((frame - f) as u64)
    }
}

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

    /// Whether it's kept as its file's bytes, decoded as it plays.
    pub fn is_encoded(&self) -> bool {
        matches!(self.data, Data::Encoded(..))
    }

    /// Bytes its samples or its file take.
    pub fn byte_len(&self) -> usize {
        match &self.data {
            Data::Pcm(samples) => samples.len() * 2,
            Data::Encoded(bytes, stamps) => bytes.len() + stamps.0.len() * 16,
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
            stamps,
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
            None => Data::Encoded(file, Stamps(stamps.into())),
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
    /// [`Stamps`]'s packets.
    stamps: Vec<(u64, usize)>,
}

fn read_whole(format: AudioFormat, file: Arc<[u8]>, kept: usize) -> Result<Whole, AudioError> {
    let mut stream = Stream::open(format, file, None)?;
    let mut samples = Some(Vec::new());
    let mut stamps = Vec::new();
    let mut chunk = Vec::new();
    let mut frames = 0;
    let mut expected = None;
    while let Some(ts) = stream.next(&mut chunk)? {
        let layout = stream.layout.expect("set by the first packet");
        let length = chunk.len() / usize::from(layout.channels);
        if expected != Some(ts) {
            stamps.push((ts, frames));
        }
        expected = ts.checked_add(length as u64);
        frames += length;
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
        stamps,
    })
}

/// An Ogg stream's length in frames: the granule position of its last page.
fn ogg_end(bytes: &[u8]) -> Option<usize> {
    let mut at = 0usize;
    let mut end = None;
    loop {
        let Some(header_end) = at.checked_add(27) else {
            break;
        };
        let Some(header) = bytes.get(at..header_end) else {
            break;
        };
        if &header[..4] != b"OggS" || header[4] != 0 {
            break;
        }

        let granule = i64::from_le_bytes(header[6..14].try_into().ok()?);
        let segments = usize::from(header[26]);
        let table = header_end;
        let Some(body) = table.checked_add(segments) else {
            break;
        };
        let Some(lacing) = bytes.get(table..body) else {
            break;
        };
        let Some(body_len) = lacing
            .iter()
            .try_fold(0usize, |sum, &size| sum.checked_add(usize::from(size)))
        else {
            break;
        };
        let Some(page_end) = body.checked_add(body_len) else {
            break;
        };
        if page_end > bytes.len() {
            break;
        }
        if let Ok(granule) = usize::try_from(granule)
            && granule > 0
        {
            end = Some(granule);
        }
        at = page_end;
    }
    end
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

    /// A sound kept as its file reads the samples it decoded to when it
    /// loaded: played through, after seeks back and ahead, and starting
    /// over as a loop does.
    #[test]
    fn a_sound_kept_encoded_reads_what_it_decoded_to() {
        let bytes = wav(8000, 2, 20_000);
        let pcm = Arc::new(Audio::decode(&bytes).unwrap());
        let encoded = Arc::new(Audio::decode_keeping(&bytes, 0).unwrap());
        assert!(encoded.is_encoded() && !pcm.is_encoded());
        assert_eq!(encoded.frames(), pcm.frames());
        let (mut want, mut got) = (pcm.reader(), encoded.reader());
        for at in 0..pcm.frames() {
            assert_eq!(got.frame(at), want.frame(at), "frame {at}");
        }
        for at in [15_000, 3, 19_999, 0, 7_777, 7_776, 7_778, 12_345, 20_000] {
            assert_eq!(got.frame(at), want.frame(at), "frame {at}");
        }
        // A voice blending each frame into the next, at an odd speed.
        let mut at = 0.0;
        while (at as usize) + 1 < pcm.frames() {
            let i = at as usize;
            assert_eq!(
                [got.frame(i), got.frame(i + 1)],
                [want.frame(i), want.frame(i + 1)]
            );
            at += 0.7;
        }
    }

    /// A long Ogg or MP3 keeps its bytes, not its samples.
    #[test]
    fn long_sounds_keep_their_file() {
        let bytes = wav(8000, 1, 1000);
        let kept = Audio::decode_keeping(&bytes, 1000 * 2).unwrap();
        let encoded = Audio::decode_keeping(&bytes, 1000 * 2 - 1).unwrap();
        assert!(!kept.is_encoded() && encoded.is_encoded());
        // The file, and a stamp or two.
        assert!((bytes.len()..bytes.len() + 64).contains(&encoded.byte_len()));
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
    fn ogg_duration_ignores_capture_pattern_in_payload() {
        let mut payload = b"compressed packet OggS".to_vec();
        payload.extend_from_slice(&[0, 0]);
        payload.extend_from_slice(&1i64.to_le_bytes());

        let page = |granule: i64, payload: &[u8]| {
            let mut page = b"OggS".to_vec();
            page.extend_from_slice(&[0, 0]); // version and header type
            page.extend_from_slice(&granule.to_le_bytes()); // granule position
            page.extend_from_slice(&[0; 12]); // serial, sequence, checksum
            page.push(1); // one segment
            page.push(payload.len() as u8);
            page.extend_from_slice(payload);
            page
        };
        let mut stream = page(100, &payload);

        assert_eq!(ogg_end(&stream), Some(100));
        stream.extend_from_slice(&page(200, &[]));
        assert_eq!(ogg_end(&stream), Some(200));
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
