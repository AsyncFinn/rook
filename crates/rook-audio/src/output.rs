//! The output device.

use std::hash::Hash;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{Device, Mix, Mixer};

/// An output device, fed by a [`Mixer`].
pub struct Output<K> {
    device: Device,
    master_volume: f32,
    mixer: Option<Arc<Mutex<Mixer<K>>>>,
    /// Kept alive while the game runs; dropping it closes the device.
    stream: Option<cpal::Stream>,
    /// Opening failed (no device, or one Rook can't feed); don't retry
    /// every frame.
    failed: bool,
}

impl<K> std::fmt::Debug for Output<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("open", &self.stream.is_some())
            .field("failed", &self.failed)
            .finish()
    }
}

impl<K: Hash + Eq + Copy + Send + 'static> Output<K> {
    /// No device yet: `device` opens on the first [`sync`](Self::sync)
    /// with something to hear, at `master_volume` (0 to 1).
    pub fn new(device: Device, master_volume: f32) -> Self {
        Self {
            device,
            master_volume,
            mixer: None,
            stream: None,
            failed: false,
        }
    }

    /// Hear everything at `volume` (0 to 1) from now on.
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume;
        if let Some(mixer) = &self.mixer
            && let Ok(mut m) = mixer.lock()
        {
            m.set_master_volume(volume);
        }
    }

    /// Play exactly `wanted` from now on.
    pub fn sync(&mut self, wanted: &Mix<K>) {
        if self.mixer.is_none() {
            if (wanted.sounds.is_empty() && !wanted.graph.has_output()) || self.failed {
                return;
            }
            if let Err(e) = self.open() {
                eprintln!("Audio is off: {e}");
                self.failed = true;
                return;
            }
        }
        if let Some(mixer) = &self.mixer
            && let Ok(mut m) = mixer.lock()
        {
            m.sync(wanted);
        }
    }

    fn open(&mut self) -> Result<(), String> {
        let host = cpal::default_host();
        let device = match &self.device {
            Device::Default => host.default_output_device().ok_or("no output device")?,
            Device::Named(name) => host
                .output_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.name().is_ok_and(|n| n == *name))
                .ok_or_else(|| format!("no output device named {name:?}"))?,
        };
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = usize::from(config.channels);
        let mut mixer = Mixer::new(config.sample_rate.0);
        mixer.set_master_volume(self.master_volume);
        let mixer = Arc::new(Mutex::new(mixer));
        let error = |e: cpal::StreamError| eprintln!("Audio output error: {e}");
        let stream = match format {
            cpal::SampleFormat::F32 => {
                let mixer = mixer.clone();
                device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| match mixer.lock() {
                        Ok(mut m) => m.mix(data, channels),
                        Err(_) => data.fill(0.0),
                    },
                    error,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let mixer = mixer.clone();
                let mut buffer = Vec::new();
                device.build_output_stream(
                    &config,
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        buffer.resize(data.len(), 0.0);
                        match mixer.lock() {
                            Ok(mut m) => m.mix(&mut buffer, channels),
                            Err(_) => buffer.fill(0.0),
                        }
                        for (d, s) in data.iter_mut().zip(&buffer) {
                            *d = (s * 32767.0) as i16;
                        }
                    },
                    error,
                    None,
                )
            }
            other => return Err(format!("unsupported sample format {other}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        self.mixer = Some(mixer);
        self.stream = Some(stream);
        Ok(())
    }
}
