//! A layout pass: sizing and placing each object's children, by their
//! parent's UI layout if it has one, then laying out inside each and
//! their text.

use super::*;

/// Where each turning UIPageLayout's view is, as a page index.
pub(crate) type Turning = FxHashMap<InstanceId, f32>;

pub(crate) type Fonted<'a> = (&'a mut Fonts, &'a Turning);

#[derive(Debug, Clone, Copy)]
pub(crate) struct Kid {
    pub(crate) id: InstanceId,
    pub(crate) size: Vec2,
}

/// The UIListLayout or UIGridLayout placing `parent`'s children: its
/// first one (measured: a UIListLayout added after a UIGridLayout changes
/// nothing).
pub(super) fn arranger(dom: &Dom, parent: InstanceId) -> Option<InstanceId> {
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

pub(super) struct Pass<'a> {
    pub(super) dom: &'a Dom,
    pub(super) previous: &'a Layout,
    pub(super) turning: &'a Turning,
    pub(super) fonts: &'a mut Fonts,
    pub(super) out: Layout,
    pub(super) screen: Vec2,
    /// What BillboardGuis are sized from.
    pub(super) eye: Option<Eye>,
    /// Cells a UITableLayout placed in their rows: top left and size.
    pub(super) cells: FxHashMap<InstanceId, (Vec2, Vec2)>,
}

impl<'a> Pass<'a> {
    pub(super) fn new(
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
    pub(super) fn arrange(&mut self, layout: InstanceId, kids: &[Kid], inner: Vec2) -> Arrangement {
        self.arrange_spaced(layout, kids, (inner, inner))
    }

    /// [`Pass::arrange`] with a list's `Padding` scale of `spaced` (an
    /// automatic size measuring, see [`list::arrange`]).
    pub(super) fn arrange_spaced(
        &mut self,
        layout: InstanceId,
        kids: &[Kid],
        (inner, spaced): (Vec2, Vec2),
    ) -> Arrangement {
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
            _ => list::arrange(dom, layout, &kids, (inner, spaced)),
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
    pub(super) fn root(&mut self, top: InstanceId) {
        let dom = self.dom;
        if dom.is_a(top, "GuiObject") {
            let inner = Inner::whole(Vec2::ZERO);
            self.place_children(Placed::NOWHERE, &[top], inner, None);
            return;
        }
        let (origin, local) = if world::is_world_gui(dom, top) {
            let hung = Hung::of(dom, top);
            let size = hung.canvas_size(dom, self.eye.as_ref());
            self.out.world.insert(top, (hung, size));
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
        rook_types::count!(GuiNodesLaidOut, 1);
        self.lay_out_inside(top, placed, local);
    }

    /// Where a ScreenGui's children go: the top left, in GUI coordinates
    /// (from just below the top bar), and size.
    pub(super) fn collector_area(&self, collector: InstanceId) -> (Vec2, Vec2) {
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
    pub(super) fn place_alone(&mut self, id: InstanceId) {
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
    pub(super) fn lay_out_inside(&mut self, parent: InstanceId, at: Placed, area: Vec2) {
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
    pub(super) fn place_children(
        &mut self,
        at: Placed,
        kids: &[InstanceId],
        inner: Inner,
        layout: Option<InstanceId>,
    ) {
        let dom = self.dom;
        rook_types::count!(GuiNodesLaidOut, kids.len());
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

    pub(super) fn lay_out_text(&mut self, id: InstanceId, size: Vec2) {
        let Some(source) = Source::of(self.dom, id) else {
            return;
        };
        let inner = padding::inner(self.dom, id, size);
        let area = (inner.origin, inner.size);
        let same = |old: &Laid| old.source == source && old.area == area && !old.waited;
        let recolors = |old: &Laid| old.area == area && !old.waited && old.source.recolors(&source);
        // Unchanged since it was last laid out, or only its colors: kept.
        let kept = match self.out.texts.remove(&id) {
            Some(old) if same(&old) => Some(old.block),
            Some(mut old) if recolors(&old) => {
                old.block.recolor(&source.style);
                Some(old.block)
            }
            _ => (self.previous.texts.get(&id))
                .filter(|old| same(old))
                .map(|old| old.block.clone()),
        };
        let (block, waited) = match kept {
            Some(block) => (block, false),
            None => {
                self.fonts.take_waited();
                let mut block = text::lay_out(self.fonts, &source, inner.size, None);
                block.shift(inner.origin);
                (block, self.fonts.take_waited())
            }
        };
        let laid = Laid {
            source,
            area,
            waited,
            block,
        };
        self.out.texts.insert(id, laid);
    }

    /// A GUI object's size in a parent area of `parent` pixels: `Size`
    /// by `SizeConstraint`, grown by `AutomaticSize`, then held by its
    /// size constraints.
    pub(super) fn size_of(&mut self, id: InstanceId, parent: Vec2) -> Vec2 {
        self.size_within(id, parent, parent)
    }

    /// [`Pass::size_of`] with `Size` against `parent` but room to grow
    /// in `area` (an automatic parent measuring its children).
    pub(super) fn size_within(&mut self, id: InstanceId, parent: Vec2, area: Vec2) -> Vec2 {
        let dom = self.dom;
        let base = base_size(dom, id, parent);
        let room = (area - resolve(position_of(dom, id), parent)).max(base);
        let grown = automatic::grow(self, id, base, room);
        constraints::apply(dom, id, grown, parent)
    }
}

/// `Size` against the parent's area, by `SizeConstraint` (measured: it
/// changes only the size, never `Position`).
pub(super) fn base_size(dom: &Dom, id: InstanceId, parent: Vec2) -> Vec2 {
    let size = dom.udim2_property(id, "Size").unwrap_or_default();
    let against = match dom.enum_item_property(id, "SizeConstraint") {
        Some("RelativeXX") => Vec2::splat(parent.x),
        Some("RelativeYY") => Vec2::splat(parent.y),
        _ => parent,
    };
    resolve(size, against)
}

pub(super) fn position_of(dom: &Dom, id: InstanceId) -> UDim2 {
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
pub(super) fn alignment(dom: &Dom, layout: InstanceId) -> Vec2 {
    let share = |name| match dom.enum_item_property(layout, name) {
        Some("Center") => 0.5,
        Some("Right" | "Bottom") => 1.0,
        _ => 0.0,
    };
    Vec2::new(share("HorizontalAlignment"), share("VerticalAlignment"))
}

/// The first UIScale's `Scale` on `id`, 1 without one.
pub(super) fn ui_scale(dom: &Dom, id: InstanceId) -> f32 {
    dom.find_first_child_of_class(id, "UIScale")
        .and_then(|s| dom.float_property(s, "Scale"))
        .unwrap_or(1.0)
}
