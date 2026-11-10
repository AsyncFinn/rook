//! The OS credential store: secrets the user saved once, kept by the
//! system rather than in a file of Rook's (the Open Cloud key).

use std::fmt::Debug;
use std::io;

pub trait Credentials: Send + Sync + Debug {
    fn get(&self, name: &str) -> io::Result<Option<String>>;
    fn set(&self, name: &str, secret: &str) -> io::Result<()>;
    /// Forget `name`'s secret; whether there was one.
    fn clear(&self, name: &str) -> io::Result<bool>;
}

#[cfg(feature = "native")]
const SERVICE: &str = "Rook";

/// The system's store through keyring: Windows Credential Manager, the
/// macOS Keychain, or the Secret Service elsewhere, opened for each use.
#[cfg(feature = "native")]
#[derive(Debug)]
pub struct System;

#[cfg(feature = "native")]
impl System {
    fn entry(name: &str) -> io::Result<keyring_core::Entry> {
        use keyring_core::api::CredentialStoreApi as _;
        #[cfg(windows)]
        let store = windows_native_keyring_store::Store::new();
        #[cfg(target_os = "macos")]
        let store = apple_native_keyring_store::keychain::Store::new();
        #[cfg(all(unix, not(target_os = "macos")))]
        let store = zbus_secret_service_keyring_store::Store::new();
        store
            .and_then(|s| s.build(SERVICE, name, None))
            .map_err(io::Error::other)
    }
}

#[cfg(feature = "native")]
impl Credentials for System {
    fn get(&self, name: &str) -> io::Result<Option<String>> {
        match Self::entry(name)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(io::Error::other(e)),
        }
    }

    fn set(&self, name: &str, secret: &str) -> io::Result<()> {
        Self::entry(name)?
            .set_password(secret)
            .map_err(io::Error::other)
    }

    fn clear(&self, name: &str) -> io::Result<bool> {
        match Self::entry(name)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring_core::Error::NoEntry) => Ok(false),
            Err(e) => Err(io::Error::other(e)),
        }
    }
}
