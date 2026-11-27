//! Taking each frame's GUI: the lists that changed packed and uploaded,
//! the cursor, where world canvases are and the ViewportFrames' pictures.

use super::*;

impl GuiPass {
    /// Draw `screen`'s pieces on the screen from now on, bottom first, and
    /// the world GUI's canvases as `world` says, with the groups they
    /// hold; `None` keeps what's drawn. Only pieces and canvases not given
    /// last time (another `Arc`) are packed and uploaded; those are
    /// returned.
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: Option<&Arc<[Arc<[GuiQuad]>]>>,
        world: Option<&WorldCanvases>,
    ) -> Vec<Fresh> {
        let screen = screen.filter(|s| !self.screen_pieces.is(s));
        let screen_kept = screen.map(|s| self.screen_pieces.kept(s));
        let mut fresh: Vec<Fresh> = match (screen, &screen_kept) {
            (Some(s), Some(kept)) => (s.iter().zip(kept))
                .filter(|(_, k)| k.is_none())
                .map(|(p, _)| Fresh::Piece(p.clone()))
                .collect(),
            _ => Vec::new(),
        };
        let fresh_pieces = fresh.len();
        let world_kept = match world {
            Some(WorldCanvases::List(canvases)) => {
                let kept = self.world.kept(canvases);
                let new = (canvases.iter().zip(&kept)).filter(|(_, k)| k.is_none());
                fresh.extend(new.map(|(c, _)| Fresh::Canvas(c.clone())));
                Some(kept)
            }
            Some(WorldCanvases::Changed { canvases, .. }) => {
                fresh.extend(canvases.iter().map(|(_, c)| Fresh::Canvas(c.clone())));
                None
            }
            None => None,
        };
        let fresh_held: Vec<Held> = fresh.iter().map(|f| groups::held(&[f.quads()])).collect();
        // Every group shown, inner ones first, each once, and every
        // ViewportFrame: looked for only where some are drawn or given.
        let mut shown: Vec<Arc<GuiGroup>> = Vec::new();
        let mut viewports: Vec<GuiViewport> = Vec::new();
        let grouped = !self.groups.is_empty()
            || !self.viewports.is_empty()
            || fresh_held.iter().any(|h| !h.is_empty());
        let replaced: rustc_hash::FxHashSet<usize> = match world {
            Some(WorldCanvases::Changed { canvases, .. }) if grouped => {
                canvases.iter().map(|(i, _)| *i).collect()
            }
            _ => rustc_hash::FxHashSet::default(),
        };
        if grouped {
            let screen_held: Box<dyn Iterator<Item = &Held>> = match &screen_kept {
                Some(kept) => Box::new(self.screen_pieces.held(kept)),
                None => Box::new(self.screen_pieces.all_held()),
            };
            let world_held: Box<dyn Iterator<Item = &Held>> = match (world, &world_kept) {
                (_, Some(kept)) => Box::new(self.world.held(kept)),
                (Some(WorldCanvases::Changed { len, .. }), None) => {
                    Box::new(self.world.held_but(&replaced, *len))
                }
                _ => Box::new(self.world.held_but(&replaced, usize::MAX)),
            };
            for held in screen_held.chain(world_held).chain(&fresh_held) {
                for group in &held.groups {
                    if !shown.iter().any(|g| g.key == group.key) {
                        shown.push(group.clone());
                    }
                }
                viewports.extend(&held.viewports);
            }
        }
        let unpacked: Vec<&Arc<GuiGroup>> = shown.iter().filter(|g| !self.groups.has(g)).collect();
        let lists: Vec<&[GuiQuad]> = (fresh.iter().map(Fresh::quads))
            .chain(unpacked.iter().map(|g| &g.quads[..]))
            .collect();
        let (packed, reset) = self.pack(queue, &lists);
        // The masks or ramps what was kept points at were written over:
        // place everything again.
        if reset {
            let screen = match screen {
                Some(s) => s.clone(),
                None => self.screen_pieces.list(),
            };
            let mut canvases = self.world.sources();
            match world {
                Some(WorldCanvases::List(list)) => canvases.clone_from(list),
                Some(WorldCanvases::Changed {
                    len,
                    canvases: changed,
                }) => {
                    canvases.truncate(*len);
                    for (i, c) in changed {
                        match canvases.get_mut(*i) {
                            Some(slot) => *slot = c.clone(),
                            None => canvases.push(c.clone()),
                        }
                    }
                }
                None => {}
            }
            let all = screen.len() + canvases.len() + shown.len();
            if lists.len() < all {
                self.screen_pieces.forget();
                self.world.forget();
                self.groups.forget();
                let world = WorldCanvases::List(canvases);
                return self.set(device, queue, Some(&screen), Some(&world));
            }
        }
        let mut packed = packed.into_iter();
        let mut fresh_held = fresh_held.into_iter();
        if let (Some(screen), Some(kept)) = (screen, &screen_kept) {
            let pieces: Vec<(Packed, Held)> = (&mut packed)
                .zip(&mut fresh_held)
                .take(fresh_pieces)
                .collect();
            (self.screen_pieces).set(device, queue, &mut self.screen, screen, kept, pieces);
        }
        let new: Vec<(Packed, Held)> = (&mut packed).zip(&mut fresh_held).collect();
        match (world, world_kept) {
            (Some(WorldCanvases::List(canvases)), Some(kept)) => {
                self.world.set(device, queue, canvases, kept, new);
            }
            (Some(WorldCanvases::Changed { len, canvases }), _) => {
                self.world.replace(device, queue, *len, canvases, new);
            }
            _ => {}
        }
        if grouped && self.viewports != viewports {
            self.viewports = viewports;
            self.viewports_version += 1;
        }
        if shown.is_empty() && self.groups.is_empty() {
            return fresh;
        }
        let binds = Binds {
            layout: &self.layout,
            glyphs: &self.glyphs,
            ramps: &self.ramps_view,
            sampler: &self.sampler,
        };
        let new_groups: Vec<(Arc<GuiGroup>, Packed)> =
            unpacked.into_iter().cloned().zip(packed).collect();
        let target = |d: &wgpu::Device| Target::new(d, binds);
        if let Some(views) = (self.groups).set(device, queue, &shown, &new_groups, target) {
            self.images.set_groups(views);
            self.world.forget_groups();
        }
        fresh
    }

    /// Draw `quad`, an image quad, over everything from now on, or
    /// nothing.
    pub(crate) fn set_cursor(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quad: Option<&GuiQuad>,
    ) {
        let packed: Packed = (quad.into_iter())
            .filter_map(|q| match &q.paint {
                Paint::Image(image) => {
                    Some((GpuQuad::new(q, image.uv, None), Some(ImageKey::of(image))))
                }
                _ => None,
            })
            .collect();
        self.cursor.set(device, queue, &packed);
    }

    /// Where each world GUI canvas is from now on.
    pub(crate) fn place(&mut self, places: impl IntoIterator<Item = (usize, Option<WorldPlace>)>) {
        self.world.place(places);
    }

    /// Each of `lists` as the shader reads them, their glyphs in the atlas
    /// and their gradients' ramps in theirs. If either fills and starts
    /// again partway, `lists` are placed again once, so none points at a
    /// mask or ramp that was written over, and whether that happened (so
    /// lists packed earlier may).
    pub(super) fn pack(
        &mut self,
        queue: &wgpu::Queue,
        lists: &[&[GuiQuad]],
    ) -> (Vec<Packed>, bool) {
        let scale = 1.0 / atlas::SIZE as f32;
        let once = |atlas: &mut Atlas, ramps: &mut Ramps| -> Vec<Packed> {
            (lists.iter())
                .map(|quads| {
                    quads
                        .iter()
                        .filter_map(|q| {
                            let (uv, image) = match &q.paint {
                                Paint::Solid => ([0.0; 4], None),
                                Paint::Glyph(mask) | Paint::Outline(mask, _) => {
                                    (atlas.place(queue, mask)?.map(|t| t * scale), None)
                                }
                                Paint::Image(image) => (image.uv, Some(ImageKey::of(image))),
                                Paint::Group(group) => {
                                    ([0.0, 0.0, 1.0, 1.0], Some(ImageKey::Group(group.key)))
                                }
                                Paint::Viewport(viewport) => {
                                    ([0.0, 0.0, 1.0, 1.0], Some(ImageKey::Viewport(viewport.key)))
                                }
                            };
                            let ramp = (q.gradient.as_ref()).map(|g| ramps.place(queue, &g.ramp));
                            Some((GpuQuad::new(q, uv, ramp), image))
                        })
                        .collect()
                })
                .collect()
        };
        let resets = (self.atlas.resets, self.ramps.resets);
        let gpu = once(&mut self.atlas, &mut self.ramps);
        if (self.atlas.resets, self.ramps.resets) == resets {
            return (gpu, false);
        }
        (once(&mut self.atlas, &mut self.ramps), true)
    }

    /// Bind the images the GUI draws that are on the GPU now (`cache`),
    /// and upload the world GUI's canvases `camera` sees this frame in a
    /// `size` window, the untoned ones as screen quads.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &GpuCache<ImageHash, Image>,
        lights: &GpuLights,
        (camera, view): (&Camera, &Frustum),
        size: (u32, u32),
    ) -> WorldRuns {
        let overlay = self.world.overlay(camera, size);
        self.overlay.set(device, queue, &overlay);
        self.images.bind(device, &self.overlay.runs, cache);
        self.images.bind(device, &self.screen.runs, cache);
        self.images.bind(device, &self.cursor.runs, cache);
        for (target, _, _) in self.groups.drawn() {
            self.images.bind(device, &target.runs, cache);
        }
        let images = (&self.images, cache);
        self.world
            .upload(device, queue, images, lights, (camera, view), size.1)
    }

    /// Drop the bind groups of `hash`'s image, which was replaced: they're
    /// made again with the new one when next drawn.
    pub(crate) fn forget_image(&mut self, hash: ImageHash) {
        self.images.forget_file(hash);
        self.world.forget_file(hash);
    }

    /// The ViewportFrames shown, with their pictures' sizes, and how many
    /// times that list changed.
    pub(crate) fn viewports(&self) -> (&[GuiViewport], u64) {
        (&self.viewports, self.viewports_version)
    }

    /// Draw ViewportFrames through these pictures from now on, by frame.
    pub(crate) fn set_viewports(&mut self, views: FxHashMap<u64, wgpu::TextureView>) {
        self.images.set_viewports(views);
    }
}
