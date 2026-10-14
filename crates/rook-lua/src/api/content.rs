//! ContentProvider: `PreloadAsync` and `RequestQueueSize`.
//!
//! Scripts queue requests in the World ([`ContentRequests`]); the engine
//! resolves them off its thread and hands the results back at a fixed
//! point in the frame (`Runtime::deliver_content`), where callbacks run and
//! the waiting thread resumes. Observed in Studio 0.742 (2026-10-12; ROADMAP
//! M4 step 2):
//!
//! - On a client, `PreloadAsync(list, callback)` yields until every entry
//!   is done, calling `callback(contentId, Enum.AssetFetchStatus)` for each
//!   as it finishes (in completion order), then returns nothing. Failures
//!   that need no download came within a few frames.
//! - Instances in the list stand for their content properties and their
//!   descendants' (a Model holding a Decal reported the Decal's texture).
//! - A mesh's `MeshId` loads as a mesh: on a client a SpecialMesh whose
//!   `MeshId` was a PNG reported `Failure`, while mesh files (versions
//!   1.00 and 2.00, by path or asset id) reported `Success` (observed
//!   2026-10-13).
//! - An image property must hold an image: a mesh, a sound or a damaged
//!   PNG in a Decal's `Texture` reported `Failure` (observed 2026-10-13).
//! - Strings are loaded as images: `rbxasset://` image files succeeded,
//!   `.mesh` and `.wav` paths failed, and so did every `rbxassetid://` and
//!   asset URL string, even an image that loaded as a Decal's texture.
//!   Empty strings are skipped; a repeated entry gets a callback each time.
//! - On a server, the call never returned (30 s and more) once anything
//!   was to be fetched, even a bad string; with nothing to fetch (`{}`,
//!   `{""}`, a Folder) it returned at once.

use std::collections::HashMap;

use mlua::{Function, Lua, Thread, Value as LuaValue};
use rbx_types::VariantType;
use rook_content::{ContentRef, Want};
use rook_dom::InstanceId;
use rook_reflection::{PropertyType, database};

use super::methods::RawMethod;
use super::properties::content_view;
use crate::args::{Args, type_name};
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::world::World;

/// ContentProvider requests scripts are waiting on.
#[derive(Debug, Default)]
pub struct ContentRequests {
    /// References to fetch, by request id, since the engine last took them.
    pub(crate) outbox: Vec<(u64, ContentRef, Want)>,
    /// Requests answered without fetching, delivered with the next results.
    pub(crate) answered: Vec<(u64, bool)>,
    /// Requests not delivered yet: their preload and content string.
    pending: HashMap<u64, (u64, String)>,
    preloads: HashMap<u64, Preload>,
    next: u64,
}

#[derive(Debug)]
struct Preload {
    thread: Thread,
    callback: Option<Function>,
    remaining: usize,
}

impl ContentRequests {
    /// `ContentProvider.RequestQueueSize`: requests not finished yet
    /// (Approximation: Studio's count didn't track a single call's entries
    /// exactly).
    pub fn queue_size(&self) -> usize {
        self.pending.len()
    }

    fn id(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("ContentProvider", "PreloadAsync") => preload_async,
        _ => return super::remotes::native(owner, name),
    })
}

/// A Lua value's type as Roblox's cast errors name it.
fn cast_name(v: &LuaValue) -> String {
    match v {
        LuaValue::Integer(_) | LuaValue::Number(_) => "double".into(),
        LuaValue::Boolean(_) => "bool".into(),
        v => type_name(v),
    }
}

/// What a content property must load as (`Want::for_property`).
fn wants(w: &World, id: InstanceId, property: &str) -> Want {
    let mesh_owner = w.dom.is_a(id, "DataModelMesh")
        || w.dom.is_a(id, "MeshPart")
        || w.dom.is_a(id, "PartOperation");
    Want::for_property(property, mesh_owner)
}

