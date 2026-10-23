//! Where every GUI object goes: its size from `Size`, `SizeConstraint`,
//! `AutomaticSize` and the size constraints; its place from `Position` and
//! `AnchorPoint`, or from a UI layout (UIListLayout with its flex,
//! UIGridLayout, UITableLayout, UIPageLayout), on a ScrollingFrame's canvas
//! if it's in one; turned about its ancestors' centers by their `Rotation`
//! and scaled by UIScales (docs/behavior/gui/layout.md#layout,
//! docs/behavior/gui/modern.md).

mod automatic;
mod constraints;
mod grid;
mod list;
mod padding;
mod page;
mod scrolling;
mod table;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::UDim2;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::text::{self, Fonts, Source, TextBlock};
use crate::world::{self, Eye, Hung};
use padding::Inner;
pub(crate) use scrolling::Scroll;

/// The top bar's height, which ScreenGuis start below unless they ignore
/// the inset (measured: 58 pixels in Studio 0.742's client, with the
/// current top bar; `GuiService:GetGuiInset()` says the same).
pub const GUI_INSET: f32 = 58.0;

/// Where a GUI object ended up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placed {
    /// Its center on the screen.
    pub(crate) center: Vec2,
    /// Its size before UIScales, which its children's `Size` and
    /// `Position` scale against.
    pub(crate) local: Vec2,
    /// The UIScales on it and its ancestors, multiplied.
    pub(crate) scale: f32,
    /// `AbsoluteRotation`, in degrees.
    pub(crate) rotation: f32,
}

impl Placed {
    /// What children of an unparented object are placed in: nothing, at
    /// the origin (measured: an unparented Frame's `AbsoluteSize` is its
    /// `Size` offsets, at (0, 0)).
    const NOWHERE: Self = Self {
        center: Vec2::ZERO,
        local: Vec2::ZERO,
        scale: 1.0,
        rotation: 0.0,
    };

    pub(crate) fn size(&self) -> Vec2 {
        self.local * self.scale
    }

    pub(crate) fn top_left(&self) -> Vec2 {
        self.center - self.size() * 0.5
    }

    /// A point given in this object's own frame (pixels from its top left,
    /// before scaling and turning), on the screen.
    pub(crate) fn screen_point(self, local: Vec2) -> Vec2 {
        self.center + turn(self.rotation, (local - self.local * 0.5) * self.scale)
    }

    /// A point on the screen in this object's own frame: the inverse of
    /// [`Placed::screen_point`].
    pub(crate) fn local_point(self, screen: Vec2) -> Vec2 {
        turn(-self.rotation, screen - self.center) / self.scale + self.local * 0.5
    }
}

/// `v` turned clockwise on the screen by `degrees` (y points down).
pub(crate) fn turn(degrees: f32, v: Vec2) -> Vec2 {
    if degrees == 0.0 {
        return v;
    }
    let (sin, cos) = degrees.to_radians().sin_cos();
    Vec2::new(v.x * cos - v.y * sin, v.x * sin + v.y * cos)
}

/// What a UIListLayout or UIGridLayout reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Arranged {
    /// `AbsoluteContentSize`.
    pub(crate) content: Vec2,
    /// A grid's `AbsoluteCellSize` and `AbsoluteCellCount`.
    pub(crate) cells: Option<(Vec2, Vec2)>,
}

/// Every placed GUI object, what each layout reports, each UIPageLayout's
/// pages in order, each text object's text, each ScrollingFrame's canvas,
/// and every world GUI with the canvas size it was laid out at.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    pub(crate) placed: FxHashMap<InstanceId, Placed>,
    pub(crate) arranged: FxHashMap<InstanceId, Arranged>,
    pub(crate) pages: FxHashMap<InstanceId, Vec<InstanceId>>,
    pub(crate) texts: FxHashMap<InstanceId, Laid>,
    pub(crate) scrolls: FxHashMap<InstanceId, Scroll>,
    pub(crate) world: Vec<(Hung, Vec2)>,
}

