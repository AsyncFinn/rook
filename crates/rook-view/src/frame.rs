//! Each frame: the scene brought up to date with its DataModel, and the
//! renderer with the scene.

use std::sync::Arc;

use glam::Vec3;
use rook_content::Resolver;
use rook_engine::DataModel;
use rook_render::Renderer;

use crate::{SceneCache, SceneChange};

impl SceneCache {
    /// The scene for a DataModel as it is now. Its pending render changes
    /// are already drawn, so they're dropped (otherwise the first frame
    /// would rebuild the whole scene again: a new DataModel's changes say
    /// "rebuild everything"), all but scripts' effect calls, which aren't
    /// in the tree. Files near `camera` load first.
    pub fn for_data_model(dm: &mut DataModel, content: &Arc<Resolver>, camera: Vec3) -> Self {
        let calls = dm.take_render_changes().effect_calls;
        let dom = &dm.runtime().world().dom;
        let mut scene = Self::new(dom, content.clone(), camera);
        scene.take_effect_calls(dom, &calls);
        scene
    }

    /// Wait for every mesh file and image the scene asks for (images on
    /// a mesh are asked for once its file has loaded), and grow the grass
    /// around `camera`: what a screenshot shows.
    pub fn load_all(&mut self, dm: &DataModel, camera: Vec3) {
        let dom = &dm.runtime().world().dom;
        while !self.poll_files(dom, true).is_empty() {}
        self.update_grass(camera);
    }

    /// Bring the scene up to date with a DataModel's changes and the mesh
    /// files and images that finished loading since the last frame, then
    /// the renderer with the scene. Returns whether anything changed.
    pub fn refresh(&mut self, dm: &mut DataModel, renderer: &mut Renderer, camera: Vec3) -> bool {
        let changes = dm.take_render_changes();
        let dom = &dm.runtime().world().dom;
        let change = if changes.is_empty() {
            SceneChange::None
        } else {
            self.apply(dom, &changes)
        };
        self.follow_camera(dom, camera);
        let mut loaded = self.poll_files(dom, false);
        loaded.extend(self.update_grass(camera));
        let change = match change {
            // A skybox image arrived: the lighting alone changed.
            SceneChange::None if loaded.is_empty() && self.take_lighting_change() => {
                SceneChange::Parts {
                    changed: Vec::new(),
                    moved: Vec::new(),
                }
            }
            _ if loaded.is_empty() => change,
            SceneChange::None => SceneChange::Parts {
                changed: loaded,
                moved: Vec::new(),
            },
            SceneChange::Parts { mut changed, moved } => {
                changed.extend(loaded);
                SceneChange::Parts { changed, moved }
            }
            SceneChange::Full => SceneChange::Full,
        };
        match &change {
            SceneChange::None => return false,
            SceneChange::Parts { changed, moved } => {
                renderer.update_parts(&self.scene, changed);
                renderer.move_parts(&self.scene, moved);
            }
            SceneChange::Full => renderer.set_scene(&self.scene),
        }
        for mesh in self.take_dropped_meshes() {
            renderer.drop_mesh(mesh);
        }
        true
    }
}
