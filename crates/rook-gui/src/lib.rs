//! 2D GUI: where every GUI object is and what its text says (the
//! `Absolute*` and `Text*` properties scripts read, laid out on the
//! DataModel's thread as Roblox does), what the local player's ScreenGuis
//! draw, and the SurfaceGuis and BillboardGuis drawn in the world, with
//! the names and health bars over heads. Rules measured in Studio are in
//! `docs/behavior/gui/` and `docs/behavior/characters/overhead.md`.

mod changes;
mod draw;
mod hits;
mod images;
mod layout;
mod modal;
mod order;
mod scroll_input;
mod style;
mod text;
mod world;

use std::sync::Arc;

use glam::{Vec2, Vec3};
use rook_dom::{Change, Dom, DomError, InstanceId};
use rook_types::{GuiQuad, Value, Vector2, WorldCanvas, WorldCanvases, WorldPlace};
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

#[derive(Debug)]
pub struct Gui {
    cursor: u64,
    /// Everything lays out again: the screen resized.
    stale: bool,
    /// GUI that moved in the tree since the last layout: each subtree, and
    /// the top of the tree it left.
    moved: Vec<(Option<InstanceId>, Vec<InstanceId>)>,
    /// Objects changed since the last layout; their trees lay out again.
    changed: FxHashSet<InstanceId>,
    /// Objects whose own place changed since the last layout; each is
    /// placed again with what it holds, or its tree laid out again when
    /// its parent depends on where its children are.
    shifted: FxHashSet<InstanceId>,
    /// Objects whose look alone changed since they were drawn; their trees
    /// draw again.
    restyled: FxHashSet<InstanceId>,
    /// Text objects whose glyphs' look alone changed: their text lays
    /// out again on its own, then they draw again.
    retexted: FxHashSet<InstanceId>,
    layout: Layout,
    /// Counts layouts and looks, so input knows when what's under the
    /// cursor may have changed.
    laid_out: u64,
    /// Counts changes to the names and health bars over heads, which
    /// change as characters move but take no input, and the count the
    /// world GUI last drew them at.
    tag_changes: u64,
    tags_drawn_at: Option<u64>,
    watch: Option<Watch>,
    /// SurfaceGuis and BillboardGuis that have been in the Workspace, so
    /// layout needn't search it; ones that left are dropped at layout.
    in_world: FxHashSet<InstanceId>,
    /// Hints and Messages that have been in the Workspace or a player, so
    /// drawing needn't search for them; ones that left are dropped at
    /// layout.
    messages: FxHashSet<InstanceId>,
    /// The world GUIs the last [`Gui::draw_world`] drew, in its order.
    world_drawn: Vec<world::Hung>,
    /// What each of those moves with (its adornee, an Attachment's part),
    /// to the indices of those on it: a frame places again only the
    /// canvases whose adornee moved.
    world_hosts: FxHashMap<InstanceId, Vec<usize>>,
    /// Where each of those is in the list.
    world_index: FxHashMap<InstanceId, usize>,
    /// World GUIs whose canvas must be drawn again.
    world_stale: FxHashSet<InstanceId>,
    /// World GUIs laid out again or changed in their own properties since
    /// the list was made: whether each draws, and how it hangs, is read
    /// again for it alone (the list is made again only when one starts
    /// or stops drawing, or hangs on something else).
    world_rehung: FxHashSet<InstanceId>,
    /// Counts what can change which world GUIs draw or what they hang on
    /// (a layout, a GUI's own properties, anything moving in the tree),
    /// and the count [`Gui::draw_world`] last listed them at: a label
    /// changing inside one GUI lists none of them again.
    world_listed: u64,
    world_list_drawn: Option<u64>,
    /// The canvases drawn anew, by index, since they were last placed
    /// and with the list as it was then: only those, and what moved,
    /// need placing. `None`: the list changed, so all do.
    world_fresh: Option<Vec<usize>>,
    /// What the screen draws, in pieces kept until they change.
    screen: draw::Screen,
    focus: Option<InstanceId>,
    modals: modal::Modals,
    /// ScrollingFrames the wheel is moving.
    scrolling: FxHashMap<InstanceId, scroll_input::Eased>,
    turning: Turning,
    /// Each world GUI's canvas as of the last layout, once drawn; one
    /// still drawn is the same `Arc`, so a renderer can keep its copy.
    canvases: FxHashMap<InstanceId, Arc<WorldCanvas>>,
    overheads: world::overhead::Overheads,
    tags: FxHashMap<InstanceId, (world::overhead::Look, Arc<WorldCanvas>)>,
    /// The tags' canvases as last handed over, after the world GUIs'.
    tags_drawn: Vec<Arc<WorldCanvas>>,
    fonts: text::Fonts,
    glyphs: text::Glyphs,
    images: images::Images,
}

