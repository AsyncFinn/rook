//! 2D GUI: where every GUI object is (the `Absolute*` properties scripts
//! read, laid out on the DataModel's thread as Roblox does) and what the
//! local player's ScreenGuis draw. Rules measured in Studio are in
//! `docs/behavior/gui.md`.

mod changes;
mod draw;
mod layout;

use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, Value, Vector2};

pub use layout::GUI_INSET;

use changes::Watch;
use layout::Layout;

/// Whether a property is one the layout writes, which a script's read
/// brings up to date first (measured: reading `AbsoluteSize` right after
/// setting `Size` gives the new size).
pub fn is_layout_output(name: &str) -> bool {
    matches!(
        name,
        "AbsolutePosition"
            | "AbsoluteSize"
            | "AbsoluteRotation"
            | "AbsoluteContentSize"
            | "AbsoluteCellSize"
            | "AbsoluteCellCount"
    )
}

/// A DataModel's GUI layout, kept up to date with its journal.
#[derive(Debug)]
pub struct Gui {
    /// Where in the journal changes were last looked at.
    cursor: u64,
    /// Something changed since the last layout.
    stale: bool,
    layout: Layout,
    /// Counts layouts, so the drawing knows when to rebuild.
    generation: u64,
    watch: Option<Watch>,
}

impl Default for Gui {
    fn default() -> Self {
        Self {
            cursor: 0,
            stale: true,
            layout: Layout::default(),
            generation: 0,
            watch: None,
        }
    }
}

impl Gui {
    /// Look at the journal since last time, so it can be trimmed.
    pub fn note_changes(&mut self, dom: &Dom) {
        let watch = self.watch.get_or_insert_with(|| Watch::new(dom));
        if !self.stale {
            self.stale = dom
                .journal()
                .since(self.cursor)
                .iter()
                .any(|c| watch.touches(dom, c));
        }
        self.cursor = dom.journal().end();
    }

    /// Lay out again if anything GUI changed, writing the `Absolute*`
    /// properties that moved (their Changed signals are the caller's to
    /// fire).
    pub fn sync(&mut self, dom: &mut Dom) {
        self.note_changes(dom);
        if self.stale {
            profiling::scope!("Gui::layout");
            self.layout = Layout::screens(dom, &self.layout);
            self.stale = false;
            self.generation += 1;
            self.write(dom);
        }
    }

    /// [`Gui::sync`], and lay out `id`'s tree too when it isn't on a screen
    /// (unparented, or kept in storage).
    pub fn sync_for(&mut self, dom: &mut Dom, id: InstanceId) {
        self.sync(dom);
        if !self.layout.contains(id) {
            self.layout.add_tree(dom, id);
            self.write(dom);
        }
    }

    /// Changes with every new layout.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// What the local player's ScreenGuis draw, bottom first, as of the
    /// last layout.
    pub fn draw(&self, dom: &Dom) -> Vec<GuiQuad> {
        draw::quads(dom, &self.layout)
    }

    fn write(&mut self, dom: &mut Dom) {
        let v2 = |v: glam::Vec2| Value::Vector2(Vector2(v));
        for (&id, placed) in &self.layout.placed {
            let (position, size) = if dom.is_a(id, "LayerCollector") {
                // Measured: a UIScale under a ScreenGui scales what it
                // holds, not the ScreenGui's own size.
                (placed.top_left(), placed.local)
            } else {
                (placed.top_left(), placed.size())
            };
            let _ = dom.set_property_by_name(id, "AbsolutePosition", v2(position));
            let _ = dom.set_property_by_name(id, "AbsoluteSize", v2(size));
            let _ =
                dom.set_property_by_name(id, "AbsoluteRotation", Value::Float32(placed.rotation));
        }
        for (&id, arranged) in &self.layout.arranged {
            let _ = dom.set_property_by_name(id, "AbsoluteContentSize", v2(arranged.content));
            if let Some((cell, count)) = arranged.cells {
                let _ = dom.set_property_by_name(id, "AbsoluteCellSize", v2(cell));
                let _ = dom.set_property_by_name(id, "AbsoluteCellCount", v2(count));
            }
        }
        // Our own writes aren't changes to lay out again for.
        self.cursor = dom.journal().end();
    }
}

#[cfg(test)]
mod tests;
