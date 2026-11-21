//! Script sources compiled to Luau bytecode, kept by a hash of the source,
//! so a source compiles once per process however many scripts run it: a
//! script cloned for each player, or one module in the server and a
//! client. The content loader compiles the scripts of the models it reads
//! on its own threads ([`precompile`]), so requiring a model a script
//! loaded only looks its modules up. Browsers keep compiled scripts the
//! same way (V8's code cache, keyed by the source).

use mlua::chunk::Compiler;
use mlua::{Function, Lua, Table, Value, ffi};
use rustc_hash::FxHashMap;
use std::ffi::CString;
use std::sync::{Arc, LazyLock, Mutex};

/// Load bytecode as a function named `name` (a chunk name, `=` and the
/// script's full name) that runs in `env`.
///
/// It goes to Luau's loader itself, not through mlua's `load`, which
/// guesses from the version byte whether bytes are bytecode and takes a
/// version newer than the ones it knows for source text. Roblox compiles
/// its scripts with a newer Luau than mlua knows about, so they failed to
/// load though the VM runs them. Luau's loader checks the version and names
/// it when the VM can't run it.
#[allow(unsafe_code)]
pub(crate) fn load(lua: &Lua, code: &[u8], name: &str, env: Table) -> mlua::Result<Function> {
    let name = CString::new(name).map_err(mlua::Error::external)?;
    let mut status = 0;
    // SAFETY: `env`, the one argument, is at stack index 1. `luau_load`
    // reads `code` within its length and pushes the function or an error
    // message, which is then the only value left on the stack.
    let loaded: Value = unsafe {
        lua.exec_raw(env, |state| {
            status = ffi::luau_load(state, name.as_ptr(), code.as_ptr().cast(), code.len(), 1);
            ffi::lua_remove(state, 1);
        })
    }?;
    match loaded {
        Value::Function(f) if status == 0 => Ok(f),
        message => Err(mlua::Error::SyntaxError {
            message: message.to_string()?,
            incomplete_input: false,
        }),
    }
}

/// How game code is compiled: optimization level 2, the default in live
/// games (level 1 is Studio testing's; creator-docs Luau comments). A
/// script's `--!optimize` comment still sets its own.
pub(crate) fn compiler() -> Compiler {
    Compiler::new().set_optimization_level(2)
}

type Cache = FxHashMap<blake3::Hash, Arc<[u8]>>;

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

/// `source` compiled, from the cache or compiled now and kept; `None` if
/// it doesn't compile (loading the text then reports the error under the
/// script's name).
pub(crate) fn of(source: &str) -> Option<Arc<[u8]>> {
    let key = blake3::hash(source.as_bytes());
    if let Some(code) = CACHE.lock().expect("bytecode cache").get(&key) {
        return Some(code.clone());
    }
    profiling::scope!("compile script");
    let code: Arc<[u8]> = compiler().compile(source).ok()?.into();
    CACHE
        .lock()
        .expect("bytecode cache")
        .insert(key, code.clone());
    Some(code)
}

/// Compile `source` into the cache, unless it's there: the content
/// loader's threads do this for the models they read.
pub(crate) fn precompile(source: &str) {
    _ = of(source);
}

/// Keep `code` as `source`'s bytecode (tests: proves a script runs what
/// the cache holds).
#[cfg(test)]
pub(crate) fn keep(source: &str, code: Vec<u8>) {
    let key = blake3::hash(source.as_bytes());
    CACHE
        .lock()
        .expect("bytecode cache")
        .insert(key, code.into());
}
