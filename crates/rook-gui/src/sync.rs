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
            profiling::scope!("Gui::layout");
            self.in_world.retain(|&id| world::held_where_drawn(dom, id));
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
