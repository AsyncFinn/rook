use std::sync::Arc;

use glam::Vec3;
use rook_content::Resolver;
use rook_content::decode::image::Image;
use rook_engine::{DataModel, RenderChanges};
use rook_render::{EffectSlots, ImageHash, MeshHash, Renderer, TerrainVoxels};

use crate::gui::GuiUpdate;
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

    /// Give `renderer`, new or drawing something else, the whole scene
    /// and the images it hasn't had.
    pub fn show_all(&mut self, renderer: &mut Renderer) {
        renderer.take_images(self.images.take_fresh());
        renderer.set_scene(&self.scene);
        renderer.update_terrain_voxels(self.light_voxels());
    }

    /// How many distinct images have loaded.
    pub fn images_loaded(&self) -> usize {
        self.images.len()
    }

    /// Bring the scene up to date with a DataModel's changes and the mesh
    /// files and images that finished loading since the last frame, then
    /// the renderer with the scene and the GUI. Returns whether anything
    /// changed.
    pub fn refresh(&mut self, dm: &mut DataModel, renderer: &mut Renderer, camera: Vec3) -> bool {
        let update = self.update(dm, camera);
        let changed = update.changed();
        update.apply(self, renderer);
        changed
    }

    /// Bring the scene up to date with a DataModel's changes and the mesh
    /// files and images that finished loading since the last frame; what
    /// the renderer must take for it. Needs no renderer, so it runs while
    /// the render thread draws the frame before
    /// (docs/design/architecture.md#frame-order).
    pub fn update(&mut self, dm: &mut DataModel, camera: Vec3) -> ToRender {
        let changes = dm.take_render_changes();
        let gui = self.update_gui(dm, Some(&changes));
        let change = self.apply_data_model(dm, changes);
        let dom = &dm.runtime().world().dom;
        self.follow_camera(dom, camera);
        let mut loaded = self.poll_files(dom, false);
        loaded.extend(self.update_terrain(camera, false));
        loaded.extend(self.update_grass(camera));
        rook_types::count!(ViewPartsChanged, loaded.len());
        let change = with_arrivals(change, loaded, self.take_lighting_change());
        rook_types::gauge!(ViewEntries, self.scene.parts.len());
        match &change {
            SceneChange::Parts { changed, moved, .. } => {
                rook_types::count!(ViewEntriesRewritten, changed.len());
                rook_types::count!(ViewEntriesMoved, moved.len());
            }
            SceneChange::Full => rook_types::count!(ViewEntriesRewritten, self.scene.parts.len()),
            SceneChange::None => {}
        }
        ToRender {
            gui,
            change,
            lights: self.lights.take_changes(),
            terrain: self.take_light_voxels(),
            effects: self.effects.take_changes(),
            dropped: self.take_dropped_meshes(),
            images: self.images.take_fresh(),
        }
    }
}

/// `change` with what arrived during the frame: the parts that files
/// reached, and whether a skybox image changed the lighting. Either comes
/// on top of whatever else changed, which in a busy place is every frame.
fn with_arrivals(change: SceneChange, loaded: Vec<usize>, lighting: bool) -> SceneChange {
    match change {
        SceneChange::None if loaded.is_empty() && !lighting => SceneChange::None,
        SceneChange::None => SceneChange::Parts {
            changed: loaded,
            moved: Vec::new(),
            lighting,
            highlights: false,
        },
        SceneChange::Parts {
            mut changed,
            moved,
            lighting: changed_lighting,
            highlights,
        } => {
            changed.extend(loaded);
            SceneChange::Parts {
                changed,
                moved,
                lighting: changed_lighting || lighting,
                highlights,
            }
        }
        SceneChange::Full => SceneChange::Full,
    }
}

/// A frame's changes for the renderer: what [`SceneCache::update`] found,
/// handed over by [`ToRender::apply`]. The hand-over is pipelined
/// rendering's extract, the one moment both the main thread and the
/// renderer hold the frame, so it only copies what changed.
#[must_use = "the renderer draws the last frame's scene until it's applied"]
#[derive(Debug)]
pub struct ToRender {
    gui: GuiUpdate,
    change: SceneChange,
    lights: Vec<usize>,
    /// What changed of the terrain's voxels, for the light voxels.
    terrain: Option<TerrainVoxels>,
    effects: EffectSlots,
    dropped: Vec<MeshHash>,
    /// Images loaded since the last hand-over: the renderer takes their
    /// pixels, and the scene keeps none.
    images: Vec<(ImageHash, Arc<Image>)>,
}

impl ToRender {
    pub fn changed(&self) -> bool {
        self.gui.laid_out()
            || !matches!(self.change, SceneChange::None)
            || !self.lights.is_empty()
            || self.terrain.is_some()
            || !self.effects.is_empty()
    }

    /// Give `renderer` the changes, reading them from `cache`, which
    /// mustn't have changed since the update that made them.
    pub fn apply(self, cache: &SceneCache, renderer: &mut Renderer) {
        profiling::scope!("ToRender::apply");
        let scene = &cache.scene;
        // First: what follows may draw them.
        renderer.take_images(self.images);
        self.gui.apply(renderer);
        match &self.change {
            SceneChange::None => {}
            SceneChange::Parts {
                changed,
                moved,
                lighting,
                highlights,
            } => {
                if *lighting {
                    renderer.update_lighting(scene);
                }
                if *highlights {
                    renderer.update_highlights(scene);
                }
                if !changed.is_empty() {
                    renderer.update_parts(scene, changed);
                }
                renderer.move_parts(scene, moved);
            }
            SceneChange::Full => renderer.set_scene(scene),
        }
        // A new scene takes every light and effect anyway.
        if !matches!(self.change, SceneChange::Full) {
            if !self.lights.is_empty() {
                renderer.update_lights(scene, &self.lights);
            }
            if !self.effects.is_empty() {
                renderer.update_effects(scene, &self.effects);
            }
        }
        // A new scene keeps the terrain's voxels the renderer has.
        if let Some(voxels) = self.terrain {
            renderer.update_terrain_voxels(voxels);
        }
        for mesh in self.dropped {
            renderer.drop_mesh(mesh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved() -> SceneChange {
        SceneChange::Parts {
            changed: Vec::new(),
            moved: vec![7],
            lighting: false,
            highlights: false,
        }
    }

    /// A skybox image reached the renderer only on a frame nothing else
    /// changed, so a place that moves something every frame kept its plain
    /// sky.
    #[test]
    fn an_arrived_skybox_comes_with_parts_that_moved() {
        assert_eq!(
            with_arrivals(moved(), vec![3], true),
            SceneChange::Parts {
                changed: vec![3],
                moved: vec![7],
                lighting: true,
                highlights: false,
            }
        );
    }

    #[test]
    fn a_quiet_frame_stays_quiet() {
        assert_eq!(
            with_arrivals(SceneChange::None, Vec::new(), false),
            SceneChange::None
        );
        assert_eq!(
            with_arrivals(SceneChange::None, Vec::new(), true),
            SceneChange::Parts {
                changed: Vec::new(),
                moved: Vec::new(),
                lighting: true,
                highlights: false,
            }
        );
    }
}
