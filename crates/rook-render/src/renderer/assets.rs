//! Files the renderer is handed besides the scene's: mesh files let go,
//! images going up, and Roblox's own head mesh, surface atlas, water
//! normals and BRDF table in place of Rook's.

use super::*;

impl Renderer {
    pub(crate) fn upload_context(&self) -> UploadContext<'_> {
        UploadContext {
            device: &self.device,
            queue: &self.queue,
            image_layout: &self.image_layout,
            sampler: &self.image_sampler,
            view_formats: self.view_formats,
            block_compression: self.block_compression,
        }
    }

    /// Free a mesh file's GPU copy once no entry draws it (the player's
    /// grass tiles, dropped as the camera leaves them). An entry still
    /// drawing it draws nothing until the scene hands the mesh over again.
    pub fn drop_mesh(&mut self, mesh: MeshHash) {
        if let Some(gpu) = self.meshes.remove(&DrawShape::File(mesh)) {
            self.pool.remove(&gpu);
        }
    }

    /// Take images the scene loaded. Each waits until something draws it,
    /// goes up as others do, and is let go once it's up: the GPU's copy is
    /// the one kept.
    pub fn take_images(&mut self, images: impl IntoIterator<Item = (ImageHash, Arc<Image>)>) {
        for (hash, image) in images {
            self.images.park(hash, image);
        }
    }

    /// Draw the Head MeshType with this mesh (the user's own `head.mesh`)
    /// instead of Rook's. It must be the head at its own size, as Roblox's
    /// file is.
    pub fn set_head_mesh(&mut self, head: &Mesh) {
        let gpu = self.pool.add(&mesh::files::from_file(head));
        if let Some(old) = self.meshes.insert(DrawShape::Head, gpu) {
            self.pool.remove(&old);
        }
    }

    /// Draw the surface patterns from this atlas (the user's own
    /// `textures/studs.dds`) instead of Rook's. Returns false, keeping
    /// Rook's, when it isn't laid out as that file is.
    pub fn set_surface_atlas(&mut self, atlas: &Image) -> bool {
        let Some(patterns) = surfaces::from_studs_atlas(atlas) else {
            return false;
        };
        self.surfaces_view = layers_view(
            &self.upload_context(),
            "rook surface patterns",
            patterns.size,
            &patterns.layers,
        );
        self.rebuild_main_bind_group();
        true
    }

    /// Ripple water with these normal maps (the install's
    /// `textures/water/normal_01.dds` to `normal_25.dds`, X in alpha and Y
    /// in green), an animation of one frame each, instead of a flat
    /// surface. Returns false, keeping the flat one, unless they're all
    /// square and the same size.
    pub fn set_water_normals(&mut self, frames: &[Image]) -> bool {
        let Some(first) = frames.first() else {
            return false;
        };
        let size = first.width;
        if frames.iter().any(|f| f.width != size || f.height != size) {
            return false;
        }
        let layers: Vec<Vec<Vec<u8>>> = frames.iter().map(|f| f.mips.clone()).collect();
        self.water_view = layers_view(&self.upload_context(), "rook water normals", size, &layers);
        self.rebuild_main_bind_group();
        true
    }

    /// Reflect the sky by this table (the install's
    /// `textures/brdfLUT.dds`: `size` × `size` pairs of half floats)
    /// instead of Rook's fit of it. Returns false, keeping Rook's, when
    /// `rg16` isn't that size.
    pub fn set_brdf_table(&mut self, size: u32, rg16: &[u8]) -> bool {
        if size == 0 || rg16.len() != (size * size * 4) as usize {
            return false;
        }
        self.brdf_view = crate::lighting::sky::brdf_table(&self.device, &self.queue, size, rg16);
        self.rebuild_main_bind_group();
        true
    }
}