/// A text object's text, with what it was laid out from, so an unchanged
/// one is reused next time rather than shaped again.
#[derive(Debug)]
pub(crate) struct Laid {
    source: Source,
    /// The area inside the padding: its top left and size.
    area: (Vec2, Vec2),
    /// [`Fonts::generation`] then: a font arriving lays text out again.
    fonts: u64,
    pub(crate) block: TextBlock,
}

impl Layout {
    /// Lay out the GUI scripts see: every player's PlayerGui, StarterGui,
    /// CoreGui (what Roblox's CoreScripts draw) and the world GUIs in
    /// `world`. `previous` is the last layout, where
    /// hidden children of a UI layout stay; `turning` says where each
    /// turning UIPageLayout's view is.
    pub(crate) fn screens(
        dom: &Dom,
        (previous, turning): (&Layout, &Turning),
        fonts: &mut Fonts,
        world: &[InstanceId],
    ) -> Self {
        let mut pass = Pass::new(dom, (previous, turning), fonts, Layout::default());
        let players = dom.find_service("Players");
        let holders = players
            .into_iter()
            .flat_map(|p| dom.children(p).iter().copied())
            .filter_map(|p| dom.find_first_child_of_class(p, "PlayerGui"))
            .chain(dom.find_service("StarterGui"))
            .chain(dom.find_service("CoreGui"));
        for holder in holders {
            for root in through_folders(dom, holder, |id| dom.is_a(id, "GuiBase2d")) {
                pass.root(root);
            }
        }
        for &root in world {
            pass.root(root);
        }
        pass.out
    }

    pub(crate) fn contains(&self, id: InstanceId) -> bool {
        self.placed.contains_key(&id) || self.arranged.contains_key(&id)
    }

    /// Lay out the trees from `tops` again (a world GUI the camera
    /// resized, a tree whose properties changed), leaving the rest as it
    /// is. What `tops` hold must be what they held when last laid out.
    pub(crate) fn redo(&mut self, dom: &Dom, tops: &[InstanceId], (fonts, turning): Fonted<'_>) {
        let redone: FxHashSet<InstanceId> = tops.iter().copied().collect();
        self.world.retain(|(h, _)| !redone.contains(&h.gui));
        let previous = Layout::default();
        let mut pass = Pass::new(dom, (&previous, turning), fonts, std::mem::take(self));
        for &top in tops {
            pass.root(top);
        }
        *self = pass.out;
    }

    /// `ids` split into the objects that can be placed again on their own
    /// (laid out, their parent placing them by `Position` and `Size`
    /// alone, nothing growing to hold them), and the rest, whose trees
    /// must lay out again.
    pub(crate) fn alone(
        &self,
        dom: &Dom,
        ids: impl IntoIterator<Item = InstanceId>,
    ) -> (Vec<InstanceId>, Vec<InstanceId>) {
        let free = |id: InstanceId| {
            let Some(parent) = gui_parent(dom, id) else {
                return false;
            };
            if !self.placed.contains_key(&id)
                || !self.placed.contains_key(&parent)
                || dom.class_name(parent) == "ScrollingFrame"
                || arranger(dom, parent).is_some()
                || dom
                    .enum_item_property(parent, "AutomaticSize")
                    .is_some_and(|a| a != "None")
            {
                return false;
            }
            // A UITableLayout places its rows' cells.
            let table = gui_parent(dom, parent)
                .and_then(|g| arranger(dom, g))
                .is_some_and(|l| dom.class_name(l) == "UITableLayout");
            if table {
                return false;
            }
            // A ScrollingFrame's automatic canvas reaches what all its
            // GUI holds, up to the next ScrollingFrame in.
            let mut at = gui_parent(dom, parent);
            while let Some(a) = at {
                if dom.class_name(a) == "ScrollingFrame" {
                    return dom.enum_item_property(a, "AutomaticCanvasSize") == Some("None");
                }
                at = gui_parent(dom, a);
            }
            true
        };
        ids.into_iter().partition(|&id| free(id))
    }

