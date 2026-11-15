use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::{Device, Mix, Mixer, OutputDevices};

#[cfg(test)]
mod tests;
mod watch;

use watch::{Changes, Watch};

/// The machine's output devices now. Listing them asks the system, so
/// it is done when the game starts, not every frame.
pub fn output_devices() -> OutputDevices {
    let host = cpal::default_host();
    let names = match host.output_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(e) => {
            eprintln!("Couldn't list the output devices: {e}");
            Vec::new()
        }
    };
    OutputDevices {
        default: host.default_output_device().and_then(|d| d.name().ok()),
        names,
    }
}

/// Where sound is heard. The device follows the system: a lost one is
/// reopened, the default device is the system's default of the moment,
/// and a chosen device that goes away gives way to the default until it
/// comes back.
pub struct Output<K> {
    device: Device,
    master_volume: f32,
    /// Outlives the device it plays through, so a device that changes
    /// at the same rate carries the sounds on where they are.
    mixer: Option<Arc<Mutex<Mixer<K>>>>,
    open: Option<Open>,
    /// Opening failed (no device, or one Rook can't feed); retried only
    /// once the system's devices change.
    failed: bool,
    changes: Arc<Changes>,
    /// `None` where the system doesn't say when its devices change.
    _watch: Option<Watch>,
}

/// The device being played through.
struct Open {
    /// Kept alive while it plays; dropping it closes the device.
    _stream: cpal::Stream,
    name: Option<String>,
    /// The device went away (unplugged, disabled): the stream has stopped.
    lost: Arc<AtomicBool>,
}

impl<K> std::fmt::Debug for Output<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output")
            .field("open", &self.open.as_ref().map(|o| &o.name))
            .field("failed", &self.failed)
            .finish()
    }
}

impl<K: Hash + Eq + Copy + Send + 'static> Output<K> {
    /// No device yet: `device` opens on the first [`sync`](Self::sync)
    /// with something to hear, at `master_volume` (0 to 1).
    pub fn new(device: Device, master_volume: f32) -> Self {
        let changes = Arc::new(Changes::default());
        let watch = Watch::start(changes.clone())
            .inspect_err(|e| eprintln!("Audio won't follow device changes: {e}"))
            .ok();
        Self {
            device,
            master_volume,
            mixer: None,
            open: None,
            failed: false,
            changes,
            _watch: watch,
        }
    }

    /// Play through `device` from now on: the one open closes, and
    /// `device` opens with the next thing to hear.
    pub fn set_device(&mut self, device: Device) {
        if device == self.device {
            return;
        }
        self.device = device;
        self.open = None;
        self.failed = false;
    }

    /// The system's output devices changed since this was last asked:
    /// the list the player picks from is out of date.
    pub fn devices_changed(&self) -> bool {
        self.changes.take_listed()
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

    pub fn sync(&mut self, wanted: &Mix<K>) {
        if self.changes.take_reroute() {
            self.failed = false;
            if let Some(open) = &self.open {
                let now = resolve(&self.device).ok().map(|(_, name)| name);
                if now.as_ref() != Some(&open.name) {
                    self.open = None;
                }
            }
        }
        if self
            .open
            .as_ref()
            .is_some_and(|o| o.lost.load(Ordering::Acquire))
        {
            // Reopened right away, on what `device` resolves to now. If that
            // fails too (the system hasn't picked a new default yet), the
            // change notification that follows retries it.
            self.open = None;
        }
        if self.open.is_none() {
            let silent = wanted.sounds.is_empty() && !wanted.graph.has_output();
            if silent || self.failed {
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
        let (device, name) = resolve(&self.device)?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = usize::from(config.channels);
        let rate = config.sample_rate.0;
        let mixer = match &self.mixer {
            Some(m) if m.lock().is_ok_and(|m| m.rate() == rate) => m.clone(),
            _ => {
                let mut mixer = Mixer::new(rate);
                mixer.set_master_volume(self.master_volume);
                Arc::new(Mutex::new(mixer))
            }
        };
        let lost = Arc::new(AtomicBool::new(false));
        // Every error ends the stream (WASAPI's loop stops on its first).
        let error = {
            let lost = lost.clone();
            move |e: cpal::StreamError| {
                eprintln!("Audio output stopped: {e}");
                lost.store(true, Ordering::Release);
            }
        };
        let stream = match format {
            cpal::SampleFormat::F32 => {
                let mixer = mixer.clone();
                let mut realtime = Realtime::default();
                device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        realtime.ensure();
                        match mixer.lock() {
                            Ok(mut m) => m.mix(data, channels),
                            Err(_) => data.fill(0.0),
                        }
                    },
                    error,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let mixer = mixer.clone();
                let mut buffer = Vec::new();
                let mut realtime = Realtime::default();
                device.build_output_stream(
                    &config,
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        realtime.ensure();
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
        self.open = Some(Open {
            _stream: stream,
            name,
            lost,
        });
        Ok(())
    }
}

/// The device `wanted` plays through now, and its name.
fn resolve(wanted: &Device) -> Result<(cpal::Device, Option<String>), String> {
    let host = cpal::default_host();
    if let Device::Named(_) = wanted {
        let devices: Vec<(cpal::Device, String)> = host
            .output_devices()
            .map_err(|e| e.to_string())?
            .filter_map(|d| d.name().ok().map(|n| (d, n)))
            .collect();
        let names: Vec<&str> = devices.iter().map(|(_, n)| n.as_str()).collect();
        if let Some(name) = chosen(wanted, &names) {
            let found = devices.into_iter().find(|(_, n)| n == name);
            return found
                .map(|(d, n)| (d, Some(n)))
                .ok_or_else(|| format!("no output device named {name:?}"));
        }
    }
    let device = host.default_output_device().ok_or("no output device")?;
    let name = device.name().ok();
    Ok((device, name))
}

/// The device of `present` that `wanted` names, while it is there; `None`
/// is the system's default. A chosen device that is unplugged gives way
/// to the default, and is played through again once it is back.
/// Approximation: not compared against Roblox's own choice.
fn chosen<'a>(wanted: &'a Device, present: &[&str]) -> Option<&'a str> {
    match wanted {
        Device::Named(name) if present.contains(&name.as_str()) => Some(name),
        _ => None,
    }
}

/// The device's callback thread, raised to real-time audio priority on its
/// first callback. cpal leaves it at normal priority, where it competes
/// with the game's own threads and anything else running: with the window
/// in front Windows favors the process, but once it is tabbed out
/// the callback misses its deadlines and the sound stutters. On Windows
/// the thread joins MMCSS's "Pro Audio" task, as SDL's and PortAudio's
/// WASAPI threads do; the registration ends with the thread, when the
/// stream closes. Elsewhere the thread keeps its priority.
#[derive(Default)]
struct Realtime {
    tried: bool,
}

impl Realtime {
    #[allow(unsafe_code)]
    fn ensure(&mut self) {
        if std::mem::replace(&mut self.tried, true) {
            return;
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Threading::AvSetMmThreadCharacteristicsW;
            let task: Vec<u16> = "Pro Audio".encode_utf16().chain([0]).collect();
            let mut index = 0u32;
            // SAFETY: `task` is a NUL-terminated UTF-16 string that outlives
            // the call, and `index` is a valid u32 to write to. A failure
            // (MMCSS service stopped) returns a null handle and leaves the
            // thread at its priority.
            unsafe {
                AvSetMmThreadCharacteristicsW(task.as_ptr(), &mut index);
            }
        }
    }
}
