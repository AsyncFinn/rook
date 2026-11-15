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