    /// Place `ids` again, with what they hold, where their parents put
    /// them now; each must be one [`Layout::alone`] allows.
    pub(crate) fn place_again(
        &mut self,
        dom: &Dom,
        ids: &[InstanceId],
        (fonts, turning): Fonted<'_>,
    ) {
        let previous = Layout::default();
        let mut pass = Pass::new(dom, (&previous, turning), fonts, std::mem::take(self));
        for &id in ids {
            pass.place_alone(id);
        }
        *self = pass.out;
    }
}

/// The GUI object or LayerCollector placing `id`: its parent, past
/// Folders.
fn gui_parent(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    let mut at = dom.parent(id);
    while let Some(p) = at {
        if dom.class_name(p) != "Folder" {
            return dom.is_a(p, "GuiBase2d").then_some(p);
        }
        at = dom.parent(p);
    }
    None
}

/// The top of the GUI tree holding `id` (a GUI object, or a UI component
/// in one): its LayerCollector, or its topmost GUI ancestor.
pub(crate) fn top_of(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    let mut top = None;
    let mut at = Some(id);
    while let Some(i) = at {
        if dom.is_a(i, "GuiBase2d") {
            top = Some(i);
        } else if dom.class_name(i) != "Folder" && !dom.is_a(i, "UIComponent") {
            break;
        }
        at = dom.parent(i);
    }
    top
}

/// The window's size: the current camera's `ViewportSize`.
pub(crate) fn screen_size(dom: &Dom) -> Vec2 {
    dom.find_service("Workspace")
        .and_then(|ws| dom.ref_property(ws, "CurrentCamera"))
        .and_then(|c| dom.vector2_property(c, "ViewportSize"))
        .map_or(Vec2::ZERO, |v| v.0)
}

/// `parent`'s children that pass `keep`, with Folders' children in their
/// place (measured: a Frame in a Folder is placed in the Folder's parent).
pub(crate) fn through_folders(
    dom: &Dom,
    parent: InstanceId,
    keep: impl Fn(InstanceId) -> bool + Copy,
) -> Vec<InstanceId> {
    let mut out = Vec::new();
    for &c in dom.children(parent) {
        if dom.class_name(c) == "Folder" {
            out.extend(through_folders(dom, c, keep));
        } else if keep(c) {
            out.push(c);
        }
    }
    out
}

/// The GUI objects `parent` places.
pub(crate) fn gui_children(dom: &Dom, parent: InstanceId) -> Vec<InstanceId> {
    through_folders(dom, parent, |id| dom.is_a(id, "GuiObject"))
}

/// Where each turning UIPageLayout's view is, as a page index.
pub(crate) type Turning = FxHashMap<InstanceId, f32>;

/// The fonts text is laid out with, and where turning UIPageLayouts are.
pub(crate) type Fonted<'a> = (&'a mut Fonts, &'a Turning);

/// A child placed by a UI layout, with its size.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Kid {
    pub(crate) id: InstanceId,
    pub(crate) size: Vec2,
}

/// The UIListLayout or UIGridLayout placing `parent`'s children: its
/// first one (measured: a UIListLayout added after a UIGridLayout changes
/// nothing).
fn arranger(dom: &Dom, parent: InstanceId) -> Option<InstanceId> {
    dom.children(parent)
        .iter()
        .copied()
        .find(|&c| dom.is_a(c, "UIGridStyleLayout"))
}

/// Where a UI layout puts each kid (its top left in the parent's inner
/// area), the size it gives each, and what it reports.
pub(crate) struct Arrangement {
    pub(crate) slots: Vec<Vec2>,
    pub(crate) sizes: Vec<Vec2>,
    pub(crate) arranged: Arranged,
}

