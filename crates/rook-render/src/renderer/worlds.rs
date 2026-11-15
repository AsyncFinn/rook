//! The worlds a frame draws: Workspace's, and each ViewportFrame's own
//! ([`viewports`](super::viewports)). A world's instances are its own
//! batches and transparent parts, culled, sorted and drawn for its own
//! camera, as Godot's scenarios and Unreal's FScenes are; the mesh pool,
//! images and pipelines are shared by all of them.

use super::*;

#[derive(Debug, Default)]
pub(crate) struct World {
    pub(crate) opaque: Batches,
    pub(crate) layers: Batches,
    /// Transparent parts, sorted back to front as its view changes.
    pub(crate) transparent: TransparentParts,
    /// How many scene entries are drawn in it (not hidden).
    pub(crate) entries: usize,
}

impl World {
    pub(crate) fn clear(&mut self) {
        self.opaque.clear();
        self.layers.clear();
        self.transparent.clear();
        self.entries = 0;
    }
}

impl Renderer {
    /// The world `space`'s entries are drawn in, if it has one yet.
    pub(crate) fn world(&self, space: Space) -> Option<&World> {
        match space {
            Space::Workspace => Some(&self.world),
            Space::Viewport(key) => self.viewports.world(key),
        }
    }

    /// The world `space`'s entries are drawn in, to change: a
    /// ViewportFrame's is made the first time, and its picture is drawn
    /// again.
    pub(crate) fn world_mut(&mut self, space: Space) -> &mut World {
        match space {
            Space::Workspace => &mut self.world,
            Space::Viewport(key) => self.viewports.world_mut(key),
        }
    }
}
