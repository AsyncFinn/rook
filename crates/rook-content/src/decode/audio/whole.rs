//! A whole file read at once into PCM, for sounds short enough to keep
//! decoded, and the end of an Ogg stream's pages.

use super::*;

/// Longest sound Rook decodes, in frames: an hour at 48 kHz. Bigger claims
/// are damaged headers, not music.
pub(super) const MAX_FRAMES: usize = 48_000 * 3600;

/// What decoding all of a file once found.
pub(super) struct Whole {
    pub(super) layout: Layout,
    pub(super) frames: usize,
    /// Its samples, while they took at most the bytes kept.
    pub(super) samples: Option<Vec<i16>>,
    /// At least its first [`CHUNK`] frames' samples.
    pub(super) head: Vec<i16>,
}

pub(super) fn read_whole(
    format: AudioFormat,
    file: Arc<[u8]>,
    kept: usize,
) -> Result<Whole, AudioError> {
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
pub(super) fn ogg_end(bytes: &[u8]) -> Option<usize> {
    let at = bytes.windows(4).rposition(|w| w == b"OggS")?;
    let granule = i64::from_le_bytes(bytes.get(at + 6..at + 14)?.try_into().ok()?);
    usize::try_from(granule).ok().filter(|&n| n > 0)
}
