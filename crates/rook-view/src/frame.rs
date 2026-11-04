//! Each frame: the scene brought up to date with its DataModel, and the
//! renderer with the scene.

use std::sync::Arc;

use glam::Vec3;
use rook_content::Resolver;
use rook_engine::{DataModel, RenderChanges};
use rook_render::Renderer;

use crate::{Floors, SceneCache, SceneChange};

impl SceneCache {
    /// The scene for a DataModel as it is now. Its pending render changes
    /// are already drawn, so they're dropped (otherwise the first frame
    /// would rebuild the whole scene again: a new DataModel's changes say
    /// "rebuild everything"), all but scripts' effect calls, which aren't
    /// in the tree. Files near `camera` load first.
    pub fn for_data_model(dm: &mut DataModel, content: &Arc<Resolver>, camera: Vec3) -> Self {
        let calls = dm.take_render_changes().effect_calls;
        let mut world = dm.runtime().world_mut();
        let world = &mut *world;
        let mut floors = Floors::default();
        floors.build(&mut world.dom, &mut world.physics);
        let dom = &world.dom;
        let mut scene = Self::new(dom, floors, content.clone(), camera);
        scene.take_effect_calls(dom, &calls);
        scene
    }

    /// Wait for every mesh file and image the scene and its GUI ask for
    /// (images on a mesh are asked for once its file has loaded), and grow
    /// the grass around `camera`: what a screenshot shows.
    pub fn load_all(&mut self, dm: &DataModel, camera: Vec3) {
        loop {
            self.draw_gui_for_images(dm);
            let dom = &dm.runtime().world().dom;
            if self.poll_files(dom, true).is_empty() && self.gui_arrived.is_empty() {
                break;
            }
        }
        self.update_terrain(camera, true);
        self.update_grass(camera);
    }

    /// Apply a DataModel's changes since the last frame, the FloorWires'
    /// floors first: a wire whose floor changed draws again with the
    /// frame's other changes.
    pub(crate) fn apply_data_model(
        &mut self,
        dm: &DataModel,
        mut changes: RenderChanges,
    ) -> SceneChange {
        if changes.is_empty() {
            return SceneChange::None;
        }
        rook_types::count!(
            ViewPartsChanged,
            changes.placed.len() + changes.parts.len() + changes.collected.len()
        );
        rook_types::count!(ViewPartsMoved, changes.moved.len());
        let mut world = dm.runtime().world_mut();
        let world = &mut *world;
        if changes.full {
            self.floors.build(&mut world.dom, &mut world.physics);
        } else {
            let dom = &mut world.dom;
            for wire in self.floors.update(dom, &mut world.physics, &changes) {
                changes.effects.insert(wire);
                changes.parts.extend(dom.ref_property(wire, "From"));
            }
        }
        self.apply(&world.dom, &changes)
    }

    /// Bring the scene up to date with a DataModel's changes and the mesh
    /// files and images that finished loading since the last frame, then
    /// the renderer with the scene and the GUI. Returns whether anything
    /// changed.
    pub fn refresh(&mut self, dm: &mut DataModel, renderer: &mut Renderer, camera: Vec3) -> bool {
        let changes = dm.take_render_changes();
        let gui = self.show_gui(dm, renderer, Some(&changes));
        let change = self.apply_data_model(dm, changes);
        let dom = &dm.runtime().world().dom;
        self.follow_camera(dom, camera);
        let mut loaded = self.poll_files(dom, false);
        loaded.extend(self.update_terrain(camera, false));
        loaded.extend(self.update_grass(camera));
        rook_types::count!(ViewPartsChanged, loaded.len());
        let change = match change {
            // A skybox image arrived: the lighting alone changed.
            SceneChange::None if loaded.is_empty() && self.take_lighting_change() => {
                SceneChange::Parts {
                    changed: Vec::new(),
                    moved: Vec::new(),
                    lighting: true,
                }
            }
            _ if loaded.is_empty() => change,
            SceneChange::None => SceneChange::Parts {
                changed: loaded,
                moved: Vec::new(),
                lighting: false,
            },
            SceneChange::Parts {
                mut changed,
                moved,
                lighting,
            } => {
                changed.extend(loaded);
                SceneChange::Parts {
                    changed,
                    moved,
                    lighting,
                }
            }
            SceneChange::Full => SceneChange::Full,
        };
        rook_types::gauge!(ViewEntries, self.scene.parts.len());
        match &change {
            SceneChange::Parts { changed, moved, .. } => {
                rook_types::count!(ViewEntriesRewritten, changed.len());
                rook_types::count!(ViewEntriesMoved, moved.len());
            }
            SceneChange::Full => rook_types::count!(ViewEntriesRewritten, self.scene.parts.len()),
            SceneChange::None => {}
        }
        let lights = self.lights.take_changes();
        let effects = self.effects.take_changes();
        match &change {
            SceneChange::None => {
                if lights.is_empty() && effects.is_empty() {
                    return gui;
                }
            }
            SceneChange::Parts {
                changed,
                moved,
                lighting,
            } => {
                if *lighting {
                    renderer.update_lighting(&self.scene);
                }
                if !changed.is_empty() {
                    renderer.update_parts(&self.scene, changed);
                }
                renderer.move_parts(&self.scene, moved);
            }
            SceneChange::Full => renderer.set_scene(&self.scene),
        }
        // A new scene takes every light and effect anyway.
        if !matches!(change, SceneChange::Full) {
            if !lights.is_empty() {
                renderer.update_lights(&self.scene, &lights);
            }
            if !effects.is_empty() {
                renderer.update_effects(&self.scene, &effects);
            }
        }
        for mesh in self.take_dropped_meshes() {
            renderer.drop_mesh(mesh);
        }
        true
    }
}
