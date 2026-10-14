//! What the renderer takes from the user's Roblox install.

use crate::*;

/// What the renderer takes from the user's Roblox install, read from disk
/// at startup: the Head MeshType's mesh and the surface patterns. Without
/// an install (or the files), Rook draws its own.
pub(crate) struct Installed {
    pub(crate) head: Option<Mesh>,
    /// `textures/studs.dds`: the studs, lattice, inlet and universal tiles.
    pub(crate) surfaces: Option<Image>,
}

impl Installed {
    pub(crate) fn find(content: &Resolver) -> Self {
        let read = |path: &str| {
            let fetched = content.resolve_local(&ContentRef::File(path.into()))?;
            Some((fetched.source.location, fetched.bytes))
        };
        let warn = |location: &str, e: &dyn std::fmt::Display, what: &str| {
            console::print_message(
                MessageType::Warning,
                &format!("{location}: {e}; drawing Rook's own {what}"),
            );
        };
        let head = read("avatar/heads/head.mesh")
            .and_then(|(at, bytes)| Mesh::decode(&bytes).map_err(|e| warn(&at, &e, "head")).ok());
        let surfaces = read("textures/studs.dds").and_then(|(at, bytes)| {
            Image::decode(&bytes)
                .map_err(|e| warn(&at, &e, "surfaces"))
                .ok()
        });
        Self { head, surfaces }
    }

    pub(crate) fn apply(&self, renderer: &mut Renderer) {
        if let Some(head) = &self.head {
            renderer.set_head_mesh(head);
        }
        if let Some(atlas) = &self.surfaces
            && !renderer.set_surface_atlas(atlas)
        {
            console::print_message(
                MessageType::Warning,
                "textures/studs.dds isn't laid out as expected; drawing Rook's own surfaces",
            );
        }
    }
}
