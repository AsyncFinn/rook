//! Which batch or slot each scene entry is drawn in, and the uploads it
//! waits on.

use super::waiting::File;
use super::*;

/// How a scene entry should be drawn now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Hidden,
    /// Its mesh or image is still being uploaded.
    Waiting,
    Opaque(BatchKey),
    Transparent,
    Layer(i32, BatchKey),
}

/// The layers' z of entries drawn over what's there ([`PartInstance::over`]):
/// before every image.
pub(crate) const OVER_Z: i32 = i32::MIN;

/// Where a scene entry is drawn, by its index in [`Scene::parts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    /// Invisible, a shape without a mesh, or waiting for an upload.
    Hidden,
    Opaque {
        batch: usize,
        slot: u32,
    },
    Layer {
        batch: usize,
        slot: u32,
    },
    /// An index into the transparent parts, which are sorted every frame.
    Transparent(usize),
}

/// Whether what an entry needs on the GPU is there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Readiness {
    Ready,
    Waiting,
    Missing,
}

impl Renderer {
    /// Replace the scene. Opaque parts are grouped per (shape, image), each
    /// group drawn instanced, and uploaded once; image layers likewise;
    /// transparent parts are kept for per-frame sorting.
    pub fn set_scene(&mut self, scene: &Scene) {
        profiling::scope!("Renderer::set_scene");
        self.opaque.clear();
        self.layers.clear();
        self.placements.clear();
        self.waiting.clear();
        self.transparent.clear();
        self.highlights.clear();
        self.lights.clear_blockers();
        self.update_parts(scene, &(0..scene.parts.len()).collect::<Vec<_>>());
    }

    /// Update the entries at `changed` (indices into `scene.parts`) and the
    /// lighting. Indices must stay with their entries from one call to the
    /// next; the scene may grow, and an entry that's no longer drawn should
    /// stay as a hidden one (fully transparent).
    pub fn update_parts(&mut self, scene: &Scene, changed: &[usize]) {
        profiling::scope!("Renderer::update_parts");
        self.lighting = scene.lighting;
        self.terrain = scene.terrain;
        self.lights.update(&self.device, &self.queue, &scene.lights);
        self.lights.update_blockers(scene, changed);
        self.particles.sim.set_emitters(&scene.emitters);
        self.request_particle_images(scene);
        self.ribbons
            .sim
            .set(&scene.beams, &scene.trails, &scene.floor_wires);
        self.request_ribbon_images(scene);
        // The skybox's images go up like any other image.
        for hash in scene.lighting.skybox.into_iter().flatten() {
            self.image_readiness(hash, Some(scene));
        }
        if self.placements.len() < scene.parts.len() {
            self.placements.resize(scene.parts.len(), Placement::Hidden);
        }
        // Near entries take near slots, so culling skips more.
        let mut changed = changed.to_vec();
        changed.sort_by_key(|&i| (crate::batch::cull::spatial_key(scene.parts[i].position), i));
        for i in changed {
            let part = scene.parts[i];
            self.waiting.remove(i);
            let target = self.target(&part, Some(scene));
            self.place(i, &part, target);
        }
    }

    /// Move the entries at `moved` (indices into `scene.parts`) to where
    /// they are now: entries whose look didn't change since they were last
    /// updated, which keep their slots.
    pub fn move_parts(&mut self, scene: &Scene, moved: &[usize]) {
        if moved.is_empty() {
            return;
        }
        profiling::scope!("Renderer::move_parts");
        self.lights.update_blockers(scene, moved);
        for &i in moved {
            let part = &scene.parts[i];
            self.highlights.moved(i, part);
            match self.placements[i] {
                Placement::Opaque { batch, slot } => self.opaque.moved(batch, slot, part),
                Placement::Layer { batch, slot } => self.layers.moved(batch, slot, part),
                Placement::Transparent(t) => self.transparent.moved(t, part),
                Placement::Hidden => {
                    if let Some(waiting) = self.waiting.get_mut(i) {
                        *waiting = *part;
                    }
                }
            }
        }
    }

