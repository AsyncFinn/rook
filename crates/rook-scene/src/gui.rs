//! The DataModel's GUI for the renderer: what its screens and world
//! canvases draw, and the images they ask for, loaded with the scene's
//! other files (docs/behavior/gui/images.md).

use rook_content::decode::image::{Encoding, Image};
use rook_engine::{DataModel, RenderChanges};
use rook_gui::{Gui, GuiImage};
use rook_render::Renderer;
use rook_types::{GuiQuad, WorldCanvas, WorldPlace};

use crate::*;

/// The GUI a frame hands the renderer (see [`crate::ToRender`]).
#[derive(Debug)]
pub(crate) struct GuiUpdate {
    /// The screen's pieces and the world canvases, when the GUI was laid
    /// out again.
    drawn: Option<DrawnGui>,
    /// Where world canvases are now, by index: all of them, or those
    /// that moved.
    places: Vec<(usize, Option<WorldPlace>)>,
}

/// The screen's pieces, bottom first, and the world GUI's canvases.
type DrawnGui = (Arc<[Arc<[GuiQuad]>]>, Vec<Arc<WorldCanvas>>);

impl GuiUpdate {
    pub(crate) fn laid_out(&self) -> bool {
        self.drawn.is_some()
    }

    pub(crate) fn apply(self, scene: &Scene, renderer: &mut Renderer) {
        if let Some((screen, canvases)) = &self.drawn {
            renderer.set_gui(screen, canvases, &scene.images);
        }
        renderer.place_world_gui(self.places);
    }
}

impl SceneCache {
    /// Give the renderer the DataModel's GUI when it was laid out again
    /// since last time (laying it out first if it changed), and where its
    /// world GUI is now: all of it when laid out again (or with no
    /// `changes` to go by), else what moved with the parts and
    /// Attachments `changes` name. Returns whether it was laid out again.
    pub fn show_gui(
        &mut self,
        dm: &DataModel,
        renderer: &mut Renderer,
        changes: Option<&RenderChanges>,
    ) -> bool {
        let gui = self.update_gui(dm, changes);
        let laid_out = gui.laid_out();
        gui.apply(&self.scene, renderer);
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
        let drawn = (self.gui_generation != Some(generation)).then(|| {
            self.gui_generation = Some(generation);
            (world.gui.draw(&world.dom), world.gui.draw_world(&world.dom))
        });
        self.feed_gui(&mut world.gui);
        let places = match changes {
            Some(changes) if drawn.is_none() && !changes.full => world
                .gui
                .world_places_moved(&world.dom, changes.maybe_moved()),
            _ => world
                .gui
                .world_places(&world.dom)
                .into_iter()
                .enumerate()
                .collect(),
        };
        GuiUpdate { drawn, places }
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

    /// A loaded GUI image's pixels, four bytes each, rows top first (for
    /// the cursor), when it's stored uncompressed.
    pub fn image_pixels(&self, image: GuiImage) -> Option<(u32, u32, Vec<u8>)> {
        let found = self.scene.images.get(&ImageHash(image.hash))?;
        let rgba = matches!(found.encoding, Encoding::Rgba8).then(|| found.mips.first())??;
        Some((found.width, found.height, rgba.clone()))
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
                    let image = self.scene.images.get(&ImageHash(hash));
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

pub(crate) fn gui_image(hash: ImageHash, image: &Image) -> GuiImage {
    GuiImage {
        hash: hash.0,
        width: image.width,
        height: image.height,
    }
}
