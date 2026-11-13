//! The DataModel's GUI for the renderer: what its screens and world
//! canvases draw, and the images they ask for, loaded with the scene's
//! other files (docs/behavior/gui/images.md).

use rook_engine::{DataModel, RenderChanges};
use rook_gui::{Gui, GuiImage};
use rook_render::Renderer;
use rook_types::{GuiQuad, WorldCanvases, WorldPlace};

use crate::content::images::ImageInfo;
use crate::*;

/// The GUI a frame hands the renderer (see [`crate::ToRender`]).
#[derive(Debug)]
pub(crate) struct GuiUpdate {
    /// The screen's pieces, when they were laid out or drawn again.
    screen: Option<Arc<[Arc<[GuiQuad]>]>>,
    /// The world canvases that changed, or all of them.
    world: Option<WorldCanvases>,
    /// Where world canvases are now, by index: all of them, or those
    /// that moved.
    places: Vec<(usize, Option<WorldPlace>)>,
}

impl GuiUpdate {
    pub(crate) fn laid_out(&self) -> bool {
        self.screen.is_some() || self.world.is_some()
    }

    pub(crate) fn apply(self, renderer: &mut Renderer) {
        if self.laid_out() {
            renderer.set_gui(self.screen.as_ref(), self.world.as_ref());
        }
        renderer.place_world_gui(self.places);
    }
}

impl SceneCache {
    /// Give the renderer the DataModel's GUI when it was laid out again
    /// since last time (laying it out first if it changed), and where its
    /// world GUI is now: all of it when which GUIs draw may have changed
    /// (or with no `changes` to go by), else the canvases drawn again and
    /// what moved with the parts and Attachments `changes` name. Returns
    /// whether it was laid out again.
    pub fn show_gui(
        &mut self,
        dm: &DataModel,
        renderer: &mut Renderer,
        changes: Option<&RenderChanges>,
    ) -> bool {
        let gui = self.update_gui(dm, changes);
        let laid_out = gui.laid_out();
        gui.apply(renderer);
        laid_out
    }

    /// The DataModel's GUI as it is now, laid out first if it changed
    /// (see [`SceneCache::show_gui`]).
    pub(crate) fn update_gui(
        &mut self,
        dm: &DataModel,
        changes: Option<&RenderChanges>,
    ) -> GuiUpdate {
        dm.runtime().sync_gui();
        let mut world = dm.runtime().world_mut();
        let world = &mut *world;
        let generation = world.gui.generation();
        // A scene that has drawn none of it yet has no place for any.
        let first = self.gui_generation.is_none();
        let screen = (self.gui_generation != Some(generation)).then(|| {
            self.gui_generation = Some(generation);
            world.gui.draw(&world.dom)
        });
        let canvases = world.gui.update_world(&world.dom, first);
        self.feed_gui(&mut world.gui);
        let places = match changes {
            Some(changes) if canvases.is_none() && !changes.full => world
                .gui
                .world_places_moved(&world.dom, changes.maybe_moved()),
            Some(changes) => {
                let all = first || changes.full;
                (world.gui).world_places_drawn(&world.dom, changes.maybe_moved(), all)
            }
            None => world.gui.world_places_drawn(&world.dom, [], true),
        };
        GuiUpdate {
            screen,
            world: canvases,
            places,
        }
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
                    let image = self.images.info(ImageHash(hash));
                    gui.image_arrived(&url, image.map(|i| gui_image(ImageHash(hash), i)));
                }
                FileState::Failed => gui.image_arrived(&url, None),
            }
        }
    }

    pub(crate) fn gui_file_done(&mut self, key: &FileRef, image: Option<GuiImage>) {
        for url in self.gui_waiting.remove(key).unwrap_or_default() {
            self.gui_arrived.push((url, image));
        }
    }
}

pub(crate) fn gui_image(hash: ImageHash, image: ImageInfo) -> GuiImage {
    GuiImage {
        hash: hash.0,
        width: image.width,
        height: image.height,
    }
}
