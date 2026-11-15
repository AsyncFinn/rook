//! The system telling Rook its output devices changed: one plugged in or
//! out, or a new default. cpal 0.15 opens a device and never says when
//! the system's devices change, so the listener is the system's own, as
//! SDL's, FMOD's and Chromium's are.
//!
//! TODO: macOS (a CoreAudio property listener on the default output
//! device and the device list) and Linux (PulseAudio's or PipeWire's
//! sink events) have no listener yet. There a lost device still reopens
//! on the next sound, but a new default or a device coming back isn't
//! noticed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// What changed since it was last asked, each flag taken by its own reader.
#[derive(Debug, Default)]
pub(super) struct Changes {
    /// The device to play through may be another now ([`super::Output`]).
    reroute: AtomicBool,
    /// The devices to list for the player changed (the window).
    listed: AtomicBool,
}

impl Changes {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) fn changed(&self) {
        self.reroute.store(true, Ordering::Release);
        self.listed.store(true, Ordering::Release);
    }

    pub(super) fn take_reroute(&self) -> bool {
        self.reroute.swap(false, Ordering::Acquire)
    }

    pub(super) fn take_listed(&self) -> bool {
        self.listed.swap(false, Ordering::Acquire)
    }
}

#[cfg(windows)]
pub(super) use windows_listener::Watch;

#[cfg(not(windows))]
pub(super) struct Watch;

#[cfg(not(windows))]
impl Watch {
    pub(super) fn start(_: Arc<Changes>) -> Result<Self, String> {
        Err("no device-change listener on this system".into())
    }
}

#[cfg(windows)]
#[allow(unsafe_code, non_snake_case)]
mod windows_listener {
    use super::{Arc, Changes};
    use windows::Win32::Foundation::{PROPERTYKEY, RPC_E_CHANGED_MODE};
    use windows::Win32::Media::Audio::{
        DEVICE_STATE, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
        IMMNotificationClient_Impl, MMDeviceEnumerator, eConsole, eRender,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    };
    use windows::core::{PCWSTR, implement};

    #[implement(IMMNotificationClient)]
    struct Listener {
        changes: Arc<Changes>,
    }

    // Called on the system's own notification thread: each only flags the
    // change, for the game's thread to act on.
    impl IMMNotificationClient_Impl for Listener_Impl {
        fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> windows::core::Result<()> {
            self.changes.changed();
            Ok(())
        }

        fn OnDeviceAdded(&self, _: &PCWSTR) -> windows::core::Result<()> {
            self.changes.changed();
            Ok(())
        }

        fn OnDeviceRemoved(&self, _: &PCWSTR) -> windows::core::Result<()> {
            self.changes.changed();
            Ok(())
        }

        /// Only the default output for games and system sounds counts: the
        /// one cpal's `default_output_device` opens.
        fn OnDefaultDeviceChanged(
            &self,
            flow: EDataFlow,
            role: ERole,
            _: &PCWSTR,
        ) -> windows::core::Result<()> {
            if flow == eRender && role == eConsole {
                self.changes.changed();
            }
            Ok(())
        }

        /// Volume, name and format changes leave the device where it is.
        fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> windows::core::Result<()> {
            Ok(())
        }
    }

    /// Listening from [`start`](Self::start) until dropped, on the thread
    /// that started it.
    pub(in crate::output) struct Watch {
        /// Held for its drop, which stops the notifications.
        _registered: Registered,
        /// Declared last, so COM ends after `_registered` lets go of it.
        _com: Com,
    }

    struct Registered {
        enumerator: IMMDeviceEnumerator,
        client: IMMNotificationClient,
    }

    impl Drop for Registered {
        fn drop(&mut self) {
            // SAFETY: `client` was registered on this enumerator in `start`.
            let _ = unsafe {
                self.enumerator
                    .UnregisterEndpointNotificationCallback(&self.client)
            };
        }
    }

    /// COM on this thread: ended on drop when it was started here.
    struct Com {
        started: bool,
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.started {
                // SAFETY: balances the CoInitializeEx that set `started`.
                unsafe { CoUninitialize() };
            }
        }
    }

    impl Watch {
        pub(in crate::output) fn start(changes: Arc<Changes>) -> Result<Self, String> {
            // SAFETY: plain COM initialization on this thread. A thread
            // already in the multithreaded apartment refuses
            // (RPC_E_CHANGED_MODE), and COM is usable there all the same.
            let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            if init.is_err() && init != RPC_E_CHANGED_MODE {
                return Err(format!("COM didn't start: {init}"));
            }
            let com = Com {
                started: init.is_ok(),
            };
            let client: IMMNotificationClient = Listener { changes }.into();
            // SAFETY: COM is started on this thread.
            let enumerator: IMMDeviceEnumerator =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
                    .map_err(|e| e.to_string())?;
            // SAFETY: the enumerator keeps a reference to `client` until
            // `Registered` unregisters it.
            unsafe { enumerator.RegisterEndpointNotificationCallback(&client) }
                .map_err(|e| e.to_string())?;
            Ok(Self {
                _registered: Registered { enumerator, client },
                _com: com,
            })
        }
    }

    impl std::fmt::Debug for Watch {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Watch").finish_non_exhaustive()
        }
    }
}
