//! A sound file decoded packet by packet: the one decoding loop that both
//! the load ([`super::Audio::decode`]) and playback ([`super::Reader`])
//! run, so a sound decoded as it plays gives exactly the samples it gave
//! when it loaded.

use std::io::Cursor;
use std::sync::Arc;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::{AudioError, AudioFormat};

/// The rate and channels kept: those of the first packet, at most two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Layout {
    pub rate: u32,
    pub channels: u16,
}

/// The file's bytes, shared with the [`super::Audio`] that keeps them.
struct Bytes(Arc<[u8]>);

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub(super) struct Stream {
    format: AudioFormat,
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track: u32,
    /// Set by the first packet decoded, or given when the sound is known.
    pub layout: Option<Layout>,
    buffer: Option<SampleBuffer<i16>>,
    /// Whether a packet has decoded since the stream opened.
    decoded_any: bool,
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stream")
            .field("format", &self.format)
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

impl Stream {
    /// Open `file` at its start. `layout` is the sound's when it's known,
    /// so packets in another rate are skipped from the first on.
    pub(super) fn open(
        format: AudioFormat,
        file: Arc<[u8]>,
        layout: Option<Layout>,
    ) -> Result<Self, AudioError> {
        let damaged = |e: &dyn std::fmt::Display| AudioError::Damaged(format.name(), e.to_string());
        let source = MediaSourceStream::new(
            Box::new(Cursor::new(Bytes(file))),
            MediaSourceStreamOptions::default(),
        );
        let mut hint = Hint::new();
        hint.with_extension(format.extension());
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| damaged(&e))?;
        let reader = probed.format;
        let track = reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| damaged(&"no audio track"))?;
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| damaged(&e))?;
        Ok(Self {
            format,
            track: track.id,
            reader,
            decoder,
            layout,
            buffer: None,
            decoded_any: false,
        })
    }

    /// Replace `out` with the next packet's frames, interleaved in the
    /// kept layout, returning the packet's timestamp (in frames). `None` at
    /// the end: the file's end, or damage after something decoded (a
    /// cut-off file keeps what came before, as players do). A packet that
    /// decodes to nothing (the first after a seek only primes a Vorbis
    /// decoder) is passed over.
    pub(super) fn next(&mut self, out: &mut Vec<i16>) -> Result<Option<u64>, AudioError> {
        let damaged =
            |e: &dyn std::fmt::Display| AudioError::Damaged(self.format.name(), e.to_string());
        out.clear();
        loop {
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(_) if self.decoded_any => return Ok(None),
                Err(e) => return Err(damaged(&e)),
            };
            if packet.track_id() != self.track {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
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
            let layout = *self.layout.get_or_insert(Layout {
                rate: spec.rate,
                channels: channels.min(2) as u16,
            });
            if layout.rate != spec.rate {
                // Rare in MP3s; the rest of the file plays at the first rate.
                continue;
            }
            let buf = match &mut self.buffer {
                Some(b) if b.capacity() >= decoded.capacity() * channels => b,
                _ => self
                    .buffer
                    .insert(SampleBuffer::new(decoded.capacity() as u64, spec)),
            };
            buf.copy_interleaved_ref(decoded);
            fold(out, buf.samples(), channels, usize::from(layout.channels));
            self.decoded_any = true;
            if !out.is_empty() {
                return Ok(Some(packet.ts()));
            }
        }
    }

    /// Move to the packet stamped `ts` or one before it.
    pub(super) fn seek(&mut self, ts: u64) -> Result<(), AudioError> {
        let to = SeekTo::TimeStamp {
            ts,
            track_id: self.track,
        };
        (self.reader.seek(SeekMode::Accurate, to))
            .map_err(|e| AudioError::Damaged(self.format.name(), e.to_string()))?;
        self.decoder.reset();
        Ok(())
    }
}

/// Append interleaved frames of `channels`, folding more than two channels
/// into the first two and keeping a mono or stereo layout as it is.
fn fold(out: &mut Vec<i16>, samples: &[i16], channels: usize, keep: usize) {
    if channels == keep {
        out.extend_from_slice(samples);
        return;
    }
    for frame in samples.chunks_exact(channels) {
        if keep == 1 {
            out.push(frame[0]);
        } else {
            out.extend_from_slice(&frame[..2]);
        }
    }
}