impl Default for Gui {
    fn default() -> Self {
        Self {
            cursor: 0,
            stale: true,
            moved: Vec::new(),
            changed: FxHashSet::default(),
            shifted: FxHashSet::default(),
            restyled: FxHashSet::default(),
            retexted: FxHashSet::default(),
            layout: Layout::default(),
            laid_out: 0,
            tag_changes: 0,
            tags_drawn_at: None,
            watch: None,
            in_world: FxHashSet::default(),
            messages: FxHashSet::default(),
            world_drawn: Vec::new(),
            world_hosts: FxHashMap::default(),
            world_index: FxHashMap::default(),
            world_stale: FxHashSet::default(),
            world_rehung: FxHashSet::default(),
            world_listed: 0,
            world_list_drawn: None,
            world_fresh: None,
            screen: draw::Screen::default(),
            focus: None,
            modals: modal::Modals::default(),
            scrolling: FxHashMap::default(),
            turning: Turning::default(),
            canvases: FxHashMap::default(),
            overheads: world::overhead::Overheads::default(),
            tags: FxHashMap::default(),
            tags_drawn: Vec::new(),
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

    /// A requested font file's bytes (`None`: it can't be had). Text that
    /// was waiting for a font lays out again, with its tree (its
    /// `TextBounds` and what sizes itself by them may change).
    pub fn font_arrived(&mut self, url: &str, bytes: Option<Arc<[u8]>>) {
        if self.fonts.arrived(url, bytes) {
            self.changed.extend(self.layout.waiting_texts());
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
    fn forget(&mut self, id: InstanceId) {
        for (old_top, subtree) in &mut self.moved {
            subtree.retain(|&i| i != id);
            if *old_top == Some(id) {
                *old_top = None;
            }
        }
        self.layout.drop(&FxHashSet::from_iter([id]));
        self.layout.texts.remove(&id);
        self.images.forget(id);
        self.changed.remove(&id);
        self.shifted.remove(&id);
        self.restyled.remove(&id);
        self.retexted.remove(&id);
        self.in_world.remove(&id);
        self.messages.remove(&id);
        if self.focus == Some(id) {
            self.focus = None;
        }
        self.modals.forget(id);
        self.scrolling.remove(&id);
        self.turning.remove(&id);
        self.canvases.remove(&id);
        self.world_stale.remove(&id);
        self.world_rehung.remove(&id);
        self.overheads.forget(id);
        self.tags.remove(&id);
    }

    /// Look at the journal since last time, so it can be trimmed.
    pub fn note_changes(&mut self, dom: &Dom) {
        if self.watch.is_none() {
            self.watch = Some(Watch::new(dom));
            self.modals.seed(dom);
            if let Some(ws) = dom.find_service("Workspace") {
                let found = dom.descendants(ws);
                (self.overheads).note(dom, found.iter().copied());
                self.in_world
                    .extend(found.into_iter().filter(|&id| world::is_world_gui(dom, id)));
            }
            let holders = (dom.find_service("Workspace").into_iter()).chain(
                dom.find_service("Players")
                    .map_or(Vec::new(), |p| dom.children(p).to_vec()),
            );
            let held: Vec<InstanceId> = holders.flat_map(|h| dom.children(h).to_vec()).collect();
            self.messages
                .extend(held.into_iter().filter(|&id| draw::is_message(dom, id)));
        }
        let changes = dom.journal().since(self.cursor);
        rook_types::count!(JournalReadGui, changes.len());
        for change in changes {
            if let Change::Collected(id) = change {
                self.world_listed += 1;
                self.forget(*id);
                continue;
            }
            self.modals.note(dom, change);
            if let Change::Reparented { subtree, .. } = change {
                // A GUI, or the part one hangs on, may have come or gone.
                self.world_listed += 1;
                self.overheads.note(dom, subtree.iter().copied());
                let messages = subtree
                    .iter()
                    .copied()
                    .filter(|&id| draw::is_message(dom, id));
                self.messages.extend(messages);
                let found = subtree.iter().copied();
                self.in_world
                    .extend(found.filter(|&id| world::is_world_gui(dom, id)));
            }
            if !self.stale {
                let watch = self.watch.as_ref().expect("made above");
                let touch = watch.touches(dom, change);
                match (touch, change) {
                    (Touch::Nothing, _) => {}
                    (Touch::Tree, Change::Reparented { subtree, .. }) => {
                        rook_types::count!(GuiObjectsChanged, subtree.len());
                    }
                    _ => rook_types::count!(GuiObjectsChanged, 1),
                }
                match (touch, change) {
                    (Touch::All, _) => self.stale = true,
                    (
                        Touch::Tree,
                        Change::Reparented {
                            old_ancestors,
                            subtree,
                            ..
                        },
                    ) => {
                        let old_top = layout::top_along(dom, old_ancestors.iter().copied());
                        self.moved.push((old_top, subtree.clone()));
                    }
                    (Touch::Layout, Change::Property { id, .. }) => _ = self.changed.insert(*id),
                    (Touch::Place, Change::Property { id, .. }) => _ = self.shifted.insert(*id),
                    (Touch::Look, Change::Property { id, .. }) => _ = self.restyled.insert(*id),
                    (Touch::Text, Change::Property { id, .. }) => _ = self.retexted.insert(*id),
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
        rook_types::gauge!(GuiNodes, self.layout.placed.len());
        if self.stale {
            profiling::scope!("Gui::layout");
            let workspace = dom.find_service("Workspace");
            self.in_world
                .retain(|&id| workspace.is_some_and(|ws| dom.is_ancestor_of(ws, id)));
            self.messages.retain(|&id| dom.parent(id).is_some());
            // A server draws no world GUI: it lays one out only when a
            // script reads it, as it does GUI in storage.
            let mut world: Vec<InstanceId> = match dom.local_player() {
                Some(_) => self.in_world.iter().copied().collect(),
                None => Vec::new(),
            };
            world.sort_unstable();
            let previous = (&self.layout, &self.turning);
            self.layout = Layout::screens(dom, previous, &mut self.fonts, &world);
            // The screen they're laid out on changed: everything did.
            rook_types::count!(GuiObjectsChanged, self.layout.placed.len());
            self.stale = false;
            self.moved.clear();
            self.changed.clear();
            self.shifted.clear();
            self.restyled.clear();
            self.retexted.clear();
            self.laid_out += 1;
            self.world_listed += 1;
            self.screen.forget();
            self.canvases.clear();
            self.write(dom);
            return;
        }
        let joined = self.take_moved(dom);
        let (shifted, whole) = self.layout.alone(dom, std::mem::take(&mut self.shifted));
        self.changed.extend(whole);
        // Trees not laid out (in storage, or a server's world GUI) wait for
        // a script to read them.
        let touched: Vec<InstanceId> = std::mem::take(&mut self.changed).into_iter().collect();
        let mut changed: Vec<InstanceId> = (tops(dom, touched.iter().copied().collect()))
            .into_iter()
            .filter(|top| self.layout.placed.contains_key(top))
            .chain(joined)
            .collect();
        changed.sort_unstable();
        changed.dedup();
        let relaid: FxHashSet<InstanceId> = changed.iter().copied().collect();
        if !changed.is_empty() {
            profiling::scope!("Gui::layout trees");
            (self.layout).redo(dom, &changed, (&mut self.fonts, &self.turning));
            self.write_trees(dom, &changed);
            self.redraw(dom, &changed);
            // What moved draws again with its tree's new stack; what
            // changed in place (a label's text, an image arriving) is named.
            self.repaint(dom, &touched, false);
        } else {
            // Hints and Messages are in no tree.
            let messages: Vec<InstanceId> = (touched.into_iter())
                .filter(|&id| draw::is_message(dom, id))
                .collect();
            self.repaint(dom, &messages, false);
        }
        // Those in a tree just laid out are already in place.
        let shifted: Vec<InstanceId> = (shifted.into_iter())
            .filter(|&id| layout::top_of(dom, id).is_some_and(|top| !relaid.contains(&top)))
            .collect();
        let placed: FxHashSet<InstanceId> = shifted.iter().copied().collect();
        if !shifted.is_empty() {
            profiling::scope!("Gui::layout objects");
            (self.layout).place_again(dom, &shifted, (&mut self.fonts, &self.turning));
            self.write_trees(dom, &shifted);
            self.repaint(dom, &shifted, true);
        }
        // Text laid out above already has its new look.
        let retexted: Vec<InstanceId> = (std::mem::take(&mut self.retexted).into_iter())
            .filter(|&id| {
                !placed.contains(&id)
                    && layout::top_of(dom, id).is_some_and(|top| !relaid.contains(&top))
            })
            .collect();
        if !retexted.is_empty() {
            self.layout
                .restyle_texts(dom, &retexted, (&mut self.fonts, &self.turning));
            self.restyled.extend(retexted);
        }
        let restyled: Vec<InstanceId> = std::mem::take(&mut self.restyled).into_iter().collect();
        self.repaint(dom, &restyled, false);
        // A server draws nothing, so its billboards keep the size they were
        // laid out at.
        if dom.local_player().is_none() {
            return;
        }
        let followers = || (self.layout.world.values()).filter(|(h, _)| h.follows());
        if followers().next().is_none() {
            return;
        }
        let eye = world::Eye::of(dom);
        let resized: Vec<InstanceId> = followers()
            .filter(|(h, size)| h.canvas_size(dom, eye.as_ref()) != *size)
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

    /// Take the GUI that moved in the tree out of where it was laid out,
    /// and return the trees to lay out again: those it left, and those it
    /// joined that are laid out (a screen's, or a client's world GUI).
    fn take_moved(&mut self, dom: &Dom) -> Vec<InstanceId> {
        let moved = std::mem::take(&mut self.moved);
        if moved.is_empty() {
            return Vec::new();
        }
        // What shows, and what input reaches, may have changed.
        self.laid_out += 1;
        self.world_listed += 1;
        let workspace = dom.find_service("Workspace");
        let in_workspace = |id: InstanceId| workspace.is_some_and(|ws| dom.is_ancestor_of(ws, id));
        let client = dom.local_player().is_some();
        let mut trees = FxHashSet::default();
        for (old_top, subtree) in moved {
            let ids: FxHashSet<InstanceId> = subtree.iter().copied().collect();
            self.layout.drop(&ids);
            for &id in &subtree {
                // A ScreenGui or world GUI that came or went.
                self.screen.restack(id);
                self.canvases.remove(&id);
                if draw::is_message(dom, id) {
                    self.screen.repaint_messages();
                }
            }
            self.in_world
                .retain(|&id| !ids.contains(&id) || in_workspace(id));
            self.messages
                .retain(|&id| !ids.contains(&id) || dom.parent(id).is_some());
            // The tree it left, if that's still laid out.
            trees.extend(old_top.filter(|top| self.layout.placed.contains_key(top)));
            let joined = (subtree.iter()).filter_map(|&id| layout::top_of(dom, id));
            trees.extend(joined.filter(|&top| {
                layout::on_screen(dom, top) || client && self.in_world.contains(&top)
            }));
        }
        trees.into_iter().collect()
    }

    /// Work out this frame's names and health bars over heads, on a
    /// client. `blocked(dom, lines, ignore)` says, for each line between
    /// two points, whether a part outside `ignore` is on it. They draw
    /// again when one changed.
    pub fn sync_overheads(
        &mut self,
        dom: &mut Dom,
        mut blocked: impl FnMut(&mut Dom, &[(Vec3, Vec3)], &[InstanceId]) -> Vec<bool>,
    ) {
        if self.overheads.update(dom, &mut blocked) {
            self.tag_changes += 1;
        }
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

    /// The TextBox keys type into, which shows its caret and selection:
    /// the one record of it, which `UserInputService:GetFocusedTextBox`
    /// reads too, so the caret drawn and the box scripts see never differ.
    pub fn focus(&self) -> Option<InstanceId> {
        self.focus
    }

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

    fn redraw(&mut self, dom: &Dom, tops: &[InstanceId]) {
        if tops.is_empty() {
            return;
        }
        self.laid_out += 1;
        for &top in tops {
            if world::is_world_gui(dom, top) {
                // How it hangs was read again with its layout.
                self.canvases.remove(&top);
                self.world_stale.insert(top);
                self.world_rehung.insert(top);
            } else {
                self.screen.restack(top);
            }
        }
    }

    /// Draw `ids` again, placed again on their own (`placed`, with what
    /// they hold) or only restyled; a world GUI's whole canvas.
    fn repaint(&mut self, dom: &Dom, ids: &[InstanceId], placed: bool) {
        if ids.is_empty() {
            return;
        }
        self.laid_out += 1;
        for &id in ids {
            if draw::is_message(dom, id) {
                self.screen.repaint_messages();
                continue;
            }
            match layout::top_of(dom, id) {
                Some(top) if world::is_world_gui(dom, top) => {
                    self.canvases.remove(&top);
                    self.world_stale.insert(top);
                    // Its own properties say whether it draws; what it
                    // holds only what its canvas shows.
                    if id == top {
                        self.world_rehung.insert(top);
                    }
                }
                Some(_) if placed => self.screen.placed(id),
                Some(_) => self.screen.repaint(id),
                None => {}
            }
        }
    }

    /// Changes with every new layout or look (world GUI and tags'
    /// changes are in [`Gui::update_world`]): when a renderer takes the
    /// screen's GUI again.
    pub fn generation(&self) -> u64 {
        self.laid_out
    }

    /// Changes with every new layout or look of what input can reach
    /// (not name tags).
    pub fn hit_generation(&self) -> u64 {
        self.laid_out
    }

    /// What the local player's ScreenGuis draw, bottom first, as of the
    /// last layout: in pieces, one per object, each the same `Arc` until
    /// what it draws changes, so a renderer can keep what it made of it.
    pub fn draw(&mut self, dom: &Dom) -> Arc<[Arc<[GuiQuad]>]> {
        let mut text = draw::Text {
            fonts: &mut self.fonts,
            glyphs: &mut self.glyphs,
        };
        let (layout, messages) = (&self.layout, &self.messages);
        (self.screen).draw(
            dom,
            layout,
            (&mut text, &mut self.images),
            self.focus,
            messages,
        )
    }

    /// What the shown SurfaceGuis and BillboardGuis draw, as of the last
    /// layout, then the tags over heads; [`Gui::world_places`] says where
    /// each is. The whole list, kept up to date by [`Gui::update_world`].
    pub fn draw_world(&mut self, dom: &Dom) -> Vec<Arc<WorldCanvas>> {
        self.update_world(dom, false);
        (self.world_drawn.iter())
            .map(|h| self.canvases[&h.gui].clone())
            .chain(self.tags_drawn.iter().cloned())
            .collect()
    }

    /// What changed in [`Gui::draw_world`]'s list since last time (the
    /// whole list when `all`): `None` when nothing did. Only canvases laid
    /// out or restyled since they were last drawn, and tags whose look
    /// changed, are drawn again, and which GUIs draw is worked out again
    /// only when that can have changed; otherwise only the GUIs that
    /// changed are given, by index.
    pub fn update_world(&mut self, dom: &Dom, all: bool) -> Option<WorldCanvases> {
        let mut relisted = self.world_list_drawn != Some(self.world_listed);
        let mut rehung: Vec<InstanceId> = self.world_rehung.drain().collect();
        rehung.sort_unstable();
        if !relisted {
            // Read again for the GUIs that changed alone; any that starts or
            // stops drawing, or hangs on something else, lists them all.
            for gui in rehung {
                let laid = self.layout.world.get(&gui).map(|(h, _)| h);
                match (self.world_index.get(&gui), laid) {
                    (Some(&i), Some(h)) if h.shown(dom) && h.same_hosts(&self.world_drawn[i]) => {
                        self.world_drawn[i] = h.clone();
                    }
                    (None, Some(h)) if !h.shown(dom) => {}
                    (None, None) => {}
                    _ => {
                        relisted = true;
                        break;
                    }
                }
            }
        }
        if relisted {
            self.world_list_drawn = Some(self.world_listed);
            let laid = self.layout.world.values().map(|(h, _)| h);
            self.world_drawn = world::drawn(dom, laid);
            self.world_hosts.clear();
            self.world_index.clear();
            for (i, hung) in self.world_drawn.iter().enumerate() {
                self.world_index.insert(hung.gui, i);
                for host in hung.hosts(dom) {
                    self.world_hosts.entry(host).or_default().push(i);
                }
            }
            self.world_fresh = None;
        }
        rook_types::gauge!(GuiCanvases, self.world_drawn.len());
        let stale: Vec<InstanceId> = self.world_stale.drain().collect();
        let mut fresh: Vec<usize> = if relisted {
            (self.world_drawn.iter().enumerate())
                .filter(|(_, h)| !self.canvases.contains_key(&h.gui))
                .map(|(i, _)| i)
                .collect()
        } else {
            (stale.iter())
                .filter_map(|g| self.world_index.get(g).copied())
                .filter(|&i| !self.canvases.contains_key(&self.world_drawn[i].gui))
                .collect()
        };
        fresh.sort_unstable();
        let missing: Vec<InstanceId> = fresh.iter().map(|&i| self.world_drawn[i].gui).collect();
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
        let drawn: Vec<Arc<WorldCanvas>> = drawn.into_iter().map(Arc::new).collect();
        self.canvases
            .extend(missing.iter().copied().zip(drawn.iter().cloned()));
        if let Some(waiting) = &mut self.world_fresh {
            waiting.extend(&fresh);
        }
        // The tags, drawn again when one's look changed.
        let world_len = self.world_drawn.len();
        let mut changed: Vec<(usize, Arc<WorldCanvas>)> = Vec::new();
        let old_tags = self.tags_drawn.len();
        if self.tags_drawn_at != Some(self.tag_changes) {
            self.tags_drawn_at = Some(self.tag_changes);
            let mut kept = std::mem::take(&mut self.tags);
            let mut tags = Vec::with_capacity(self.overheads.shown.len());
            for tag in &self.overheads.shown {
                let canvas = match kept.remove(&tag.humanoid) {
                    Some((look, canvas)) if look == tag.look => canvas,
                    _ => draw::overhead::canvas(&tag.look, &mut text),
                };
                self.tags
                    .insert(tag.humanoid, (tag.look.clone(), canvas.clone()));
                tags.push(canvas);
            }
            for (j, canvas) in tags.iter().enumerate() {
                if self
                    .tags_drawn
                    .get(j)
                    .is_none_or(|c| !Arc::ptr_eq(c, canvas))
                {
                    changed.push((world_len + j, canvas.clone()));
                }
            }
            self.tags_drawn = tags;
        }
        if relisted || all {
            let list = (self.world_drawn.iter())
                .map(|h| self.canvases[&h.gui].clone())
                .chain(self.tags_drawn.iter().cloned())
                .collect();
            return Some(WorldCanvases::List(list));
        }
        let len = world_len + self.tags_drawn.len();
        let mut canvases: Vec<(usize, Arc<WorldCanvas>)> = fresh.into_iter().zip(drawn).collect();
        canvases.extend(changed);
        if canvases.is_empty() && self.tags_drawn.len() == old_tags {
            return None;
        }
        Some(WorldCanvases::Changed { len, canvases })
    }

    /// Where each canvas the last [`Gui::draw_world`] gave is now (`None`:
    /// it can't be drawn, its part gone).
    pub fn world_places(&self, dom: &Dom) -> Vec<Option<WorldPlace>> {
        rook_types::count!(GuiCanvasesPlaced, self.world_drawn.len());
        (self.world_drawn.iter().map(|h| h.place(dom)))
            .chain(self.tag_places())
            .collect()
    }

    /// Where the canvases the last [`Gui::draw_world`] gave whose adornee
    /// is among `moved` are now, by index in its list, then every tag over
    /// a head (each follows its Humanoid every frame). `moved` names what
    /// may be somewhere new: parts drawn somewhere new, resized or gone,
    /// and Attachments changed or moved in the tree.
    pub fn world_places_moved(
        &mut self,
        dom: &Dom,
        moved: impl IntoIterator<Item = InstanceId>,
    ) -> Vec<(usize, Option<WorldPlace>)> {
        self.places_at(dom, moved, Vec::new())
    }

    /// The places a renderer needs after [`Gui::draw_world`], by index in
    /// its list: every canvas when the list changed since they were last
    /// asked for (or with `all`, for a renderer that has none of them),
    /// else those drawn anew since and those [`Gui::world_places_moved`]
    /// gives. A canvas kept from the list before keeps its place.
    pub fn world_places_drawn(
        &mut self,
        dom: &Dom,
        moved: impl IntoIterator<Item = InstanceId>,
        all: bool,
    ) -> Vec<(usize, Option<WorldPlace>)> {
        match self.world_fresh.replace(Vec::new()).filter(|_| !all) {
            Some(fresh) => self.places_at(dom, moved, fresh),
            None => self.world_places(dom).into_iter().enumerate().collect(),
        }
    }

    /// [`Gui::world_places_moved`], and the canvases at `hit` too.
    fn places_at(
        &mut self,
        dom: &Dom,
        moved: impl IntoIterator<Item = InstanceId>,
        mut hit: Vec<usize>,
    ) -> Vec<(usize, Option<WorldPlace>)> {
        for id in moved {
            let Some(on) = self.world_hosts.get(&id) else {
                continue;
            };
            hit.extend(on);
            // An Attachment put in another part: its GUIs move with that
            // one from now on (the old part's entry only places them
            // again for nothing until the next draw).
            if dom.class_name(id) == "Attachment"
                && let Some(part) = dom.parent(id)
            {
                let on = on.clone();
                let list = self.world_hosts.entry(part).or_default();
                for i in on {
                    if !list.contains(&i) {
                        list.push(i);
                    }
                }
            }
        }
        hit.sort_unstable();
        hit.dedup();
        rook_types::count!(GuiCanvasesPlaced, hit.len());
        let tags = self.world_drawn.len();
        let world = hit.into_iter().map(|i| (i, self.world_drawn[i].place(dom)));
        world
            .chain((self.tag_places()).enumerate().map(|(i, p)| (tags + i, p)))
            .collect()
    }

    fn tag_places(&self) -> impl Iterator<Item = Option<WorldPlace>> + '_ {
        (self.overheads.shown.iter()).map(|tag| Some(tag.place(draw::overhead::size(&tag.look))))
    }

    fn write(&mut self, dom: &mut Dom) {
        let ids: Vec<InstanceId> = (self.layout.placed.keys())
            .chain(self.layout.arranged.keys())
            .copied()
            .collect();
        self.write_ids(dom, &ids);
    }

    fn write_trees(&mut self, dom: &mut Dom, tops: &[InstanceId]) {
        let ids: Vec<InstanceId> = (tops.iter())
            .flat_map(|&id| std::iter::once(id).chain(dom.descendants(id)))
            .collect();
        self.write_ids(dom, &ids);
    }

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
                // Measured: a size or place with a NaN in it (a script's
                // 0/0 scale, a NaN UIScale) reads as nothing at the
                // screen's top left, inset or not, so laying it out again
                // changes nothing. NaN never equals itself, so writing it
                // would fire Changed on every layout, and scripts that
                // answer by setting the size again loop until the
                // re-entrancy limit.
                let (at, size) = if at.is_nan() || size.is_nan() {
                    let at = if at.is_nan() {
                        Vec2::new(0.0, -GUI_INSET)
                    } else {
                        at
                    };
                    (at, if size.is_nan() { Vec2::ZERO } else { size })
                } else {
                    (at, size)
                };
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
                    // Measured: a NaN-sized scroller's canvas reads 0
                    // where it is NaN. A NaN position (one a script asked
                    // for) is left as asked, not written back every layout.
                    let real = |v: Vec2| Vec2::select(v.is_nan_mask(), Vec2::ZERO, v);
                    let canvas = v2(real(scroll.canvas * k));
                    let _ = dom.set_property_by_name(id, "AbsoluteCanvasSize", canvas);
                    let window = v2(real(scroll.window * k));
                    let _ = dom.set_property_by_name(id, "AbsoluteWindowSize", window);
                    // Measured: a canvas that shrank pulls the position back.
                    let asked = dom.vector2_property(id, "CanvasPosition").map(|v| v.0);
                    if !scroll.position.is_nan() && asked != Some(scroll.position) {
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
