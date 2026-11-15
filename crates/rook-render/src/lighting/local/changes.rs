//! Taking the scene's changes: lights coming, going and moving (and the
//! hash built again when they move far), the parts blocking light and the
//! terrain.

use super::*;

/// What changed since the voxels were filled, besides the blockers.
#[derive(Debug, Default)]
pub(super) struct Dirty {
    /// The lights were hashed again: everything.
    pub(super) all: bool,
    /// The voxels lights that changed reach, before and after (the
    /// world's voxels).
    pub(super) lights: Vec<Span>,
}

impl Dirty {
    pub(super) fn is_empty(&self) -> bool {
        !self.all && self.lights.is_empty()
    }
}

impl GpuLights {
    /// Take a new scene's lights.
    pub(crate) fn replace(&mut self, device: &wgpu::Device, scene: &[LocalLight]) {
        self.lights = scene.iter().map(GpuLight::of).collect();
        self.shadowed = self.lights.iter().filter(|l| l.shadows()).count();
        self.dirty.all = true;
        self.rehash(device);
    }

    /// Take the lights at `changed` (indices into `scene`, which may have
    /// grown or shrunk) again. Only they are written, and voxels fill again
    /// only where a lit one was or is ([`GpuLight::dark`]); the hash is
    /// built again only when lights come or go, or one lights up, moves past
    /// [`SLACK`] from where it was hashed or reaches further.
    pub(crate) fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &[LocalLight],
        changed: &[usize],
    ) {
        let mut rehash = scene.len() != self.lights.len();
        for l in self.lights.drain(scene.len().min(self.lights.len())..) {
            self.shadowed -= usize::from(l.shadows());
            self.dirty
                .lights
                .extend((!l.dark()).then(|| lit_voxels(&l)));
        }
        // New lights are among `changed`.
        self.lights.resize(scene.len(), GpuLight::zeroed());
        let mut written = Vec::new();
        for &i in changed.iter().filter(|&&i| i < scene.len()) {
            let new = GpuLight::of(&scene[i]);
            let old = self.lights[i];
            if old == new {
                continue;
            }
            self.shadowed = self.shadowed - usize::from(old.shadows()) + usize::from(new.shadows());
            self.lights[i] = new;
            self.dirty
                .lights
                .extend([old, new].iter().filter(|l| !l.dark()).map(lit_voxels));
            // The hash lists no dark light, and one that goes dark lights
            // nothing where it's still listed.
            rehash |= !new.dark()
                && (old.dark()
                    || new.position[3] > old.position[3]
                    || (self.hashed_at.get(i)).is_none_or(|at| new.at().distance(*at) > SLACK));
            written.push(i);
        }
        if rehash {
            self.rehash(device);
            return;
        }
        if written.is_empty() {
            return;
        }
        profiling::scope!("GpuLights::update");
        self.shadows = self.shadowed > 0;
        let size = std::mem::size_of::<GpuLight>();
        for i in written {
            let bytes = bytemuck::bytes_of(&self.lights[i]);
            queue.write_buffer(&self.light_buffer, (i * size) as u64, bytes);
        }
    }

    /// Build the hash and the buffers again for the lights as they are.
    /// The voxels fill again only where lights changed (the caller marks
    /// those): the hash finds the same lights for every other voxel.
    pub(super) fn rehash(&mut self, device: &wgpu::Device) {
        profiling::scope!("GpuLights::rehash");
        self.shadows = self.shadowed > 0;
        self.hash = LightHash::build(&self.lights);
        self.hashed_at = self.lights.iter().map(GpuLight::at).collect();
        (self.light_buffer, self.bucket_buffer, self.entry_buffer) =
            buffers(device, &self.lights, &self.hash);
        self.fill_group = fill_group(
            device,
            &self.fill_layout,
            [
                &self.params,
                &self.light_buffer,
                &self.bucket_buffer,
                &self.entry_buffer,
            ],
            (&self.sky_buffers, &self.cover),
            [&self.voxels, &self.sun_voxels],
        );
    }

    /// Forget the parts blocking light (the scene is replaced).
    pub(crate) fn clear_blockers(&mut self) {
        self.blockers.clear();
    }

    /// Take the terrain's voxels the scene sends ([`TerrainVoxels`]).
    pub(crate) fn update_terrain(&mut self, voxels: TerrainVoxels) {
        self.blockers.update_terrain(voxels);
    }

    /// Take the parts at `changed` (indices into `scene.parts`) as they
    /// block light now. Only noted while nothing reads the blockers (the
    /// sky lights nothing, the sun shadows through the shadow map and no
    /// light has `Shadows`), and taken once something does.
    pub(crate) fn update_blockers(&mut self, scene: &Scene, changed: &[usize]) {
        let l = &scene.lighting;
        let used = l.environment_diffuse > 0.0 || l.sun_in_voxels() || self.shadows;
        self.blockers.update(scene, changed, used);
    }
}
