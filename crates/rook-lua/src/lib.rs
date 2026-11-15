//! Luau scripting for Rook.
//!
//! A [`Runtime`] is one DataModel's Luau VM plus its [`World`] (DOM,
//! signals, scheduler, output). Scripts see the Roblox API: Instance
//! userdata, datatypes, `game`, `wait`/`task`, signals and `require`.
//! See `docs/design/architecture/scripting.md#luau-bridge-rook-lua`.

mod api;
pub mod audio_api;
mod calls;
pub mod gamepads;
mod net;
pub mod runtime;
mod styles;
mod types;
pub mod user_settings;

pub use api::methods::camera::rays::{Lens, rolled, viewport_ray};
pub use api::methods::characters::movement::drives as humanoid_drives;
pub use api::methods::characters::movement::face_camera;
pub use api::methods::characters::ownership::{authority, network_owner};
pub use api::methods::characters::{
    Avatar, BuildRig, RigSource, RigType, Scales, character_root, humanoid_of_root, humanoid_root,
    root_part,
};
pub use api::methods::content::assets::AssetModel;
pub use api::methods::prompts::{Action as PromptAction, ActionKind as PromptActionKind};
pub use api::methods::tools::{Activation as ToolActivation, ToolMove};
pub use api::methods::web::requests::{WebAnswer, WebRequest};
pub use calls::input::input_object;
pub use net::{NetValue, RemoteKind, RemoteMessage};
pub use runtime::world::{
    Compiled, EffectCall, Listener, LogEntry, MessageType, SignalBehavior, SoundControl,
};

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;
use std::time::Duration;

use mlua::Lua;
use rook_dom::Dom;

use runtime::scheduler::Clock;
use runtime::threads::watchdog;
use runtime::world::World;
use runtime::{bytecode, ctx};
/// Roblox Studio's default script timeout: a script that runs this long
/// without yielding is stopped with an error.
pub const DEFAULT_SCRIPT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct Runtime {
    lua: Lua,
    world: Rc<RefCell<World>>,
    /// How long code may run without yielding.
    timeout: Cell<Duration>,
    /// The heap size in bytes past which running code stops (0: none).
    heap_limit: Cell<usize>,
}

impl Runtime {
    pub fn new(dom: Dom, clock: Clock) -> mlua::Result<Self> {
        let lua = Lua::new();
        rook_content::set_script_compiler(bytecode::precompile);
        let world = Rc::new(RefCell::new(World::new(dom, clock)));
        ctx::Ctx::install(&lua, world.clone())?;

        let debug: mlua::Table = lua.globals().get("debug")?;
        lua.set_named_registry_value("rook.traceback", debug.get::<mlua::Function>("traceback")?)?;

        types::install(&lua)?;
        api::install(&lua)?;

        watchdog::install(&lua)?;

        Ok(Self {
            lua,
            world,
            timeout: Cell::new(DEFAULT_SCRIPT_TIMEOUT),
            heap_limit: Cell::new(0),
        })
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn world(&self) -> Ref<'_, World> {
        self.world.borrow()
    }

    pub fn world_mut(&self) -> RefMut<'_, World> {
        self.world.borrow_mut()
    }

    pub fn set_script_timeout(&self, timeout: Duration) {
        self.timeout.set(timeout);
    }

    /// Stop code that grows the Luau heap past `bytes` (0: no limit)
    /// with Luau's out-of-memory error.
    pub fn set_heap_limit(&self, bytes: usize) {
        self.heap_limit.set(bytes);
    }

    fn guarded<R>(&self, f: impl FnOnce() -> R) -> R {
        watchdog::guarded(self.timeout.get(), self.heap_limit.get(), f)
    }

    pub fn take_log(&self) -> Vec<LogEntry> {
        std::mem::take(&mut self.world_mut().log)
    }
}

#[cfg(test)]
mod tests;
