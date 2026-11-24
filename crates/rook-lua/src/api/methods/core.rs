//! What Roblox's own CoreScripts call: fast flags and engine features, StarterGui's handles on the GUI they draw (docs/behavior/gui/core-scripts.md), MemStorageService, and the menu's calls on the engine (docs/behavior/gui/menu.md).

use crate::api::methods::{RawMethod, first, mutate};

mod core_gui;
pub(crate) mod flags;
mod mem_storage;
mod menu;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[flags::native, core_gui::native, menu::native],
        owner,
        name,
    )
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    mem_storage::luau_source(owner, name)
}