struct Pass<'a> {
    dom: &'a Dom,
    previous: &'a Layout,
    turning: &'a Turning,
    fonts: &'a mut Fonts,
    out: Layout,
    screen: Vec2,
    /// What BillboardGuis are sized from.
    eye: Option<Eye>,
    /// Cells a UITableLayout placed in their rows: top left and size.
    cells: FxHashMap<InstanceId, (Vec2, Vec2)>,
}

impl<'a> Pass<'a> {
    fn new(
        dom: &'a Dom,
        (previous, turning): (&'a Layout, &'a Turning),
        fonts: &'a mut Fonts,
        out: Layout,
    ) -> Self {
        Self {
            dom,
            previous,
            turning,
            fonts,
            out,
            screen: screen_size(dom),
            eye: Eye::of(dom),
            cells: FxHashMap::default(),
        }
    }

    /// Arrange `kids` (visible, in child order) with `layout` in `inner`.
    fn arrange(&mut self, layout: InstanceId, kids: &[Kid], inner: Vec2) -> Arrangement {
        let dom = self.dom;
        let order = list::sorted(dom, layout, kids);
        let kids: Vec<Kid> = order.iter().map(|&i| kids[i]).collect();
        let mut out = match dom.class_name(layout) {
            "UIGridLayout" => grid::arrange(dom, layout, kids.len(), inner),
            "UITableLayout" => {
                let (out, cells) =
                    table::arrange(dom, layout, &kids, inner, |c| self.size_of(c, inner));
                (self.cells).extend(cells.into_iter().map(|c| (c.id, (c.at, c.size))));
                out
            }
            "UIPageLayout" => {
                let pages: Vec<InstanceId> = kids.iter().map(|k| k.id).collect();
                let current = dom.ref_property(layout, "CurrentPage");
                let at = self.turning.get(&layout).copied().unwrap_or_else(|| {
                    let found = current.and_then(|c| pages.iter().position(|&p| p == c));
                    found.unwrap_or(0) as f32
                });
                self.out.pages.insert(layout, pages);
                page::arrange(dom, layout, &kids, inner, at)
            }
            _ => list::arrange(dom, layout, &kids, inner),
        };
        // Back to child order.
        let mut slots = vec![Vec2::ZERO; order.len()];
        let mut sizes = vec![Vec2::ZERO; order.len()];
        for (n, &i) in order.iter().enumerate() {
            slots[i] = out.slots[n];
            sizes[i] = out.sizes[n];
        }
        out.slots = slots;
        out.sizes = sizes;
        out
    }

    /// Lay out a tree from its top: a LayerCollector, or a GUI object
    /// with no collector above it.
    fn root(&mut self, top: InstanceId) {
        let dom = self.dom;
        if dom.is_a(top, "GuiObject") {
            let inner = Inner::whole(Vec2::ZERO);
            self.place_children(Placed::NOWHERE, &[top], inner, None);
            return;
        }
        let (origin, local) = if world::is_world_gui(dom, top) {
            let hung = Hung::of(dom, top);
            let size = hung.canvas_size(dom, self.eye.as_ref());
            self.out.world.push((hung, size));
            (Vec2::ZERO, size)
        } else {
            self.collector_area(top)
        };
        let scale = ui_scale(dom, top);
        let placed = Placed {
            center: origin + local * scale * 0.5,
            local,
            scale,
            rotation: 0.0,
        };
        self.out.placed.insert(top, placed);
        self.lay_out_inside(top, placed, local);
    }

    /// Where a ScreenGui's children go: the top left, in GUI coordinates
    /// (from just below the top bar), and size.
    fn collector_area(&self, collector: InstanceId) -> (Vec2, Vec2) {
        let dom = self.dom;
        match dom.class_name(collector) {
            "ScreenGui" => {
                // Measured: CoreUISafeInsets (the default) starts below the
                // top bar; the others (`IgnoreGuiInset` sets
                // DeviceSafeInsets) cover the whole window, from (0, -58).
                // Inference: DeviceSafeInsets has no inset on a desktop.
                let inset =
                    dom.enum_item_property(collector, "ScreenInsets") == Some("CoreUISafeInsets");
                if inset {
                    let size = self.screen - Vec2::new(0.0, GUI_INSET);
                    (Vec2::ZERO, size.max(Vec2::ZERO))
                } else {
                    (Vec2::new(0.0, -GUI_INSET), self.screen)
                }
            }
            _ => (Vec2::ZERO, Vec2::ZERO),
        }
    }

