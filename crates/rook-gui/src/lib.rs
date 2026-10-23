//! 2D GUI: where every GUI object is and what its text says (the
//! `Absolute*` and `Text*` properties scripts read, laid out on the
//! DataModel's thread as Roblox does), what the local player's ScreenGuis
//! draw, and the SurfaceGuis and BillboardGuis drawn in the world. Rules
//! measured in Studio are in `docs/behavior/gui/`.

mod changes;
mod draw;
mod hits;
mod images;
mod layout;
mod order;
mod scroll_input;
mod style;
mod text;
mod world;

use std::sync::Arc;

use glam::Vec2;
use rook_dom::{Change, Dom, DomError, InstanceId};
use rook_types::{GuiQuad, Value, Vector2, WorldCanvas, WorldPlace};
use rustc_hash::{FxHashMap, FxHashSet};

pub use hits::WorldHit;
pub use images::GuiImage;
pub use layout::GUI_INSET;
pub use scroll_input::BarDrag;

use changes::{Touch, Watch};
use layout::{Layout, Turning};

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
            | "AbsoluteCanvasSize"
            | "AbsoluteWindowSize"
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
    /// Everything lays out again: GUI trees came or went, the screen
    /// resized, or a font arrived.
    stale: bool,
    /// Objects changed since the last layout; their trees lay out again.
    changed: FxHashSet<InstanceId>,
    /// Objects whose own place changed since the last layout; each is
    /// placed again with what it holds, or its tree laid out again when
    /// its parent depends on where its children are.
    shifted: FxHashSet<InstanceId>,
    /// Objects whose look alone changed since they were drawn; their trees
    /// draw again.
    restyled: FxHashSet<InstanceId>,
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
    screen_drawn: Option<Arc<[GuiQuad]>>,
    /// The focused TextBox, which draws its caret.
    focus: Option<InstanceId>,
    /// ScrollingFrames the wheel is moving.
    scrolling: FxHashMap<InstanceId, scroll_input::Eased>,
    /// Where each turning UIPageLayout's view is.
    turning: Turning,
    /// Each world GUI's canvas as of the last layout, once drawn; one
    /// still drawn is the same `Arc`, so a renderer can keep its copy.
    canvases: FxHashMap<InstanceId, Arc<WorldCanvas>>,
    fonts: text::Fonts,
    glyphs: text::Glyphs,
    images: images::Images,
}

