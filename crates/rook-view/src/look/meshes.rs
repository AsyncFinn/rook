//! A MeshPart's mesh file, and a mesh's texture.

use super::*;

impl SceneCache {
    /// What a MeshPart draws.
    pub(super) fn mesh_part(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        size: Vec3,
        mut out: PartInstance,
    ) -> (PartInstance, Decals) {
        // Its own mesh file, scaled by Size / InitialSize; with an empty
        // MeshId, its Size box (observed).
        let url = mesh_id(dom, id);
        if url.is_empty() {
            self.settle(id, id, Settled::Empty);
            return (out, Decals::Faces);
        }
        let initial = dom
            .vector3_property(id, "InitialSize")
            .unwrap_or(Vec3::ZERO);
        let scale = mesh_part_scale(size, initial);
        match self.file(id, id, &url) {
            FileDraw::Mesh(hash) => {
                out.shape = DrawShape::File(hash);
                out.size = scale;
                self.mesh_texture(dom, id, id, &mut out, [1.0; 3]);
            }
            FileDraw::Empty => {}
            FileDraw::Nothing => {
                out.transparency = 1.0;
                return (out, Decals::None);
            }
        }
        // Decals on a MeshPart are projected onto its faces (observed).
        (out, Decals::Faces)
    }

    /// A mesh's texture (`TextureId` of `mesh`, a MeshPart's `TextureID`),
    /// observed in Studio 0.742 (2026-10-13): once loaded it replaces the
    /// part's color, times `tint` (the mesh's `VertexColor`); while it
    /// loads the mesh is black; if it fails, or there is none, the mesh
    /// keeps the part's color.
    pub(super) fn mesh_texture(
        &mut self,
        dom: &Dom,
        part: InstanceId,
        mesh: InstanceId,
        out: &mut PartInstance,
        tint: [f32; 3],
    ) {
        let url = texture_id(dom, mesh);
        if url.is_empty() {
            return;
        }
        match self.file_state(part, &url, Want::Image) {
            FileState::Loaded(hash) => {
                out.texture = Some(ImageHash(hash));
                out.color = tint;
            }
            FileState::Loading => out.color = [0.0; 3],
            FileState::Failed => {}
        }
    }
}
