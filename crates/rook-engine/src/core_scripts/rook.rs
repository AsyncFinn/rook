//! Rook's own CoreScripts: the Luau in `crates/rook-engine/corescripts`,
//! built into [`FILE`] (`cargo xtask core-scripts`, and `cargo xtask
//! package` for releases), which sits beside the exe as Roblox's
//! `DataModelPatch.rbxm` sits in its version folder. A build with no such
//! file reads the folder it was built from, so editing a script during
//! development takes a restart, not a rebuild. See
//! docs/behavior/gui/core-scripts.md#rooks-own.
//!
//! The file has the patch's shape ([`tree`]), so [`CorePatch::install`]
//! runs either one; its scripts are source, compiled when it loads.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use rbx_dom_weak::{InstanceBuilder, WeakDom, types::Ref};
use rustc_hash::FxHashMap;

use super::CorePatch;

/// The built package's name beside the exe.
pub const FILE: &str = "RookCoreScripts.rbxm";

/// The package's source folder in the build that made this exe.
pub const SOURCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/corescripts");

/// Where each source folder goes in the patch's shape: the entry scripts
/// `AddCoreScriptLocal` starts, the packages, and everything else as
/// `CoreGui.RobloxGui.Modules`.
const SCRIPTS: &str = "CoreScripts";
const PACKAGES: &str = "Packages";

impl CorePatch {
    /// Rook's CoreScripts, loaded once a run and shared by its clients:
    /// [`FILE`] beside the exe, else the [`SOURCE`] folder. `None` (and an
    /// error logged) if neither loads.
    pub(crate) fn rook() -> Option<Arc<Self>> {
        static PACKAGE: OnceLock<Option<Arc<CorePatch>>> = OnceLock::new();
        PACKAGE
            .get_or_init(|| match Self::load_rook() {
                Ok(package) => Some(Arc::new(package)),
                Err(e) => {
                    tracing::error!("Rook's CoreScripts: {e}");
                    None
                }
            })
            .clone()
    }

    fn load_rook() -> Result<Self, String> {
        let files = &rook_platform::get().files;
        let built = std::env::current_exe()
            .ok()
            .and_then(|exe| Some(exe.parent()?.join(FILE)))
            .filter(|path| files.exists(path));
        let dom = match built {
            Some(path) => {
                let bytes = files
                    .read(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                rook_formats::load(&bytes)
                    .map_err(|e| format!("{}: {e}", path.display()))?
                    .dom
            }
            None => tree(Path::new(SOURCE))?,
        };
        Self::from_source(dom)
    }

    /// A package whose scripts hold source: each is compiled now, as
    /// Roblox's game code is (optimization level 2), across the workers.
    pub(crate) fn from_source(dom: WeakDom) -> Result<Self, String> {
        let mut scripts = Vec::new();
        for (i, &top) in dom.root().children().iter().enumerate() {
            let index = u32::try_from(i).map_err(|e| e.to_string())?;
            collect(&dom, top, &[index], &mut scripts);
        }
        let compiled = rook_platform::get()
            .workers
            .map(&scripts, |(path, name, source)| {
                mlua::chunk::Compiler::new()
                    .set_optimization_level(2)
                    .compile(source)
                    .map(|code| (path.clone(), Arc::<[u8]>::from(code)))
                    .map_err(|e| format!("{name}: {e}"))
            });
        let compiled = compiled.into_iter().collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            dom,
            compiled,
            flags: Arc::new(FxHashMap::default()),
            server_scripts: None,
        })
    }
}

/// Every script under `id` (and `id` itself): its place in the tree, its
/// full name for errors, and its source.
fn collect(dom: &WeakDom, id: Ref, path: &[u32], out: &mut Vec<(Vec<u32>, String, String)>) {
    let Some(instance) = dom.get_by_ref(id) else {
        return;
    };
    if let Some(rbx_dom_weak::types::Variant::String(source)) =
        instance.properties.get(&"Source".into())
    {
        out.push((path.to_vec(), full_name(dom, id), source.clone()));
    }
    for (i, &child) in instance.children().iter().enumerate() {
        let mut at = path.to_vec();
        at.push(u32::try_from(i).unwrap_or(u32::MAX));
        collect(dom, child, &at, out);
    }
}

fn full_name(dom: &WeakDom, mut id: Ref) -> String {
    let mut names = Vec::new();
    while let Some(instance) = dom.get_by_ref(id) {
        if instance.parent().is_none() {
            break;
        }
        names.push(instance.name.clone());
        id = instance.parent();
    }
    names.reverse();
    names.join(".")
}