impl Default for Gui {
    fn default() -> Self {
        Self {
            cursor: 0,
            stale: true,
            changed: FxHashSet::default(),
            shifted: FxHashSet::default(),
            restyled: FxHashSet::default(),
            layout: Layout::default(),
            generation: 0,
            watch: None,
            in_world: FxHashSet::default(),
            world_drawn: Vec::new(),
            screen_drawn: None,
            focus: None,
            scrolling: FxHashMap::default(),
            turning: Turning::default(),
            canvases: FxHashMap::default(),
            fonts: text::Fonts::default(),
            glyphs: text::Glyphs::default(),
            images: images::Images::default(),
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

    /// How big `text` is in `font` at `size` pixels, wrapped to `frame`'s
    /// width: `TextService:GetTextSize` (creator-docs), measured as a
    /// TextLabel would lay it out.
    pub fn text_size(
        &mut self,
        text: &str,
        font: rook_types::Font,
        size: f32,
        frame: Vec2,
    ) -> Vec2 {
        let source = text::Source::plain(text, font, size);
        text::lay_out(&mut self.fonts, &source, frame, Some(Some(frame.x))).bounds
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

    /// Images the drawing is waiting for, by content URL, each asked for
    /// once; hand each back with [`Gui::image_arrived`].
    pub fn take_image_requests(&mut self) -> Vec<String> {
        self.images.take_requests()
    }

    /// A requested image loaded (`None`: it can't be had). What shows it
    /// draws again, and its `IsLoaded` turns true.
    pub fn image_arrived(&mut self, url: &str, image: Option<GuiImage>) {
        self.changed.extend(self.images.arrived(url, image));
    }

    /// The image at `url` if it has loaded, asking for it the first time
    /// (the mouse's icon loads this way).
    pub fn image(&mut self, url: &str) -> Option<GuiImage> {
        self.images.get(url, None)
    }

    /// Set a ScrollingFrame's `CanvasPosition` as a script does: measured,
    /// it's kept within the canvas as laid out then.
    pub fn set_canvas_position(
        &mut self,
        dom: &mut Dom,
        id: InstanceId,
        at: Vec2,
    ) -> Result<(), DomError> {
        self.sync_for(dom, id);
        let at = match self.layout.scrolls.get(&id) {
            Some(scroll) => at.clamp(Vec2::ZERO, scroll.reach()),
            None => at,
        };
        dom.set_property_by_name(id, "CanvasPosition", Value::Vector2(Vector2(at)))
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
            if !self.stale {
                match (watch.touches(dom, change), change) {
                    (Touch::All, _) => self.stale = true,
                    (Touch::Layout, Change::Property { id, .. }) => _ = self.changed.insert(*id),
                    (Touch::Place, Change::Property { id, .. }) => _ = self.shifted.insert(*id),
                    (Touch::Look, Change::Property { id, .. }) => _ = self.restyled.insert(*id),
                    _ => {}
                }
            }
        }
        self.cursor = dom.journal().end();
    }

    /// Lay out again the GUI trees that changed (all of them when trees
    /// came or went), writing the `Absolute*` properties that moved (their
    /// Changed signals are the caller's to fire), and draw again those
    /// whose look alone changed. World GUIs whose canvas alone changed
    /// size (a BillboardGui sized in studs as the camera moves, a
    /// SurfaceGui sized by its part) lay out again on their own.
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
            let previous = (&self.layout, &self.turning);
            self.layout = Layout::screens(dom, previous, &mut self.fonts, &world);
            self.stale = false;
            self.changed.clear();
            self.shifted.clear();
            self.restyled.clear();
            self.generation += 1;
            self.screen_drawn = None;
            self.canvases.clear();
            self.write(dom);
            return;
        }
        let (shifted, whole) = self.layout.alone(dom, std::mem::take(&mut self.shifted));
        self.changed.extend(whole);
        // Trees not laid out (in storage, or a server's world GUI) wait for
        // a script to read them.
        let changed: Vec<InstanceId> = (tops(dom, std::mem::take(&mut self.changed)).into_iter())
            .filter(|top| self.layout.placed.contains_key(top))
            .collect();
        if !changed.is_empty() {
            profiling::scope!("Gui::layout trees");
            (self.layout).redo(dom, &changed, (&mut self.fonts, &self.turning));
            self.write_trees(dom, &changed);
            self.redraw(dom, &changed);
        }
        // Those in a tree just laid out are already in place.
        let shifted: Vec<InstanceId> = (shifted.into_iter())
            .filter(|&id| layout::top_of(dom, id).is_some_and(|top| !changed.contains(&top)))
            .collect();
        if !shifted.is_empty() {
            profiling::scope!("Gui::layout objects");
            (self.layout).place_again(dom, &shifted, (&mut self.fonts, &self.turning));
            self.write_trees(dom, &shifted);
            self.redraw(dom, &tops(dom, shifted.into_iter().collect()));
        }
        let restyled = tops(dom, std::mem::take(&mut self.restyled));
        self.redraw(dom, &restyled);
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
        (self.layout).redo(dom, &resized, (&mut self.fonts, &self.turning));
        self.write_trees(dom, &resized);
        self.redraw(dom, &resized);
    }

    /// [`Gui::sync`], and lay out `id`'s tree too when it isn't on a screen
    /// (unparented, or kept in storage).
    pub fn sync_for(&mut self, dom: &mut Dom, id: InstanceId) {
        self.sync(dom);
        if !self.layout.contains(id)
            && let Some(top) = layout::top_of(dom, id)
        {
            (self.layout).redo(dom, &[top], (&mut self.fonts, &self.turning));
            self.write_trees(dom, &[top]);
        }
    }

    /// UIPageLayout `layout`'s pages in order, as laid out now.
    pub fn pages(&mut self, dom: &mut Dom, layout: InstanceId) -> Vec<InstanceId> {
        self.sync_for(dom, layout);
        self.layout.pages.get(&layout).cloned().unwrap_or_default()
    }

    /// Show UIPageLayout `layout`'s pages from `at` (a page index,
    /// fractional while it turns), or (`None`) from its `CurrentPage`.
    pub fn turn_pages(&mut self, layout: InstanceId, at: Option<f32>) {
        let changed = match at {
            Some(at) => self.turning.insert(layout, at) != Some(at),
            None => self.turning.remove(&layout).is_some(),
        };
        if changed {
            self.changed.insert(layout);
        }
    }

    /// The TextBox that has focus, which shows its caret and selection.
    pub fn set_focus(&mut self, focus: Option<InstanceId>) {
        if self.focus != focus {
            self.restyled.extend(self.focus.into_iter().chain(focus));
            self.focus = focus;
        }
    }

