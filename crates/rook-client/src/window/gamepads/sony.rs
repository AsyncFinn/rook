//! Sony's pads on Windows, read over HID: Windows.Gaming.Input, gilrs's
//! only Windows backend, doesn't list a DualSense (measured), so Rook
//! reads them as SDL does, each pad's reports parsed by
//! [`super::sony_reports`]. A thread per pad blocks reading reports; a
//! device notification marks the pad list stale, so the list is read
//! again only after a device arrives.

use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use rustc_hash::FxHashMap;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_GET_DEVICE_INTERFACE_LIST_PRESENT, CM_Get_Device_Interface_List_SizeW,
    CM_Get_Device_Interface_ListW, CM_NOTIFY_ACTION, CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL,
    CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER, CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
    CM_Register_Notification, CM_Unregister_Notification, CR_BUFFER_SMALL, CR_SUCCESS,
    HCMNOTIFICATION,
};
use windows_sys::Win32::Devices::HumanInterfaceDevice::{
    HIDD_ATTRIBUTES, HidD_GetAttributes, HidD_GetFeature, HidD_GetHidGuid,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile,
};
use windows_sys::Win32::System::IO::CancelIoEx;
use windows_sys::core::GUID;

use super::sony_reports::{Model, parse};
use super::{PadChange, PadId, PadState, SONY, STUDIO_KEYS, kind};

/// Bluetooth's HID service, in the device path of a pad paired over it.
const BLUETOOTH_HID: &str = "{00001124-0000-1000-8000-00805f9b34fb}";

/// Room for any report these pads send: Windows reads a collection's
/// longest, 547 bytes for a DualShock 4 on Bluetooth.
const REPORT_BYTES: usize = 1024;

#[derive(Debug)]
enum Report {
    State(PadState),
    Gone,
}

#[derive(Debug)]
pub(super) struct Sony {
    /// Set by the device notification; starts set, for the pads already
    /// plugged in.
    stale: Arc<AtomicBool>,
    notification: HCMNOTIFICATION,
    /// Open pads by device path, with their id and handle.
    open: FxHashMap<Vec<u16>, (u64, Arc<Handle>)>,
    next_id: u64,
    reports: Receiver<(u64, Report)>,
    sender: Sender<(u64, Report)>,
}

impl Sony {
    #[allow(unsafe_code)]
    pub(super) fn new() -> std::io::Result<Self> {
        let hid = hid_class();
        let stale = Arc::new(AtomicBool::new(true));
        let mut filter = CM_NOTIFY_FILTER {
            cbSize: size_of::<CM_NOTIFY_FILTER>() as u32,
            FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
            ..Default::default()
        };
        filter.u.DeviceInterface.ClassGuid = hid;
        let mut notification = std::ptr::null_mut();
        // SAFETY: the filter is valid for the call; the context is the
        // AtomicBool `stale` holds, which outlives the registration
        // (Drop unregisters before the Arc goes).
        let result = unsafe {
            CM_Register_Notification(
                &filter,
                Arc::as_ptr(&stale).cast(),
                Some(on_arrival),
                &mut notification,
            )
        };
        if result != CR_SUCCESS {
            return Err(std::io::Error::other(format!(
                "no device notifications (CONFIGRET {result})"
            )));
        }
        let (sender, reports) = channel();
        Ok(Self {
            stale,
            notification,
            open: FxHashMap::default(),
            next_id: 0,
            reports,
            sender,
        })
    }

    pub(super) fn poll(&mut self, out: &mut Vec<PadChange>) {
        if self.stale.swap(false, Ordering::AcqRel) {
            for path in hid_paths(&hid_class()) {
                if !self.open.contains_key(&path) {
                    self.try_open(path, out);
                }
            }
        }
        while let Ok((id, report)) = self.reports.try_recv() {
            match report {
                Report::State(state) => out.push(PadChange::State(PadId::Sony(id), state)),
                Report::Gone => {
                    self.open.retain(|_, (open, _)| *open != id);
                    out.push(PadChange::Disconnected(PadId::Sony(id)));
                }
            }
        }
    }

    fn try_open(&mut self, path: Vec<u16>, out: &mut Vec<PadChange>) {
        let lower = String::from_utf16_lossy(&path).to_ascii_lowercase();
        if !lower.contains("054c") {
            return;
        }
        let Some(handle) = Handle::open(&path) else {
            return;
        };
        let Some((vendor, product)) = handle.ids() else {
            return;
        };
        let Some(model) = Model::of(product).filter(|_| vendor == SONY) else {
            return;
        };
        let bluetooth = lower.contains(BLUETOOTH_HID);
        if bluetooth {
            handle.read_feature(model.full_reports_feature());
        }
        let id = self.next_id;
        self.next_id += 1;
        let handle = Arc::new(handle);
        let reader = Arc::clone(&handle);
        let sender = self.sender.clone();
        let body = move || read_reports(&reader, model, bluetooth, id, &sender);
        if let Err(e) = rook_platform::get()
            .threads
            .spawn("rook gamepad", Box::new(body))
        {
            tracing::warn!("a PlayStation pad won't be read: {e}");
            return;
        }
        self.open.insert(path, (id, handle));
        out.push(PadChange::Connected {
            id: PadId::Sony(id),
            kind: kind(vendor, product),
            supported: STUDIO_KEYS.to_vec(),
        });
    }
}

impl Drop for Sony {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: registered in `new`; no callback runs after this
        // returns, so `stale` may go.
        unsafe { CM_Unregister_Notification(self.notification) };
        for (_, handle) in self.open.values() {
            handle.stopping.store(true, Ordering::Release);
            handle.cancel();
        }
    }
}

