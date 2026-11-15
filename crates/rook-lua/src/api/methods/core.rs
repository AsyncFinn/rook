//! What Roblox's own CoreScripts call: fast flags and engine features, and StarterGui's handles on the GUI they draw (docs/behavior/gui/core-scripts.md).

use super::*;

mod core_gui;
pub(crate) mod flags;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(&[flags::native, core_gui::native], owner, name)
}
