//! 2D GUI: where every GUI object is and what its text says (the
//! `Absolute*` and `Text*` properties scripts read, laid out on the
//! DataModel's thread as Roblox does), what the local player's ScreenGuis
//! draw, and the SurfaceGuis and BillboardGuis drawn in the world. Rules
//! measured in Studio are in `docs/behavior/gui.md`, `gui-text.md` and
//! `gui-world.md`.

mod changes;
mod draw;
mod layout;
mod text;
mod world;

use std::sync::Arc;

use rook_dom::{Change, Dom, InstanceId};
use rook_types::{GuiQuad, Value, Vector2, WorldCanvas, WorldPlace};
use rustc_hash::{FxHashMap, FxHashSet};

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
            | "TextBounds"
            | "TextFits"
            | "ContentText"
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
    /// SurfaceGuis and BillboardGuis that have been in the Workspace, so
    /// layout needn't search it; ones that left are dropped at layout.
    in_world: FxHashSet<InstanceId>,
    /// The world GUIs the last [`Gui::draw_world`] drew, in its order.
    world_drawn: Vec<world::Hung>,
    /// What the screen draws as of the last layout, once drawn.
    screen_drawn: Option<Vec<GuiQuad>>,
    /// Each world GUI's canvas as of the last layout, once drawn.
    canvases: FxHashMap<InstanceId, WorldCanvas>,
    fonts: text::Fonts,
    glyphs: text::Glyphs,
}

impl Default for Gui {
    fn default() -> Self {
        Self {
            cursor: 0,
            stale: true,
            layout: Layout::default(),
            generation: 0,
            watch: None,
            in_world: FxHashSet::default(),
            world_drawn: Vec::new(),
            screen_drawn: None,
            canvases: FxHashMap::default(),
            fonts: text::Fonts::default(),
            glyphs: text::Glyphs::default(),
        }
    }
}

impl Gui {
    /// Where fonts in the Roblox install are read from. Without one, text
    /// has no fonts and draws nothing.
    pub fn set_content(&mut self, resolver: Arc<rook_content::Resolver>) {
        self.fonts.set_resolver(resolver);
        self.stale = true;
    }

    /// Fonts by asset id (`rbxassetid://...`) text is waiting for, each
    /// asked for once; hand each back with [`Gui::font_arrived`].
    pub fn take_font_requests(&mut self) -> Vec<String> {
        self.fonts.take_requests()
    }

    /// A requested font file's bytes (`None`: it can't be had). Text using
    /// it lays out again.
    pub fn font_arrived(&mut self, url: &str, bytes: Option<Arc<[u8]>>) {
        if self.fonts.arrived(url, bytes) {
            self.stale = true;
        }
    }
    /// Look at the journal since last time, so it can be trimmed.
    pub fn note_changes(&mut self, dom: &Dom) {
        if self.watch.is_none() {
            self.watch = Some(Watch::new(dom));
            if let Some(ws) = dom.find_service("Workspace") {
                let found = dom.descendants(ws).into_iter();
                self.in_world
                    .extend(found.filter(|&id| world::is_world_gui(dom, id)));
            }
        }
        let watch = self.watch.as_ref().expect("made above");
        for change in dom.journal().since(self.cursor) {
            if let Change::Reparented { subtree, .. } = change {
                let found = subtree.iter().copied();
                self.in_world
                    .extend(found.filter(|&id| world::is_world_gui(dom, id)));
            }
            if !self.stale && watch.touches(dom, change) {
                self.stale = true;
            }
        }
        self.cursor = dom.journal().end();
    }

    /// Lay out again if anything GUI changed, writing the `Absolute*`
    /// properties that moved (their Changed signals are the caller's to
    /// fire). World GUIs whose canvas alone changed size (a BillboardGui
    /// sized in studs as the camera moves, a SurfaceGui sized by its part)
    /// lay out again on their own.
    pub fn sync(&mut self, dom: &mut Dom) {
        self.note_changes(dom);
        if self.stale {
            profiling::scope!("Gui::layout");
            let workspace = dom.find_service("Workspace");
            self.in_world
                .retain(|&id| workspace.is_some_and(|ws| dom.is_ancestor_of(ws, id)));
            // A server draws no world GUI: it lays one out only when a
            // script reads it, as it does GUI in storage.
            let mut world: Vec<InstanceId> = match dom.local_player() {
                Some(_) => self.in_world.iter().copied().collect(),
                None => Vec::new(),
            };
            world.sort_unstable();
            self.layout = Layout::screens(dom, &self.layout, &mut self.fonts, &world);
            self.stale = false;
            self.generation += 1;
            self.screen_drawn = None;
            self.canvases.clear();
            self.write(dom);
            return;
        }
        // A server draws nothing, so its billboards keep the size they were
        // laid out at.
        if dom.local_player().is_none() {
            return;
        }
        let eye = world::Eye::of(dom);
        let resized: Vec<InstanceId> = (self.layout.world.iter())
            .filter(|(h, size)| h.follows() && h.canvas_size(dom, eye.as_ref()) != *size)
            .map(|(h, _)| h.gui)
            .collect();
        if resized.is_empty() {
            return;
        }
        profiling::scope!("Gui::layout world");
        self.layout.redo_world(dom, &resized, &mut self.fonts);
        self.generation += 1;
        for id in &resized {
            self.canvases.remove(id);
        }
        let trees: Vec<InstanceId> = (resized.iter())
            .flat_map(|&id| std::iter::once(id).chain(dom.descendants(id)))
            .collect();
        self.write_ids(dom, &trees);
    }