/// A HID device open for reading, shared by its reader thread; closed
/// when both let go.
#[derive(Debug)]
struct Handle {
    raw: HANDLE,
    /// The pad is being let go, so a failed read isn't it leaving.
    stopping: AtomicBool,
}

// SAFETY: a file handle may be used from any thread; reads happen on the
// reader's thread alone, and the others only cancel them.
#[allow(unsafe_code)]
unsafe impl Send for Handle {}
#[allow(unsafe_code)]
unsafe impl Sync for Handle {}

impl Handle {
    /// Open for reading and writing (rumble, later), or reading alone if
    /// another program keeps writing to itself.
    #[allow(unsafe_code)]
    fn open(path: &[u16]) -> Option<Self> {
        let mut name = path.to_vec();
        name.push(0);
        for access in [GENERIC_READ | GENERIC_WRITE, GENERIC_READ] {
            // SAFETY: `name` is a NUL-terminated device path; no security
            // attributes or template.
            let raw = unsafe {
                CreateFileW(
                    name.as_ptr(),
                    access,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if raw != INVALID_HANDLE_VALUE {
                return Some(Self {
                    raw,
                    stopping: AtomicBool::new(false),
                });
            }
        }
        None
    }

    #[allow(unsafe_code)]
    fn ids(&self) -> Option<(u16, u16)> {
        let mut attributes = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            VendorID: 0,
            ProductID: 0,
            VersionNumber: 0,
        };
        // SAFETY: the handle is open and the struct's size is set.
        unsafe { HidD_GetAttributes(self.raw, &mut attributes) }
            .then_some((attributes.VendorID, attributes.ProductID))
    }

    /// Read a feature report for its effect on the pad; its contents
    /// aren't needed.
    #[allow(unsafe_code)]
    fn read_feature(&self, (id, len): (u8, usize)) {
        let mut buffer = vec![0u8; len];
        buffer[0] = id;
        // SAFETY: the buffer is valid for its length.
        let read = unsafe { HidD_GetFeature(self.raw, buffer.as_mut_ptr().cast(), len as u32) };
        if !read {
            tracing::warn!("a PlayStation pad on Bluetooth may send only short reports");
        }
    }

    /// One report, or None once the pad is gone or let go.
    #[allow(unsafe_code)]
    fn read<'a>(&self, buffer: &'a mut [u8]) -> Option<&'a [u8]> {
        let mut read = 0u32;
        // SAFETY: the buffer is valid for its length; the handle isn't
        // overlapped, so the read finishes (or fails) before returning.
        let ok = unsafe {
            ReadFile(
                self.raw,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then(|| &buffer[..read as usize])
    }

    /// End a read blocked on another thread.
    #[allow(unsafe_code)]
    fn cancel(&self) {
        // SAFETY: the handle is open until every holder lets it go, and
        // this one still holds it.
        unsafe { CancelIoEx(self.raw, std::ptr::null()) };
    }
}

impl Drop for Handle {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: opened by `Handle::open` and closed only here.
        unsafe { CloseHandle(self.raw) };
    }
}

/// The reader thread: each report that changes the pad's state is sent,
/// until a read fails because the pad left or `Sony` let it go.
fn read_reports(
    handle: &Handle,
    model: Model,
    bluetooth: bool,
    id: u64,
    sender: &Sender<(u64, Report)>,
) {
    let mut buffer = vec![0u8; REPORT_BYTES];
    let mut last = None;
    while let Some(report) = handle.read(&mut buffer) {
        let Some(state) = parse(model, bluetooth, report) else {
            continue;
        };
        if last != Some(state) {
            last = Some(state);
            if sender.send((id, Report::State(state))).is_err() {
                return;
            }
        }
    }
    if !handle.stopping.load(Ordering::Acquire) {
        let _ = sender.send((id, Report::Gone));
    }
}

#[allow(unsafe_code)]
fn hid_class() -> GUID {
    let mut hid = GUID::from_u128(0);
    // SAFETY: writes the GUID it's given.
    unsafe { HidD_GetHidGuid(&mut hid) };
    hid
}

/// Every present HID device's interface path.
#[allow(unsafe_code)]
fn hid_paths(hid: &GUID) -> Vec<Vec<u16>> {
    loop {
        let mut len = 0u32;
        // SAFETY: writes the length it's given.
        let sized = unsafe {
            CM_Get_Device_Interface_List_SizeW(
                &mut len,
                hid,
                std::ptr::null(),
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        if sized != CR_SUCCESS {
            return Vec::new();
        }
        let mut list = vec![0u16; len as usize];
        // SAFETY: the buffer is valid for `len` characters.
        let listed = unsafe {
            CM_Get_Device_Interface_ListW(
                hid,
                std::ptr::null(),
                list.as_mut_ptr(),
                len,
                CM_GET_DEVICE_INTERFACE_LIST_PRESENT,
            )
        };
        match listed {
            // A device arrived between the two calls.
            CR_BUFFER_SMALL => continue,
            CR_SUCCESS => {
                return list
                    .split(|&c| c == 0)
                    .filter(|p| !p.is_empty())
                    .map(<[u16]>::to_vec)
                    .collect();
            }
            _ => return Vec::new(),
        }
    }
}

#[allow(unsafe_code)]
unsafe extern "system" fn on_arrival(
    _: HCMNOTIFICATION,
    context: *const c_void,
    action: CM_NOTIFY_ACTION,
    _: *const CM_NOTIFY_EVENT_DATA,
    _: u32,
) -> u32 {
    if action == CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL {
        // SAFETY: the context is `Sony::stale`'s AtomicBool, alive while
        // the registration is.
        let stale = unsafe { &*context.cast::<AtomicBool>() };
        stale.store(true, Ordering::Release);
    }
    0
}