    /// Whether uploads are still queued (entries waiting on them draw
    /// nothing yet).
    pub fn is_uploading(&self) -> bool {
        self.meshes.has_queued() || self.images.has_queued()
    }

    /// Upload everything queued now, and place what waited on it
    /// (screenshots, which shouldn't miss what a few more frames would
    /// show).
    pub fn finish_uploads(&mut self) {
        self.upload_files(usize::MAX);
    }

    /// Upload queued files within `budget` bytes, then place the entries
    /// whose files are all there.
    pub(crate) fn upload_files(&mut self, mut budget: usize) {
        if !self.is_uploading() {
            return;
        }
        profiling::scope!("Renderer::upload_files");
        let mut cx = UploadContext {
            device: &self.device,
            queue: &self.queue,
            image_layout: &self.image_layout,
            sampler: &self.image_sampler,
            view_formats: self.view_formats,
            block_compression: self.block_compression,
        };
        let mut first = true;
        let meshes = self.meshes.upload(&mut self.pool, &mut budget, &mut first);
        let images = self.images.upload(&mut cx, &mut budget, &mut first);
        if meshes.is_empty() && images.is_empty() {
            return;
        }
        self.add_images(&images);
        let uploaded =
            (meshes.into_iter().map(File::Mesh)).chain(images.into_iter().map(File::Image));
        for (i, part) in self.waiting.take(uploaded) {
            let target = self.target(&part, None);
            self.place(i, &part, target);
        }
    }

    /// Draw entry `i` as `target`, leaving where it was drawn before.
    fn place(&mut self, i: usize, part: &PartInstance, target: Target) {
        let drawn = !matches!(target, Target::Hidden | Target::Waiting);
        let masked = (drawn && part.layer == Layer::Base && part.highlight.is_some())
            .then(|| self.instance(part));
        self.highlights.set(i, part, masked);
        match (self.placements[i], target) {
            (Placement::Hidden, Target::Hidden) => {}
            (Placement::Transparent(t), Target::Transparent) => {
                let instance = self.instance(part);
                self.transparent.set(t, part, instance);
            }
            (Placement::Opaque { batch, slot }, Target::Opaque(key))
                if self.opaque.list[batch].key == key =>
            {
                let instance = self.instance(part);
                self.opaque.set(batch, slot, instance);
            }
            (Placement::Layer { batch, slot }, Target::Layer(z, key))
                if self.layers.list[batch].key == key && self.layers.z[batch] == z =>
            {
                let instance = self.instance(part);
                self.layers.set(batch, slot, instance);
            }
            (old, _) => {
                self.vacate(old);
                if target == Target::Waiting {
                    let files = self.queued_files(part);
                    self.waiting.insert(i, *part, &files);
                }
                self.placements[i] = self.occupy(part, target);
            }
        }
    }

    /// The bounds of a shape's mesh, in its own units.
    pub(crate) fn bounds(&self, shape: DrawShape) -> (Vec3, Vec3) {
        self.meshes
            .get(&shape)
            .map_or((Vec3::splat(-0.5), Vec3::splat(0.5)), |m| m.bounds)
    }

    /// How an entry should be drawn. With the scene, files it needs that
    /// aren't on the GPU are queued; without, they're only waited for.
    fn target(&mut self, part: &PartInstance, scene: Option<&Scene>) -> Target {
        let z = match part.layer {
            Layer::Base
            | Layer::Image {
                on_transparent: true,
                ..
            } => None,
            Layer::Image { z, .. } => Some(z),
        };
        if part.transparency >= 1.0 {
            return Target::Hidden;
        }
        let mut images = image_set(part);
        let mut ready = self.mesh_readiness(part.shape, scene);
        for (slot, image) in images.iter_mut().enumerate() {
            let Some(hash) = *image else {
                continue;
            };
            match self.image_readiness(hash, scene) {
                // A missing map leaves the material's plain value; a
                // missing image hides what draws it.
                Readiness::Missing if slot > 0 => *image = None,
                Readiness::Missing => ready = Readiness::Missing,
                Readiness::Waiting if ready == Readiness::Ready => ready = Readiness::Waiting,
                Readiness::Ready | Readiness::Waiting => {}
            }
        }
        match ready {
            Readiness::Missing => return Target::Hidden,
            Readiness::Waiting => return Target::Waiting,
            Readiness::Ready => self.ensure_group(images),
        }
        let key = BatchKey {
            shape: part.shape,
            images,
        };
        match z {
            None if part.over => Target::Layer(OVER_Z, key),
            Some(z) => Target::Layer(z, key),
            // ForceField always shows what's behind it (observed); an
            // unlit adornment's image is cut out by its alpha.
            None if part.transparency > 0.0
                || part.on_top
                || (part.light != Light::Lit && part.texture.is_some())
                || part.layer != Layer::Base
                || part.material == Material::ForceField
                || part.maps.alpha == MapAlpha::Transparency =>
            {
                Target::Transparent
            }
            None => Target::Opaque(key),
        }
    }

