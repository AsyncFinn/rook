//! Reading a sound's frames in order, as a voice plays it: straight from
//! its samples when it's kept decoded, else decoding its file a packet at
//! a time, as FMOD plays a compressed sample. Reading on from the last
//! frame read decodes what follows; reading far ahead or back seeks.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use super::stream::{Layout, Stream};
use super::{Audio, AudioError, Data};

/// How far ahead a read decodes on to rather than seeking, as a fraction
/// of a second: a quarter, the gap between a voice's reads being a buffer
/// or so.
const DECODE_ON: usize = 4;

#[derive(Debug)]
pub struct Reader {
    audio: Arc<Audio>,
    /// Encoded sounds only: the file, open where the window ends.
    stream: Option<Stream>,
    /// The first packet's frames, interleaved: a looped sound starts over
    /// from these without opening the file again.
    head: Vec<i16>,
    /// Frames decoded from `start`, interleaved: the last packet, and the
    /// frame before it (a voice blending one frame into the next reads
    /// both, one in each packet).
    window: Vec<i16>,
    start: usize,
    /// The frame the stream's next packet starts at.
    next: usize,
    /// The file failed to decode as it did when it loaded (or the decoder
    /// panicked): silent from then on.
    broken: bool,
    packet: Vec<i16>,
}

impl Reader {
    pub(super) fn new(audio: Arc<Audio>) -> Self {
        Self {
            audio,
            stream: None,
            head: Vec::new(),
            window: Vec::new(),
            start: 0,
            next: 0,
            broken: false,
            packet: Vec::new(),
        }
    }

    pub fn audio(&self) -> &Arc<Audio> {
        &self.audio
    }

    /// Frame `at`'s left and right samples (a mono sound's one sample on
    /// both); silence past the end.
    pub fn frame(&mut self, at: usize) -> [i16; 2] {
        if at >= self.audio.frames {
            return [0; 2];
        }
        let channels = usize::from(self.audio.channels);
        let pick = |samples: &[i16], i: usize| {
            let left = samples.get(i * channels).copied().unwrap_or(0);
            let right = if channels > 1 {
                samples.get(i * channels + 1).copied().unwrap_or(0)
            } else {
                left
            };
            [left, right]
        };
        if let Data::Pcm(samples) = &self.audio.data {
            return pick(samples, at);
        }
        if at < self.head.len() / channels {
            return pick(&self.head, at);
        }
        if !self.covers(at) {
            self.fill(at);
        }
        match at.checked_sub(self.start) {
            Some(i) if self.covers(at) => pick(&self.window, i),
            _ => [0; 2],
        }
    }

    fn covers(&self, at: usize) -> bool {
        let channels = usize::from(self.audio.channels);
        at >= self.start && at < self.start + self.window.len() / channels
    }

    fn fill(&mut self, at: usize) {
        if self.broken {
            return;
        }
        let filled = std::panic::catch_unwind(AssertUnwindSafe(|| self.decode_to(at)));
        if !matches!(filled, Ok(Ok(()))) {
            self.broken = true;
            self.stream = None;
            self.window.clear();
        }
    }

    /// Decode on until the window holds frame `at`, seeking first unless
    /// it's a little way on from where the stream is.
    fn decode_to(&mut self, at: usize) -> Result<(), AudioError> {
        let Data::Encoded(file, stamps) = &self.audio.data else {
            return Ok(());
        };
        let format = self.audio.format.expect("an encoded sound has a format");
        let layout = Layout {
            rate: self.audio.sample_rate,
            channels: self.audio.channels,
        };
        let channels = usize::from(layout.channels);
        let reach = layout.rate as usize / DECODE_ON;
        let on = self.stream.is_some() && at >= self.next && at - self.next <= reach;
        // After a seek, packets start where their timestamps say; reading
        // on, each starts where the one before ended. The seek goes a
        // reach early: the packet it lands on only primes the decoder, and
        // MP3 frames lean on those before them.
        let mut seeked = false;
        if !on {
            let mut stream = Stream::open(format, file.clone(), Some(layout))?;
            if at > reach {
                stream.seek(stamps.ts(at - reach))?;
                seeked = true;
            }
            self.stream = Some(stream);
            self.next = 0;
            self.window.clear();
        }
        loop {
            let stream = self.stream.as_mut().expect("opened");
            let Some(ts) = stream.next(&mut self.packet)? else {
                self.window.clear();
                return Ok(());
            };
            if seeked {
                seeked = false;
                let from = stamps.frame(ts);
                if from > at {
                    // Landed past it: read on from the start instead.
                    self.stream = Some(Stream::open(format, file.clone(), Some(layout))?);
                    continue;
                }
                self.next = from;
            }
            if self.next == 0 && self.head.is_empty() {
                self.head = self.packet.clone();
            }
            // Keep the window's last frame when it comes right before.
            let follows = self.start + self.window.len() / channels == self.next;
            let kept = if follows && self.window.len() >= channels {
                self.window.drain(..self.window.len() - channels);
                1
            } else {
                self.window.clear();
                0
            };
            self.start = self.next - kept;
            self.window.extend_from_slice(&self.packet);
            self.next += self.packet.len() / channels;
            if at < self.next {
                return Ok(());
            }
        }
    }
}
