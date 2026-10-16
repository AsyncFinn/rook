//! The script timeout: Luau code that runs too long without yielding is
//! stopped with Roblox's error.
//!
//! Luau calls the VM's interrupt at every call and loop back edge, so the
//! interrupt is a plain C function that counts on a thread-local and reads
//! the clock every 1024th time. An mlua interrupt closure made every loop
//! iteration three times slower and every call 2.7 times slower.
//!
//! This is one of only two unsafe spots in Rook (the other lowers a
//! thread's priority, in rook-threads): mlua's safe interrupt API is the
//! closure. It is limited to installing the function and raising the
//! timeout error from it, as mlua's own interrupt does.

#![allow(unsafe_code)]

use std::cell::Cell;
use std::ffi::c_int;
use std::time::{Duration, Instant};

use mlua::{Lua, ffi};

const MESSAGE: &str = "Script timeout: exhausted allowed execution time";

thread_local! {
    /// When the Luau code running on this thread must stop. Engine code
    /// runs one VM at a time per thread, so one deadline per thread does.
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
    /// Interrupts since the clock was last read.
    static TICKS: Cell<u32> = const { Cell::new(0) };
}

/// Install the interrupt on a VM.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    // SAFETY: the callbacks belong to the VM's global state, which the
    // function only writes one field of.
    unsafe {
        lua.exec_raw::<()>((), |state| {
            (*ffi::lua_callbacks(state)).interrupt = Some(interrupt);
        })
    }
}

/// Run `f` with a deadline `timeout` from now; nested calls get their own
/// and the outer one's comes back afterwards.
pub(crate) fn guarded<R>(timeout: Duration, f: impl FnOnce() -> R) -> R {
    let outer = DEADLINE.replace(Some(Instant::now() + timeout));
    let r = f();
    DEADLINE.set(outer);
    r
}

unsafe extern "C-unwind" fn interrupt(state: *mut ffi::lua_State, gc: c_int) {
    // Garbage-collection interrupts (gc >= 0) can't raise errors.
    if gc >= 0 {
        return;
    }
    let ticks = TICKS.get().wrapping_add(1);
    TICKS.set(ticks);
    if !ticks.is_multiple_of(1024) || !DEADLINE.get().is_some_and(|d| Instant::now() > d) {
        return;
    }
    // Cleared so the error itself can unwind.
    DEADLINE.set(None);
    // SAFETY: an interrupt may raise a Lua error; no Rust value with a
    // destructor is alive in this frame.
    unsafe {
        ffi::lua_pushlstring(state, MESSAGE.as_ptr().cast(), MESSAGE.len());
        ffi::lua_error(state);
    }
}