    /// [`Gui::sync`], and lay out `id`'s tree too when it isn't on a screen
    /// (unparented, or kept in storage).
    pub fn sync_for(&mut self, dom: &mut Dom, id: InstanceId) {
        self.sync(dom);
        if !self.layout.contains(id) {
            self.layout.add_tree(dom, id, &mut self.fonts);
            self.screen_drawn = None;
            self.canvases.clear();
            self.write(dom);
        }
    }

    /// Changes with every new layout.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// What the local player's ScreenGuis draw, bottom first, as of the
    /// last layout.
    pub fn draw(&mut self, dom: &Dom) -> Vec<GuiQuad> {
        if let Some(drawn) = &self.screen_drawn {
            return drawn.clone();
        }
        let mut text = draw::Text {
            fonts: &mut self.fonts,
            glyphs: &mut self.glyphs,
        };
        let drawn = draw::quads(dom, &self.layout, &mut text);
        self.screen_drawn = Some(drawn.clone());
        drawn
    }

    /// What the shown SurfaceGuis and BillboardGuis draw, as of the last
    /// layout; [`Gui::world_places`] says where each is. Only canvases laid
    /// out since they were last drawn are drawn again.
    pub fn draw_world(&mut self, dom: &Dom) -> Vec<WorldCanvas> {
        let laid = self.layout.world.iter().map(|(h, _)| h);
        self.world_drawn = world::drawn(dom, laid);
        let missing: Vec<InstanceId> = (self.world_drawn.iter())
            .map(|h| h.gui)
            .filter(|id| !self.canvases.contains_key(id))
            .collect();
        let mut text = draw::Text {
            fonts: &mut self.fonts,
            glyphs: &mut self.glyphs,
        };
        let drawn = draw::world(dom, &self.layout, &missing, &mut text);
        self.canvases.extend(missing.into_iter().zip(drawn));
        (self.world_drawn.iter())
            .map(|h| self.canvases[&h.gui].clone())
            .collect()
    }

    /// Where each canvas the last [`Gui::draw_world`] gave is now (`None`:
    /// it can't be drawn, its part gone).
    pub fn world_places(&self, dom: &Dom) -> Vec<Option<WorldPlace>> {
        self.world_drawn.iter().map(|h| h.place(dom)).collect()
    }

    /// Write every laid-out object's outputs.
    fn write(&mut self, dom: &mut Dom) {
        let ids: Vec<InstanceId> = (self.layout.placed.keys())
            .chain(self.layout.arranged.keys())
            .copied()
            .collect();
        self.write_ids(dom, &ids);
    }

    /// Write the outputs of the objects among `ids` that were laid out.
    fn write_ids(&mut self, dom: &mut Dom, ids: &[InstanceId]) {
        let v2 = |v: glam::Vec2| Value::Vector2(Vector2(v));
        for &id in ids {
            if let Some(placed) = self.layout.placed.get(&id) {
                // Measured: a UIScale under a ScreenGui scales what it
                // holds, not the ScreenGui's own size.
                let size = if dom.is_a(id, "LayerCollector") {
                    placed.local
                } else {
                    placed.size()
                };
                let (at, turn) = (placed.top_left(), placed.rotation);
                let _ = dom.set_property_by_name(id, "AbsolutePosition", v2(at));
                let _ = dom.set_property_by_name(id, "AbsoluteSize", v2(size));
                let _ = dom.set_property_by_name(id, "AbsoluteRotation", Value::Float32(turn));
            }
            if let Some(arranged) = self.layout.arranged.get(&id) {
                let _ = dom.set_property_by_name(id, "AbsoluteContentSize", v2(arranged.content));
                if let Some((cell, count)) = arranged.cells {
                    let _ = dom.set_property_by_name(id, "AbsoluteCellSize", v2(cell));
                    let _ = dom.set_property_by_name(id, "AbsoluteCellCount", v2(count));
                }
            }
            if let Some(laid) = self.layout.texts.get(&id) {
                let block = &laid.block;
                let content = Value::String(block.content.clone());
                let _ = dom.set_property_by_name(id, "TextBounds", v2(block.bounds));
                let _ = dom.set_property_by_name(id, "TextFits", Value::Bool(block.fits));
                let _ = dom.set_property_by_name(id, "ContentText", content);
            }
        }
        // Our own writes aren't changes to lay out again for.
        self.cursor = dom.journal().end();
    }
}

#[cfg(test)]
mod tests;