    /// The character a caret put at `at` (GUI coordinates) in TextBox
    /// `id` stands before.
    pub fn caret_index_at(&mut self, dom: &mut Dom, id: InstanceId, at: Vec2) -> Option<usize> {
        self.sync(dom);
        let placed = self.layout.placed.get(&id)?;
        let block = &self.layout.texts.get(&id)?.block;
        block.index_at(placed.local_point(at))
    }

    /// Draw the trees from `tops` again, as of the current layout.
    fn redraw(&mut self, dom: &Dom, tops: &[InstanceId]) {
        if tops.is_empty() {
            return;
        }
        self.generation += 1;
        for top in tops {
            if world::is_world_gui(dom, *top) {
                self.canvases.remove(top);
            } else {
                self.screen_drawn = None;
            }
        }
    }

    /// Changes with every new layout or look.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// What the local player's ScreenGuis draw, bottom first, as of the
    /// last layout.
    pub fn draw(&mut self, dom: &Dom) -> Arc<[GuiQuad]> {
        if let Some(drawn) = &self.screen_drawn {
            return drawn.clone();
        }
        let mut text = draw::Text {
            fonts: &mut self.fonts,
            glyphs: &mut self.glyphs,
        };
        let drawn: Arc<[GuiQuad]> =
            draw::quads(dom, &self.layout, &mut text, &mut self.images, self.focus).into();
        self.screen_drawn = Some(drawn.clone());
        drawn
    }

    /// What the shown SurfaceGuis and BillboardGuis draw, as of the last
    /// layout; [`Gui::world_places`] says where each is. Only canvases laid
    /// out or restyled since they were last drawn are drawn again.
    pub fn draw_world(&mut self, dom: &Dom) -> Vec<Arc<WorldCanvas>> {
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
        let drawn = draw::world(
            dom,
            &self.layout,
            &missing,
            &mut text,
            &mut self.images,
            self.focus,
        );
        self.canvases
            .extend(missing.into_iter().zip(drawn.into_iter().map(Arc::new)));
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

    /// Write the outputs of the trees from `tops`.
    fn write_trees(&mut self, dom: &mut Dom, tops: &[InstanceId]) {
        let ids: Vec<InstanceId> = (tops.iter())
            .flat_map(|&id| std::iter::once(id).chain(dom.descendants(id)))
            .collect();
        self.write_ids(dom, &ids);
    }

    /// Write the outputs of the objects among `ids` that were laid out.
    fn write_ids(&mut self, dom: &mut Dom, ids: &[InstanceId]) {
        let v2 = |v: glam::Vec2| Value::Vector2(Vector2(v));
        let client = dom.local_player().is_some();
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
                if draw::is_image(dom, id) {
                    // A client asks for every image object's images, shown
                    // or not, so `IsLoaded` turns true as in Studio
                    // (scripts wait on it to show it).
                    if client {
                        for url in draw::all_images(dom, id).collect::<Vec<_>>() {
                            self.images.get(&url, Some(id));
                        }
                    }
                    let url = dom.content_property(id, "ImageContent").unwrap_or_default();
                    let loaded = Value::Bool(self.images.is_loaded(&url));
                    let _ = dom.set_property_by_name(id, "IsLoaded", loaded);
                }
                if let Some(scroll) = self.layout.scrolls.get(&id) {
                    let k = placed.scale;
                    let _ =
                        dom.set_property_by_name(id, "AbsoluteCanvasSize", v2(scroll.canvas * k));
                    let _ =
                        dom.set_property_by_name(id, "AbsoluteWindowSize", v2(scroll.window * k));
                    // Measured: a canvas that shrank pulls the position back.
                    let asked = dom.vector2_property(id, "CanvasPosition").map(|v| v.0);
                    if asked != Some(scroll.position) {
                        let _ = dom.set_property_by_name(id, "CanvasPosition", v2(scroll.position));
                    }
                }
            }
            // Measured: a UIPageLayout without a current page shows its
            // first.
            let first = self.layout.pages.get(&id).and_then(|p| p.first());
            if let Some(&first) = first
                && dom.ref_property(id, "CurrentPage").is_none()
            {
                let _ =
                    dom.set_property_by_name(id, "CurrentPage", Value::Ref(Some(first.to_ref())));
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

/// The tops of the GUI trees holding `ids`, each once, in id order.
fn tops(dom: &Dom, ids: FxHashSet<InstanceId>) -> Vec<InstanceId> {
    let mut tops: Vec<InstanceId> = (ids.into_iter())
        .filter_map(|id| layout::top_of(dom, id))
        .collect();
    tops.sort_unstable();
    tops.dedup();
    tops
}

#[cfg(test)]
mod tests;
