//! Reading a sound's frames as a voice plays them: straight from its
//! samples when it's kept decoded, else from its head and the chunks
//! decoded for everything playing it ([`super::decoded`]).

use std::sync::Arc;

use super::decoded::Decoded;
use super::{Audio, CHUNK, Data};

#[derive(Debug)]
pub struct Reader {
    audio: Arc<Audio>,
    /// Encoded sounds only: their decoded chunks, which this keeps.
    pub(super) decoded: Option<Arc<Decoded>>,
}

impl Reader {
    pub(super) fn new(audio: Arc<Audio>) -> Self {
        let decoded = audio.decoded();
        Self { audio, decoded }
    }

    pub fn audio(&self) -> &Arc<Audio> {
        &self.audio
    }

    /// Frame `at`'s left and right samples (a mono sound's one sample on
    /// both); silence past the end, and in a chunk not decoded yet (which
    /// only a seek far into a sound just started reaches).
    pub fn frame(&self, at: usize) -> [i16; 2] {
        if at >= self.audio.frames {
            return [0; 2];
        }
        let (samples, i) = match &self.audio.data {
            Data::Pcm(samples) => (&samples[..], at),
            Data::Encoded(encoded) => match at / CHUNK {
                0 => (&encoded.head[..], at),
                chunk => match self.decoded.as_ref().and_then(|d| d.chunk(chunk)) {
                    Some(samples) => (samples, at % CHUNK),
                    None => return [0; 2],
                },
            },
        };
        let channels = usize::from(self.audio.channels);
        let left = samples.get(i * channels).copied().unwrap_or(0);
        let right = if channels > 1 {
            samples.get(i * channels + 1).copied().unwrap_or(0)
        } else {
            left
        };
        [left, right]
    }
}
