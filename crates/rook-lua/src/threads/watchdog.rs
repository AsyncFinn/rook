//! The script timeout and heap limit: Luau code that runs too long
//! without yielding is stopped with Roblox's error, and code that grows
//! the heap past the limit with Luau's out-of-memory error.
//!
//! The interrupt runs at every call and loop back edge, so it is a plain C
//! function (an mlua closure made loops 3x slower) that reads the clock
//! and heap size every 1024th time. The heap isn't limited by mlua's
//! allocator because then mlua protects every string, table and userdata
//! the engine creates, which made each `inst.Name` read 300 ns slower.
//!
//! One of Rook's two unsafe spots (the other is in rook-content's loader):
//! it only installs the function, reads the heap size and raises errors.

#![allow(unsafe_code)]

use std::cell::Cell;
use std::ffi::c_int;
use std::time::{Duration, Instant};

use mlua::{Lua, ffi};

const TIMEOUT: &str = "Script timeout: exhausted allowed execution time";
/// Luau's own message for a failed allocation.
const OUT_OF_MEMORY: &str = "not enough memory";

thread_local! {
    /// When the Luau code running on this thread must stop. Engine code
    /// runs one VM at a time per thread, so one deadline per thread does.
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
    /// The running VM's heap limit in bytes (0: none).
    static HEAP_LIMIT: Cell<usize> = const { Cell::new(0) };
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

/// Run `f` with a deadline `timeout` from now and a heap limit of
/// `heap_limit` bytes (0: none); nested calls get their own and the outer
/// one's comes back afterwards.
pub(crate) fn guarded<R>(timeout: Duration, heap_limit: usize, f: impl FnOnce() -> R) -> R {
    let outer = DEADLINE.replace(Some(Instant::now() + timeout));
    let outer_limit = HEAP_LIMIT.replace(heap_limit);
    let r = f();
    DEADLINE.set(outer);
    HEAP_LIMIT.set(outer_limit);
    r
}

unsafe extern "C-unwind" fn interrupt(state: *mut ffi::lua_State, gc: c_int) {
    // Garbage-collection interrupts (gc >= 0) can't raise errors.
    if gc >= 0 {
        return;
    }
    let ticks = TICKS.get().wrapping_add(1);
    TICKS.set(ticks);
    if !ticks.is_multiple_of(1024) {
        return;
    }
    let message = if DEADLINE.get().is_some_and(|d| Instant::now() > d) {
        // Cleared so the error itself can unwind.
        DEADLINE.set(None);
        TIMEOUT
    } else {
        let limit = HEAP_LIMIT.get();
        // SAFETY: reading the heap size only reads the global state.
        let used = unsafe { heap_bytes(state) };
        if limit == 0 || used <= limit {
            return;
        }
        OUT_OF_MEMORY
    };
    // SAFETY: an interrupt may raise a Lua error; no Rust value with a
    // destructor is alive in this frame.
    unsafe {
        ffi::lua_pushlstring(state, message.as_ptr().cast(), message.len());
        ffi::lua_error(state);
    }
}

/// The VM's heap size in bytes.
unsafe fn heap_bytes(state: *mut ffi::lua_State) -> usize {
    // SAFETY: the caller passes a live state.
    let (kb, b) = unsafe {
        (
            ffi::lua_gc(state, ffi::LUA_GCCOUNT, 0),
            ffi::lua_gc(state, ffi::LUA_GCCOUNTB, 0),
        )
    };
    usize::try_from(kb).unwrap_or(0) * 1024 + usize::try_from(b).unwrap_or(0)
}