/// The package in `dir` as Roblox's patch lays out its own:
///
/// - `PatchRoot.CoreScripts`: `dir/CoreScripts`'s files, the scripts
///   `ScriptContext:AddCoreScriptLocal` starts, named `CoreScripts/<Name>`
///   as Roblox's are, but `StarterScript`, which a client starts itself.
/// - `PatchRoot.DataModelInstances.CorePackages.Packages`: `dir/Packages`,
///   which a client puts in `CorePackages`.
/// - `PatchRoot.DataModelInstances.CoreGui.RobloxGui.Modules`: every other
///   folder (the pieces, `Components`, `CoreApi`).
///
/// In a folder, `<name>.lua` (or `.luau`) is a ModuleScript `<name>`, and a
/// folder is one too if it holds an `init.lua`, whose source it takes;
/// otherwise it's a Folder. Other files (licenses, `packages.toml`) are
/// left out. Names sort, so the same folder always builds the same file.
pub fn tree(dir: &Path) -> Result<WeakDom, String> {
    let entries = list(dir)?;
    let mut scripts = InstanceBuilder::new("Folder").with_name(SCRIPTS);
    let mut modules = InstanceBuilder::new("Folder").with_name("Modules");
    let mut packages = InstanceBuilder::new("Folder").with_name("Packages");
    for entry in entries {
        let name = stem(&entry.path);
        if !entry.is_file {
            match name.as_str() {
                SCRIPTS => {
                    for file in list(&entry.path)? {
                        let Some(module) = item(&file.path, file.is_file)? else {
                            continue;
                        };
                        let name = stem(&file.path);
                        let name = if name == "StarterScript" {
                            name
                        } else {
                            format!("{SCRIPTS}/{name}")
                        };
                        scripts = scripts.with_child(module.with_name(name));
                    }
                }
                PACKAGES => {
                    for package in list(&entry.path)? {
                        if let Some(item) = item(&package.path, package.is_file)? {
                            packages = packages.with_child(item);
                        }
                    }
                }
                _ => {
                    if let Some(item) = item(&entry.path, false)? {
                        modules = modules.with_child(item);
                    }
                }
            }
        }
    }
    let gui = InstanceBuilder::new("Folder")
        .with_name("CoreGui")
        .with_child(
            InstanceBuilder::new("Folder")
                .with_name("RobloxGui")
                .with_child(modules),
        );
    let instances = InstanceBuilder::new("Folder")
        .with_name("DataModelInstances")
        .with_child(gui)
        .with_child(
            InstanceBuilder::new("Folder")
                .with_name("CorePackages")
                .with_child(packages),
        );
    let root = InstanceBuilder::new("Folder")
        .with_name("PatchRoot")
        .with_child(scripts)
        .with_child(instances);
    let mut dom = WeakDom::new(InstanceBuilder::new("DataModel"));
    let top = dom.root_ref();
    dom.insert(top, root);
    Ok(dom)
}

/// A file or folder as an instance, `None` if it's neither Luau nor a
/// folder.
fn item(path: &Path, is_file: bool) -> Result<Option<InstanceBuilder>, String> {
    let name = stem(path);
    if is_file {
        return Ok(luau(path)
            .then(|| read(path))
            .transpose()?
            .map(|source| module(&name, source)));
    }
    let mut children = Vec::new();
    let mut init = None;
    for entry in list(path)? {
        if entry.is_file && luau(&entry.path) && stem(&entry.path) == "init" {
            init = Some(read(&entry.path)?);
        } else if let Some(child) = item(&entry.path, entry.is_file)? {
            children.push(child);
        }
    }
    let builder = match init {
        Some(source) => module(&name, source),
        None => InstanceBuilder::new("Folder").with_name(name),
    };
    Ok(Some(builder.with_children(children)))
}

fn module(name: &str, source: String) -> InstanceBuilder {
    InstanceBuilder::new("ModuleScript")
        .with_name(name)
        .with_property("Source", source)
}

fn luau(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("lua" | "luau")
    )
}

fn stem(path: &Path) -> String {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let name = name
        .strip_suffix(".luau")
        .or_else(|| name.strip_suffix(".lua"))
        .unwrap_or(name);
    name.to_owned()
}

fn read(path: &Path) -> Result<String, String> {
    let bytes = rook_platform::get()
        .files
        .read(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    String::from_utf8(bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// A folder's entries, sorted by name.
fn list(dir: &Path) -> Result<Vec<rook_platform::files::Entry>, String> {
    let mut entries = rook_platform::get()
        .files
        .list(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    entries.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));
    Ok(entries)
}
