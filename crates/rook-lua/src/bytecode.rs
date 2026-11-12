//! Script sources compiled to Luau bytecode, kept by a hash of the source,
//! so a source compiles once per process however many scripts run it: a
//! script cloned for each player, or one module in the server and a
//! client. The content loader compiles the scripts of the models it reads
//! on its own threads ([`precompile`]), so requiring a model a script
//! loaded only looks its modules up. Browsers keep compiled scripts the
//! same way (V8's code cache, keyed by the source).

use mlua::chunk::Compiler;
use rustc_hash::FxHashMap;
use std::sync::{Arc, LazyLock, Mutex};

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
