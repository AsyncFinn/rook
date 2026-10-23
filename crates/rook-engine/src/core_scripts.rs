//! Roblox's own CoreScripts and CorePackages, read from the user's Studio
//! install at runtime and never shipped (docs/design/policies.md#content-and-assets).
//!
//! Studio keeps all of them, compiled, in
//! `ExtraContent/models/DataModelPatch/DataModelPatch.rbxm`; its plain
//! `.lua` copies leave out the packages under `CorePackages.Workspace`.
//! A client gets the patch's `CoreGui.RobloxGui.Modules` and
//! `CorePackages`, and `StarterScript` running as a CoreScript, which
//! starts the rest with `ScriptContext:AddCoreScriptLocal`. See
//! docs/behavior/gui/core-scripts.md.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use rbx_dom_weak::WeakDom;
use rook_dom::{Dom, InstanceId};

use crate::DataModel;

/// The patch file under an install's version folder.
pub const PATCH: &str = "ExtraContent/models/DataModelPatch/DataModelPatch.rbxm";

/// A loaded patch, shared by every client of a run.
pub struct CorePatch {
    dom: WeakDom,
    /// Each compiled script's place in the file and its bytecode.
    compiled: Vec<(Vec<u32>, Arc<[u8]>)>,
    /// Fast flags as Studio last downloaded them (its
    /// `ClientSettings/StudioAppSettings.json`), so the scripts take the
    /// paths they take in Studio.
    flags: Arc<HashMap<String, String>>,
}

impl std::fmt::Debug for CorePatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CorePatch({} scripts, {} flags)",
            self.compiled.len(),
            self.flags.len()
        )
    }
}

impl CorePatch {
    /// Read the patch, and Studio's flags if `flags` names a file.
    pub fn load(patch: &Path, flags: Option<&Path>) -> Result<Self, String> {
        let files = &rook_platform::get().files;
        let bytes = files
            .read(patch)
            .map_err(|e| format!("{}: {e}", patch.display()))?;
        let dom = rook_formats::load(&bytes).map_err(|e| e.to_string())?.dom;
        let compiled = rook_formats::compiled_scripts(&bytes)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|c| (c.path, Arc::from(c.bytecode)))
            .collect();
        let flags = flags
            .and_then(|path| files.read(path).ok())
            .and_then(|text| {
                serde_json::from_slice::<HashMap<String, serde_json::Value>>(&text).ok()
            })
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| match v {
                serde_json::Value::String(s) => (k, s),
                other => (k, other.to_string()),
            })
            .collect();
        Ok(Self {
            dom,
            compiled,
            flags: Arc::new(flags),
        })
    }

    /// Give a client DataModel the CoreGui and CorePackages, and start
    /// `StarterScript`. Problems are logged; the place runs on without
    /// them.
    pub(crate) fn install(&self, dm: &DataModel) {
        let starter = {
            let mut w = dm.runtime().world_mut();
            let w = &mut *w;
            w.fast_flags = self.flags.clone();
            let journaling = w.dom.journal().is_enabled();
            w.dom.journal_mut().set_enabled(false);
            let installed = self.build(&mut w.dom, &mut w.bytecode);
            w.dom.journal_mut().set_enabled(journaling);
            match installed {
                Ok((scripts, starter)) => {
                    w.core_scripts = Some(scripts);
                    Some(starter)
                }
                Err(e) => {
                    w.push_log(rook_lua::MessageType::Error, format!("CoreScripts: {e}"));
                    None
                }
            }
        };
        if let Some(starter) = starter {
            dm.runtime().run_script(starter);
        }
    }

    /// Build the patch into `dom`: returns the folder holding the
    /// CoreScripts (outside the DataModel, as Roblox keeps them out of
    /// scripts' reach) and the StarterScript to run.
    fn build(
        &self,
        dom: &mut Dom,
        bytecode: &mut HashMap<InstanceId, Arc<[u8]>>,
    ) -> Result<(InstanceId, InstanceId), String> {
        let missing = |what: &str| format!("the patch has no {what}");
        let holder = dom.create_by_name("Folder").map_err(|e| e.to_string())?;
        let tops = rook_formats::into_dom(&self.dom, dom, holder);
        for (path, code) in &self.compiled {
            let mut at = path.first().and_then(|&i| tops.get(i as usize).copied());
            for &i in path.iter().skip(1) {
                at = at.and_then(|id| dom.children(id).get(i as usize).copied());
            }
            if let Some(id) = at {
                bytecode.insert(id, code.clone());
            }
        }
        let root = *tops.first().ok_or_else(|| missing("PatchRoot"))?;
        let child = |dom: &Dom, parent: InstanceId, name: &str| {
            dom.find_first_child(parent, name, false)
                .ok_or_else(|| missing(name))
        };
        let scripts = child(dom, root, "CoreScripts")?;
        let instances = child(dom, root, "DataModelInstances")?;

        // CoreGui.RobloxGui is a ScreenGui (Folder in the patch) holding
        // the shared Modules.
        let core_gui = dom.get_service("CoreGui").map_err(|e| e.to_string())?;
        let patch_gui = child(dom, instances, "CoreGui")?;
        let patch_robloxgui = child(dom, patch_gui, "RobloxGui")?;
        let robloxgui = dom.create_by_name("ScreenGui").map_err(|e| e.to_string())?;
        dom.set_name(robloxgui, "RobloxGui".into())
            .map_err(|e| e.to_string())?;
        // Observed: the in-game menu's new buttons (ZIndex 1) show over its
        // older panel (higher ZIndex), so objects stack by sibling here.
        let sibling = dom
            .database()
            .enum_by_name("ZIndexBehavior")
            .and_then(|e| e.items.iter().find(|(name, _)| *name == "Sibling"))
            .map(|&(_, value)| value)
            .ok_or_else(|| missing("ZIndexBehavior.Sibling"))?;
        dom.set_property_by_name(
            robloxgui,
            "ZIndexBehavior",
            rook_types::Value::Enum(sibling),
        )
        .map_err(|e| e.to_string())?;
        for c in dom.children(patch_robloxgui).to_vec() {
            dom.set_parent(c, Some(robloxgui))
                .map_err(|e| e.to_string())?;
        }
        dom.set_parent(robloxgui, Some(core_gui))
            .map_err(|e| e.to_string())?;

        let packages = dom.get_service("CorePackages").map_err(|e| e.to_string())?;
        let patch_packages = child(dom, instances, "CorePackages")?;
        for c in dom.children(patch_packages).to_vec() {
            dom.set_parent(c, Some(packages))
                .map_err(|e| e.to_string())?;
        }

        let source = child(dom, scripts, "StarterScript")?;
        let starter = dom
            .create_by_name("CoreScript")
            .map_err(|e| e.to_string())?;
        dom.set_name(starter, "StarterScript".into())
            .map_err(|e| e.to_string())?;
        if let Some(code) = bytecode.get(&source).cloned() {
            bytecode.insert(starter, code);
        }
        dom.set_parent(starter, Some(core_gui))
            .map_err(|e| e.to_string())?;
        Ok((scripts, starter))
    }
}