/// The non-empty content strings of an instance and its descendants, with
/// what each must load as.
fn instance_content(w: &World, root: InstanceId, out: &mut Vec<(String, Want)>) {
    let db = database();
    for id in std::iter::once(root).chain(w.dom.descendants(root)) {
        let Some(class) = w.dom.get(id).map(|i| i.class) else {
            continue;
        };
        for &pid in &db.class(class).all_properties {
            let p = db.property(pid);
            let is_content = matches!(
                p.ty,
                PropertyType::Value(VariantType::Content | VariantType::ContentId)
            );
            // A legacy view (`MeshId`) is read through its `Content` one.
            if !is_content || content_view(pid).is_some() {
                continue;
            }
            if let Some(url) = w
                .dom
                .property(id, pid)
                .and_then(|v| v.content_url().filter(|u| !u.is_empty()).map(str::to_owned))
            {
                out.push((url, wants(w, id, p.name)));
            }
        }
    }
}

fn preload_async(lua: &Lua, _this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let LuaValue::Table(list) = a.get(0) else {
        return Err(ScriptError(match a.get(0) {
            LuaValue::Nil => "Argument 1 missing or nil".into(),
            v => format!("Unable to cast {} to Array", cast_name(v)),
        }));
    };
    let callback = match a.get(1) {
        LuaValue::Nil => None,
        LuaValue::Function(f) => Some(f.clone()),
        v => {
            return Err(ScriptError(format!(
                "Unable to cast {} to Function",
                cast_name(v)
            )));
        }
    };
    // (content string, whether it came as a string, what it loads as).
    let mut entries: Vec<(String, bool, Want)> = Vec::new();
    for value in list.sequence_values::<LuaValue>() {
        match value? {
            LuaValue::String(s) => {
                let s = s.to_string_lossy();
                if !s.is_empty() {
                    entries.push((s, true, Want::Image));
                }
            }
            LuaValue::UserData(ud) => {
                let Ok(id) = ud.borrow::<super::instance::LuaInstance>().map(|i| i.0) else {
                    return Err(ScriptError(format!(
                        "Unable to cast {} to ContentId",
                        type_name(&LuaValue::UserData(ud))
                    )));
                };
                let mut urls = Vec::new();
                with_world(lua, |w| instance_content(w, id, &mut urls));
                entries.extend(urls.into_iter().map(|(u, want)| (u, false, want)));
            }
            v => {
                return Err(ScriptError(format!(
                    "Unable to cast {} to ContentId",
                    cast_name(&v)
                )));
            }
        }
    }
    if entries.is_empty() {
        return Outcome::ret(lua, ());
    }
    let thread = lua.current_thread();
    with_world(lua, |w| {
        // A server never finishes (observed): the thread stays suspended.
        if !w.is_client {
            return;
        }
        let c = &mut w.content;
        let preload = c.id();
        c.preloads.insert(
            preload,
            Preload {
                thread,
                callback,
                remaining: entries.len(),
            },
        );
        for (text, as_string, want) in entries {
            let request = c.id();
            // Strings load only `rbxasset://` files (as images).
            let target =
                ContentRef::parse(&text).filter(|r| !as_string || matches!(r, ContentRef::File(_)));
            match target {
                Some(r) => c.outbox.push((request, r, want)),
                None => c.answered.push((request, false)),
            }
            c.pending.insert(request, (preload, text));
        }
    });
    Ok(Outcome::Yield)
}

/// Deliver finished requests: run each one's callback, and resume a
/// PreloadAsync when its last entry is done.
pub(crate) fn deliver(lua: &Lua, results: &[(u64, bool)]) {
    let status = |ok: bool| {
        let name = if ok { "Success" } else { "Failure" };
        let value = database()
            .enum_by_name("AssetFetchStatus")
            .and_then(|e| e.items.iter().find(|(n, _)| *n == name))
            .map_or(0, |(_, v)| *v);
        crate::types::enums::enum_item(lua, "AssetFetchStatus", value).unwrap_or(LuaValue::Nil)
    };
    for &(request, ok) in results {
        let found = with_world(lua, |w| {
            let c = &mut w.content;
            let (preload, text) = c.pending.remove(&request)?;
            let p = c.preloads.get_mut(&preload)?;
            p.remaining -= 1;
            let callback = p.callback.clone();
            let done = (p.remaining == 0)
                .then(|| c.preloads.remove(&preload))
                .flatten()
                .map(|p| p.thread);
            Some((callback, text, done))
        });
        let Some((callback, text, done)) = found else {
            continue;
        };
        if let Some(f) = callback {
            let _ = crate::threads::spawn(lua, f, (text, status(ok)));
        }
        if let Some(thread) = done {
            crate::threads::resume(lua, &thread, ());
        }
    }
}
