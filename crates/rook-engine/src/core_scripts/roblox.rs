//! Reading Roblox's patch from an install or Studio's packages.

use rustc_hash::FxHashMap;
use std::path::Path;
use std::sync::Arc;

use super::CorePatch;

impl CorePatch {
    /// Read the patch, the server's scripts if `server_scripts` names a
    /// file, and Studio's flags if `flags` does.
    pub fn load(
        patch: &Path,
        server_scripts: Option<&Path>,
        flags: Option<&Path>,
    ) -> Result<Self, String> {
        let files = &rook_platform::get().files;
        let bytes = files
            .read(patch)
            .map_err(|e| format!("{}: {e}", patch.display()))?;
        let (dom, compiled) = Self::read(&bytes)?;
        let flags = flags
            .and_then(|path| files.read(path).ok())
            .and_then(|text| {
                serde_json::from_slice::<FxHashMap<String, serde_json::Value>>(&text).ok()
            })
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) => (k, s),
                other => (k, other.to_string()),
            })
            .collect();
        let server_scripts = server_scripts
            .and_then(|path| files.read(path).ok())
            .and_then(|bytes| rook_formats::load(&bytes).ok())
            .map(|place| place.dom);
        Ok(Self {
            dom,
            compiled,
            flags: Arc::new(flags),
            server_scripts,
        })
    }

    /// A package file's tree and its scripts' bytecode, saved as Roblox
    /// saves it (`rook_formats::compiled_scripts`): Roblox's patch, or
    /// Rook's built one.
    #[expect(
        clippy::type_complexity,
        reason = "the pair is CorePatch's own two fields"
    )]
    pub(super) fn read(
        bytes: &[u8],
    ) -> Result<(rbx_dom_weak::WeakDom, Vec<(Vec<u32>, Arc<[u8]>)>), String> {
        let dom = rook_formats::load(bytes).map_err(|e| e.to_string())?.dom;
        let compiled = rook_formats::compiled_scripts(bytes)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|c| (c.path, Arc::from(c.bytecode)))
            .collect();
        Ok((dom, compiled))
    }
}
