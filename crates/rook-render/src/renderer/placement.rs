//! Which batch or slot each scene entry is drawn in, and the uploads it
//! waits on.

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

/// Where a scene entry is drawn, by its index in [`Scene::parts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    /// Invisible, a shape without a mesh, or waiting for an upload.
    Hidden,
    Opaque {
        batch: usize,
        slot: usize,
    },
    Layer {
        batch: usize,
        slot: usize,
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
        self.transparent_parts.clear();
        self.transparent_free.clear();
        self.update_parts(scene, &(0..scene.parts.len()).collect::<Vec<_>>());
    }

    /// Update the entries at `changed` (indices into `scene.parts`) and the
    /// lighting. Indices must stay with their entries from one call to the
    /// next; the scene may grow, and an entry that's no longer drawn should
    /// stay as a hidden one (fully transparent).
    pub fn update_parts(&mut self, scene: &Scene, changed: &[usize]) {
        profiling::scope!("Renderer::update_parts");
        self.lighting = scene.lighting;
        // The skybox's images go up like any other image.
        for hash in scene.lighting.skybox.into_iter().flatten() {
            self.image_readiness(hash, Some(scene));
        }
        if self.placements.len() < scene.parts.len() {
            self.placements.resize(scene.parts.len(), Placement::Hidden);
        }
        for &i in changed {
            let part = scene.parts[i];
            self.waiting.remove(&i);
            let target = self.target(&part, Some(scene));
            self.place(i, &part, target);
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
        let cx = UploadContext {
            device: &self.device,
            queue: &self.queue,
            image_layout: &self.image_layout,
            sampler: &self.image_sampler,
        };
        let mut first = true;
        let meshes = self.meshes.upload(&cx, &mut budget, &mut first);
        let images = self.images.upload(&cx, &mut budget, &mut first);
        if !meshes && !images {
            return;
        }
        let waiting: Vec<(usize, PartInstance)> = self.waiting.drain().collect();
        for (i, part) in waiting {
            let target = self.target(&part, None);
            self.place(i, &part, target);
        }
    }

    /// Draw entry `i` as `target`, leaving where it was drawn before.
    fn place(&mut self, i: usize, part: &PartInstance, target: Target) {
        match (self.placements[i], target) {
            (Placement::Hidden, Target::Hidden) => {}
            (Placement::Transparent(t), Target::Transparent) => {
                self.transparent_parts[t] = Some(*part);
            }
            (Placement::Opaque { batch, slot }, Target::Opaque(key))
                if self.opaque.list[batch].key == key =>
            {
                let instance = Instance::new(part, self.bounds(part.shape));
                self.opaque.list[batch].set(slot, instance);
            }
            (Placement::Layer { batch, slot }, Target::Layer(z, key))
                if self.layers.list[batch].key == key && self.layers.z[batch] == z =>
            {
                let instance = Instance::new(part, self.bounds(part.shape));
                self.layers.list[batch].set(slot, instance);
            }
            (old, _) => {
                self.vacate(old);
                if target == Target::Waiting {
                    self.waiting.insert(i, *part);
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
        let (image, z) = match part.layer {
            Layer::Base => (part.texture, None),
            Layer::Image {
                on_transparent: true,
                image,
                ..
            } => (Some(image), None),
            Layer::Image { image, z, .. } => (Some(image), Some(z)),
        };
        if part.transparency >= 1.0 {
            return Target::Hidden;
        }
        let mesh = self.mesh_readiness(part.shape, scene);
        let image_ready = image.map_or(Readiness::Ready, |hash| self.image_readiness(hash, scene));
        match (mesh, image_ready) {
            (Readiness::Missing, _) | (_, Readiness::Missing) => return Target::Hidden,
            (Readiness::Waiting, _) | (_, Readiness::Waiting) => return Target::Waiting,
            _ => {}
        }
        let key = BatchKey {
            shape: part.shape,
            image,
        };
        match z {
            Some(z) => Target::Layer(z, key),
            None if part.transparency > 0.0 || part.layer != Layer::Base => Target::Transparent,
            None => Target::Opaque(key),
        }
    }

    /// Whether a shape's mesh is on the GPU, queueing a mesh file (or
    /// building a truss) the first time one is drawn. A file the scene
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
            // Built the first time a truss of this shape is drawn: a few
            // kilobytes, uploaded at once and shared by every such truss.
            DrawShape::Truss(_) => {
                let gpu = GpuMesh::upload(&self.device, &mesh::build(shape));
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

    fn image_readiness(&mut self, hash: ImageHash, scene: Option<&Scene>) -> Readiness {
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

    /// Free the slot an entry was drawn in.
    pub(crate) fn vacate(&mut self, placement: Placement) {
        match placement {
            Placement::Hidden => {}
            Placement::Opaque { batch, slot } => self.opaque.vacate(batch, slot),
            Placement::Layer { batch, slot } => self.layers.vacate(batch, slot),
            Placement::Transparent(t) => {
                self.transparent_parts[t] = None;
                self.transparent_free.push(t);
            }
        }
    }

    /// Take a slot to draw an entry in.
    pub(crate) fn occupy(&mut self, part: &PartInstance, target: Target) -> Placement {
        let instance = Instance::new(part, self.bounds(part.shape));
        match target {
            Target::Hidden | Target::Waiting => Placement::Hidden,
            Target::Transparent => {
                let t = match self.transparent_free.pop() {
                    Some(t) => t,
                    None => {
                        self.transparent_parts.push(None);
                        self.transparent_parts.len() - 1
                    }
                };
                self.transparent_parts[t] = Some(*part);
                Placement::Transparent(t)
            }
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
