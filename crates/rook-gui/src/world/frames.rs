//! Each frame's world GUI: drawing the canvases, keeping them on what they
//! hang from, and telling where they are.

use super::*;

impl Gui {
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
                    _ => draw::overlays::overhead::canvas(&tag.look, &mut text),
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
    pub(crate) fn places_at(
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

    pub(crate) fn tag_places(&self) -> impl Iterator<Item = Option<WorldPlace>> + '_ {
        (self.overheads.shown.iter())
            .map(|tag| Some(tag.place(draw::overlays::overhead::size(&tag.look))))
    }
}
