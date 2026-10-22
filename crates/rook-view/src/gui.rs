//! The DataModel's GUI for the renderer: what its screens and world
//! canvases draw, and the images they ask for, loaded with the scene's
//! other files (docs/behavior/gui/images.md).

use rook_content::decode::image::Image;
use rook_engine::DataModel;
use rook_gui::{Gui, GuiImage};
use rook_render::Renderer;

use crate::*;

impl SceneCache {
    /// Give the renderer the DataModel's GUI when it was laid out again
    /// since last time (laying it out first if it changed), and where its
    /// world GUI is now. Returns whether it was laid out again.
    pub fn show_gui(&mut self, dm: &DataModel, renderer: &mut Renderer) -> bool {
        dm.runtime().sync_gui();
        let mut world = dm.runtime().world_mut();
        let world = &mut *world;
        let generation = world.gui.generation();
        let changed = self.gui_generation != Some(generation);
        if changed {
            self.gui_generation = Some(generation);
            let screen = world.gui.draw(&world.dom);
            let canvases = world.gui.draw_world(&world.dom);
            renderer.set_gui(&screen, &canvases, &self.scene.images);
        }
        self.feed_gui(&mut world.gui);
        renderer.place_world_gui(world.gui.world_places(&world.dom));
        changed
    }

    /// Lay out and draw the GUI, so it asks for the images it shows, and
    /// hand it those that arrived (screenshots, before the first frame).
    pub(crate) fn draw_gui_for_images(&mut self, dm: &DataModel) {
        dm.runtime().sync_gui();
        let mut world = dm.runtime().world_mut();
        let world = &mut *world;
        world.gui.draw(&world.dom);
        world.gui.draw_world(&world.dom);
        self.feed_gui(&mut world.gui);
    }

    /// Hand `gui` the images that arrived, and ask for those it wants.
    fn feed_gui(&mut self, gui: &mut Gui) {
        for (url, image) in self.gui_arrived.drain(..) {
            gui.image_arrived(&url, image);
        }
        for url in gui.take_image_requests() {
            let Some(r) = ContentRef::parse(&url) else {
                gui.image_arrived(&url, None);
                continue;
            };
            // GUI is drawn over everything, so it loads first.
            let state = self.files.state(&r, Want::Image, 0.0);
            match state {
                FileState::Loading => self
                    .gui_waiting
                    .entry((r, Want::Image))
                    .or_default()
                    .push(url),
                FileState::Loaded(hash) => {
                    let image = self.scene.images.get(&ImageHash(hash));
                    gui.image_arrived(&url, image.map(|i| gui_image(ImageHash(hash), i)));
                }
                FileState::Failed => gui.image_arrived(&url, None),
            }
        }
    }

    /// Note that a file the GUI waits for finished.
    pub(crate) fn gui_file_done(&mut self, key: &FileRef, image: Option<GuiImage>) {
        for url in self.gui_waiting.remove(key).unwrap_or_default() {
            self.gui_arrived.push((url, image));
        }
    }
}

/// What the GUI needs to know of a loaded image.
pub(crate) fn gui_image(hash: ImageHash, image: &Image) -> GuiImage {
    GuiImage {
        hash: hash.0,
        width: image.width,
        height: image.height,
    }
}