    /// Place `id` again in its parent as it is placed now, then lay out
    /// inside it.
    fn place_alone(&mut self, id: InstanceId) {
        let dom = self.dom;
        let Some(parent) = gui_parent(dom, id) else {
            return;
        };
        let Some(&at) = self.out.placed.get(&parent) else {
            return;
        };
        let inner = padding::inner(dom, parent, at.local);
        self.place_children(at, &[id], inner, None);
    }

    /// Lay out what `parent` (already placed in an `area` of its parent)
    /// holds.
    fn lay_out_inside(&mut self, parent: InstanceId, at: Placed, area: Vec2) {
        let dom = self.dom;
        if dom.class_name(parent) == "ScrollingFrame" {
            scrolling::lay_out(self, parent, at, area);
            return;
        }
        let kids = gui_children(dom, parent);
        if kids.is_empty() {
            return;
        }
        let inner = padding::inner(dom, parent, at.local);
        self.place_children(at, &kids, inner, arranger(dom, parent));
    }

    /// Size and place `kids` in the `inner` area of their parent (placed
    /// `at`), then lay out inside each.
    fn place_children(
        &mut self,
        at: Placed,
        kids: &[InstanceId],
        inner: Inner,
        layout: Option<InstanceId>,
    ) {
        let dom = self.dom;
        let mut sizes: Vec<Vec2> = kids.iter().map(|&k| self.size_of(k, inner.size)).collect();
        // Hidden children take no part in a UI layout (measured: they keep
        // the place it last gave them).
        let shown: Vec<usize> = (0..kids.len())
            .filter(|&i| layout.is_none() || visible(dom, kids[i]))
            .collect();
        let mut slots: Vec<Option<Vec2>> = vec![None; kids.len()];
        if let Some(layout) = layout {
            let laid: Vec<Kid> = shown
                .iter()
                .map(|&i| Kid {
                    id: kids[i],
                    size: sizes[i],
                })
                .collect();
            let arrangement = self.arrange(layout, &laid, inner.size);
            for (n, &i) in shown.iter().enumerate() {
                slots[i] = Some(arrangement.slots[n]);
                sizes[i] = arrangement.sizes[n];
            }
            self.out.arranged.insert(layout, arrangement.arranged);
        }
        for (i, &kid) in kids.iter().enumerate() {
            let cell = self.cells.get(&kid).copied();
            let size = cell.map_or(sizes[i], |(_, size)| size);
            let (top_left, anchor) = match (cell, slots[i]) {
                // Inference: a table's cell is placed from its row's top
                // left, whatever the row's padding.
                (Some((at, _)), _) => (at, Vec2::ZERO),
                (None, Some(slot)) => (inner.origin + slot, Vec2::ZERO),
                (None, None) if layout.is_some() => {
                    // Laying out part of a layout again, the earlier
                    // place is still in `out`.
                    let kept = self.previous.placed.get(&kid);
                    if let Some(&kept) = kept.or_else(|| self.out.placed.get(&kid)) {
                        self.out.placed.insert(kid, kept);
                        self.lay_out_text(kid, kept.local);
                        self.lay_out_inside(kid, kept, inner.size);
                        continue;
                    }
                    (inner.origin, Vec2::ZERO)
                }
                (None, None) => {
                    let anchor = vec2_or(dom, kid, "AnchorPoint", Vec2::ZERO);
                    let position = resolve(position_of(dom, kid), inner.size);
                    (inner.origin + position - size * anchor, anchor)
                }
            };
            // A UIScale scales the object about its anchor point.
            let k = ui_scale(dom, kid);
            let pivot = top_left + size * anchor;
            let center = pivot + size * k * (Vec2::splat(0.5) - anchor);
            let placed = Placed {
                center: at.screen_point(center),
                local: size,
                scale: at.scale * k,
                rotation: at.rotation + dom.float_property(kid, "Rotation").unwrap_or(0.0),
            };
            self.out.placed.insert(kid, placed);
            self.lay_out_text(kid, size);
            self.lay_out_inside(kid, placed, inner.size);
        }
    }

