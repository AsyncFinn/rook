//! Bringing GUI up to date: laying out again what changed since last time,
//! and writing what the layout found (`Absolute*`, `TextBounds`, ...) back
//! to the DOM, where scripts read it.

use super::*;

impl Gui {
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
            self.lay_out_screens(dom);
        }
        self.sort_pending(dom);
        let pending = (std::mem::take(&mut self.pending).into_values())
            .fold(Pending::default(), Pending::merge);
        let (relaid, placed) = self.lay_out_pending(dom, pending);
        // Text laid out above already has its new look.
        let retexted: Vec<InstanceId> = (std::mem::take(&mut self.retexted).into_iter())
            .filter(|&id| {
                placed.binary_search(&id).is_err()
                    && layout::top_of(dom, id)
                        .is_some_and(|top| relaid.binary_search(&top).is_err())
            })
            .collect();
        if !retexted.is_empty() {
            self.layout
                .restyle_texts(dom, &retexted, (&mut self.fonts, &self.turning));
            self.restyled.extend(retexted);
        }
        let restyled: Vec<InstanceId> = std::mem::take(&mut self.restyled).into_iter().collect();
        self.repaint(dom, &restyled, false);
        self.resize_world(dom, None);
    }

    /// Bring up to date what reading `id`'s layout outputs depends on: its
    /// own tree, laid out again where it changed, or whole when the screen
    /// changed size or it isn't on one and hasn't been laid out yet, as
    /// browsers lay out only the dirty part a script's read needs. Other
    /// trees wait for the frame's [`Gui::sync`]; with nothing changed this
    /// is a lookup.
    pub fn sync_for(&mut self, dom: &mut Dom, id: InstanceId) {
        self.note_changes(dom);
        let Some(top) = layout::top_of(dom, id) else {
            return;
        };
        self.sort_pending(dom);
        let pending = self.pending.remove(&Some(top));
        let stale = self.stale && !self.fresh.contains(&top);
        if stale || !self.layout.placed.contains_key(&top) {
            // Unparented, kept in storage, or a server's world GUI: laid
            // out when first read, then kept up to date as a screen's is.
            // On a stale screen, the frame's layout keeps it.
            profiling::scope!("Gui::layout tree read");
            (self.layout).redo(dom, &[top], (&mut self.fonts, &self.turning));
            self.write_trees(dom, &[top]);
            if self.stale {
                self.fresh.insert(top);
            }
            return;
        }
        if let Some(pending) = pending {
            self.lay_out_pending(dom, pending);
        }
        self.resize_world(dom, Some(top));
    }

    /// Lay out every screen's GUI and the world GUIs drawn on the screen
    /// as it is now, but the trees reads laid out since it changed, which
    /// are kept with what changed in them since.
    fn lay_out_screens(&mut self, dom: &mut Dom) {
        profiling::scope!("Gui::layout");
        if !self.fresh.is_empty() {
            // What moved is taken out of where it was, and filed.
            self.sort_pending(dom);
        }
        self.in_world.retain(|&id| world::held_where_drawn(dom, id));
        self.messages.retain(|&id| dom.parent(id).is_some());
        // A server draws no world GUI: it lays one out only when a
        // script reads it, as it does GUI in storage.
        let mut world: Vec<InstanceId> = match dom.local_player() {
            Some(_) => self.in_world.iter().copied().collect(),
            None => Vec::new(),
        };
        world.sort_unstable();
        let mut previous = std::mem::take(&mut self.layout);
        let (layout, kept) = Layout::screens(
            dom,
            (&mut previous, &self.turning),
            &mut self.fonts,
            &world,
            &self.fresh,
        );
        self.layout = layout;
        // The screen they're laid out on changed: everything did.
        rook_types::count!(GuiObjectsChanged, self.layout.placed.len());
        self.stale = false;
        self.fresh.clear();
        self.moved.clear();
        self.changed.clear();
        self.shifted.clear();
        let kept: FxHashSet<InstanceId> = kept.into_iter().collect();
        self.pending
            .retain(|tree, _| tree.is_some_and(|t| kept.contains(&t)));
        if kept.is_empty() {
            self.retexted.clear();
        } else {
            (self.retexted)
                .retain(|&id| layout::top_of(dom, id).is_some_and(|t| kept.contains(&t)));
        }
        self.restyled.clear();
        self.laid_out += 1;
        self.world_listed += 1;
        self.screen.forget();
        self.canvases.clear();
        self.write(dom);
    }

    /// File what changed since the last layout under the tree it's in, so
    /// a read can lay out its own tree alone. Each object's tree is found
    /// once. GUI that moved in the tree is first taken out of where it
    /// was laid out (see [`Gui::take_moved`]).
    fn sort_pending(&mut self, dom: &Dom) {
        for top in self.take_moved(dom) {
            self.pending.entry(Some(top)).or_default().joined.push(top);
        }
        for id in std::mem::take(&mut self.changed) {
            let tree = layout::top_of(dom, id);
            self.pending.entry(tree).or_default().changed.push(id);
        }
        for id in std::mem::take(&mut self.shifted) {
            let tree = layout::top_of(dom, id);
            self.pending.entry(tree).or_default().shifted.push(id);
        }
    }

    /// Lay out again what `pending` names: trees whole where they must
    /// be, objects alone where they can be. Returns the trees laid out
    /// whole and the objects placed alone, each sorted.
    fn lay_out_pending(
        &mut self,
        dom: &mut Dom,
        mut pending: Pending,
    ) -> (Vec<InstanceId>, Vec<InstanceId>) {
        // Some may have been freed since they were filed.
        for ids in [
            &mut pending.changed,
            &mut pending.shifted,
            &mut pending.joined,
        ] {
            ids.retain(|&id| dom.get(id).is_some());
            ids.sort_unstable();
            ids.dedup();
        }
        let (shifted, whole) = self.layout.alone(dom, pending.shifted);
        let mut touched = pending.changed;
        touched.extend(whole);
        // Trees not laid out (in storage, or a server's world GUI) wait for
        // a script to read them.
        let mut relaid: Vec<InstanceId> = (tops(dom, touched.iter().copied().collect()))
            .into_iter()
            .filter(|top| self.layout.placed.contains_key(top))
            .chain(pending.joined)
            .collect();
        relaid.sort_unstable();
        relaid.dedup();
        if !relaid.is_empty() {
            profiling::scope!("Gui::layout trees");
            (self.layout).redo(dom, &relaid, (&mut self.fonts, &self.turning));
            self.write_trees(dom, &relaid);
            self.redraw(dom, &relaid);
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
            .filter(|&id| {
                layout::top_of(dom, id).is_some_and(|top| relaid.binary_search(&top).is_err())
            })
            .collect();
        if !shifted.is_empty() {
            profiling::scope!("Gui::layout objects");
            (self.layout).place_again(dom, &shifted, (&mut self.fonts, &self.turning));
            self.write_trees(dom, &shifted);
            self.repaint(dom, &shifted, true);
        }
        (relaid, shifted)
    }

    /// Lay out again the world GUIs sized by the camera or their part
    /// whose canvas changed size: `tree`'s, or every one.
    fn resize_world(&mut self, dom: &mut Dom, tree: Option<InstanceId>) {
        let followers = || {
            let hung = match tree {
                Some(top) => self.layout.world.range(top..=top),
                None => self.layout.world.range(..),
            };
            hung.map(|(_, hung)| hung).filter(|(h, _)| h.follows())
        };
        // A server draws nothing, so its billboards keep the size they were
        // laid out at.
        if followers().next().is_none() || dom.local_player().is_none() {
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

    pub(crate) fn write(&mut self, dom: &mut Dom) {
        let ids: Vec<InstanceId> = (self.layout.placed.keys())
            .chain(self.layout.arranged.keys())
            .copied()
            .collect();
        self.write_ids(dom, &ids);
    }

    pub(crate) fn write_trees(&mut self, dom: &mut Dom, tops: &[InstanceId]) {
        let ids: Vec<InstanceId> = (tops.iter())
            .flat_map(|&id| std::iter::once(id).chain(dom.descendants(id)))
            .collect();
        self.write_ids(dom, &ids);
    }

    pub(crate) fn write_ids(&mut self, dom: &mut Dom, ids: &[InstanceId]) {
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
