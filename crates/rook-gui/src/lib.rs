//! 2D GUI: where every GUI object is and what its text says (the
//! `Absolute*` and `Text*` properties scripts read, laid out on the
//! DataModel's thread as Roblox does), what the local player's ScreenGuis
//! draw, and the SurfaceGuis and BillboardGuis drawn in the world, with
//! the names and health bars over heads. Rules measured in Studio are in
//! `docs/behavior/gui/` and `docs/behavior/characters/overhead.md`.

mod changes;
mod draw;
mod images;
mod input;
mod layout;
mod order;
mod selection;
mod style;
mod sync;
mod text;
mod world;

use std::sync::Arc;

use glam::{Vec2, Vec3};
use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{Value, Vector2, WorldCanvas, WorldCanvases};
use rustc_hash::{FxHashMap, FxHashSet};

pub use images::GuiImage;
pub use input::hits::WorldHit;
pub use input::scrolling::BarDrag;
pub use layout::GUI_INSET;

use changes::Watch;
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

/// A subtree that moved in the tree holding GUI.
#[derive(Debug)]
struct Moved {
    /// The parent it left.
    left: Option<InstanceId>,
    /// The top of the tree it left.
    old_top: Option<InstanceId>,
    /// It and its descendants.
    subtree: Vec<InstanceId>,
}

#[derive(Debug)]
pub struct Gui {
    cursor: u64,
    /// Everything lays out again: the screen resized.
    stale: bool,
    /// GUI that moved in the tree since the last layout.
    moved: Vec<Moved>,
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
    /// SurfaceGuis and BillboardGuis that have been in the Workspace or
    /// the local PlayerGui, so layout needn't search them; ones that left
    /// are dropped at layout.
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
    modals: input::modal::Modals,
    /// ScrollingFrames the wheel is moving.
    scrolling: FxHashMap<InstanceId, input::scrolling::Eased>,
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
            modals: input::modal::Modals::default(),
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
    fn forget(&mut self, id: InstanceId) {
        for moved in &mut self.moved {
            moved.subtree.retain(|&i| i != id);
            if moved.old_top == Some(id) {
                moved.old_top = None;
            }
            if moved.left == Some(id) {
                moved.left = None;
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