    /// Whether a shape's mesh is on the GPU, queueing a mesh file (or
    /// building a truss or adornment shape) the first time one is drawn. A file the scene
    /// doesn't hold, or one without triangles, draws nothing.
    fn mesh_readiness(&mut self, shape: DrawShape, scene: Option<&Scene>) -> Readiness {
        if self.meshes.contains(&shape) {
            return Readiness::Ready;
        }
        if self.meshes.is_queued(&shape) {
            return Readiness::Waiting;
        }
        let hash = match shape {
            DrawShape::File(hash) => hash,
            // Built the first time a truss or adornment of this shape is
            // drawn: a few kilobytes, uploaded at once and shared by every
            // such part.
            DrawShape::Truss(_) | DrawShape::Adorn(_) => {
                let gpu = self.pool.add(&mesh::build(shape));
                self.meshes.insert(shape, gpu);
                return Readiness::Ready;
            }
            _ => return Readiness::Missing,
        };
        match scene.and_then(|s| s.meshes.get(&hash)) {
            Some(file) if !file.indices.is_empty() => {
                self.meshes.request(shape, file);
                Readiness::Waiting
            }
            _ => Readiness::Missing,
        }
    }

    pub(super) fn image_readiness(&mut self, hash: ImageHash, scene: Option<&Scene>) -> Readiness {
        if self.images.contains(&hash) {
            return Readiness::Ready;
        }
        if self.images.is_queued(&hash) {
            return Readiness::Waiting;
        }
        match scene.and_then(|s| s.images.get(&hash)) {
            Some(file) => {
                self.images.request(hash, file);
                Readiness::Waiting
            }
            None => Readiness::Missing,
        }
    }

    /// The files an entry waits on: its mesh and images queued for upload.
    fn queued_files(&self, part: &PartInstance) -> Vec<File> {
        let mesh = (self.meshes.is_queued(&part.shape)).then_some(File::Mesh(part.shape));
        let images = (image_set(part).into_iter().flatten())
            .filter(|h| self.images.is_queued(h))
            .map(File::Image);
        mesh.into_iter().chain(images).collect()
    }

    /// Free the slot an entry was drawn in.
    pub(crate) fn vacate(&mut self, placement: Placement) {
        match placement {
            Placement::Hidden => {}
            Placement::Opaque { batch, slot } => self.opaque.vacate(batch, slot),
            Placement::Layer { batch, slot } => self.layers.vacate(batch, slot),
            Placement::Transparent(t) => self.transparent.vacate(t),
        }
    }

    /// Take a slot to draw an entry in.
    pub(crate) fn occupy(&mut self, part: &PartInstance, target: Target) -> Placement {
        let instance = self.instance(part);
        match target {
            Target::Hidden | Target::Waiting => Placement::Hidden,
            Target::Transparent => Placement::Transparent(self.transparent.occupy(part, instance)),
            Target::Opaque(key) => {
                let (batch, slot) = self.opaque.occupy(0, key, instance);
                Placement::Opaque { batch, slot }
            }
            Target::Layer(z, key) => {
                let (batch, slot) = self.layers.occupy(z, key, instance);
                Placement::Layer { batch, slot }
            }
        }
    }
}