    /// A text object's text, in the area inside its padding.
    fn lay_out_text(&mut self, id: InstanceId, size: Vec2) {
        let Some(source) = Source::of(self.dom, id) else {
            return;
        };
        let inner = padding::inner(self.dom, id, size);
        let area = (inner.origin, inner.size);
        let fonts = self.fonts.generation();
        let earlier = self.previous.texts.get(&id);
        let block = match earlier.or_else(|| self.out.texts.get(&id)) {
            Some(old) if old.source == source && old.area == area && old.fonts == fonts => {
                old.block.clone()
            }
            _ => {
                let mut block = text::lay_out(self.fonts, &source, inner.size, None);
                block.shift(inner.origin);
                block
            }
        };
        let laid = Laid {
            source,
            area,
            fonts,
            block,
        };
        self.out.texts.insert(id, laid);
    }

    /// A GUI object's size in a parent area of `parent` pixels: `Size`
    /// by `SizeConstraint`, grown by `AutomaticSize`, then held by its
    /// size constraints.
    fn size_of(&mut self, id: InstanceId, parent: Vec2) -> Vec2 {
        let dom = self.dom;
        let base = base_size(dom, id, parent);
        let room = (parent - resolve(position_of(dom, id), parent)).max(base);
        let grown = automatic::grow(self, id, base, room);
        constraints::apply(dom, id, grown, parent)
    }
}

/// `Size` against the parent's area, by `SizeConstraint` (measured: it
/// changes only the size, never `Position`).
fn base_size(dom: &Dom, id: InstanceId, parent: Vec2) -> Vec2 {
    let size = dom.udim2_property(id, "Size").unwrap_or_default();
    let against = match dom.enum_item_property(id, "SizeConstraint") {
        Some("RelativeXX") => Vec2::splat(parent.x),
        Some("RelativeYY") => Vec2::splat(parent.y),
        _ => parent,
    };
    resolve(size, against)
}

fn position_of(dom: &Dom, id: InstanceId) -> UDim2 {
    dom.udim2_property(id, "Position").unwrap_or_default()
}

pub(crate) fn resolve(u: UDim2, parent: Vec2) -> Vec2 {
    Vec2::new(u.x.resolve(parent.x), u.y.resolve(parent.y))
}

pub(crate) fn vec2_or(dom: &Dom, id: InstanceId, name: &str, default: Vec2) -> Vec2 {
    dom.vector2_property(id, name).map_or(default, |v| v.0)
}

pub(crate) fn visible(dom: &Dom, id: InstanceId) -> bool {
    dom.bool_property(id, "Visible") != Some(false)
}

/// A UI layout's `HorizontalAlignment` and `VerticalAlignment` as the
/// share of the spare room before what it places.
fn alignment(dom: &Dom, layout: InstanceId) -> Vec2 {
    let share = |name| match dom.enum_item_property(layout, name) {
        Some("Center") => 0.5,
        Some("Right" | "Bottom") => 1.0,
        _ => 0.0,
    };
    Vec2::new(share("HorizontalAlignment"), share("VerticalAlignment"))
}

/// The first UIScale's `Scale` on `id`, 1 without one.
fn ui_scale(dom: &Dom, id: InstanceId) -> f32 {
    dom.find_first_child_of_class(id, "UIScale")
        .and_then(|s| dom.float_property(s, "Scale"))
        .unwrap_or(1.0)
}
