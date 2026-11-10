use std::path::Path;

use anyhow::Context;
use rook_engine::console;
use rook_lua::MessageType;
use rook_render::{Camera, Renderer};
use rook_scene::{Installed, SceneCache};

use crate::game::Game;
use crate::start::Start;

pub fn screenshot(
    game: &mut Game,
    mut scene: SceneCache,
    camera: Camera,
    start: &Start,
    install: &Installed,
    out: &Path,
) -> anyhow::Result<()> {
    // A screenshot isn't a frame: it waits for the place's mesh files
    // and images, and their uploads.
    scene.load_all(game.view(), camera.position);
    // GUI laid out again around the images that arrived fires Changed,
    // and scripts answer it (Roblox's React GUI re-renders): a few
    // frames let that settle, as it would in a window, before the shot.
    game.run_for(0.25);
    scene.load_all(game.view(), camera.position);
    let mut renderer = Renderer::headless()?;
    crate::note_gpu(&renderer);
    install.apply(&mut renderer);
    renderer.set_scene(&scene.scene);
    scene.show_gui(game.view(), &mut renderer, None);
    if !start.clicks.is_empty() {
        let (dm, look) = (game.view(), game.view().cursor_look());
        scene.show_cursor(dm, &mut renderer, &look, Some(dm.mouse_position()));
    }
    renderer.finish_uploads();
    // Effects as they look once going: Smoke's puffs live 5 seconds.
    let (width, height) = (start.width, start.height);
    renderer.settle_particles(6.0, &camera, width, height);
    let pixels = renderer.render_to_image(width, height, &camera)?;
    let image = image::RgbaImage::from_raw(width, height, pixels)
        .context("the frame read back the wrong size")?;
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png)?;
    rook_platform::get()
        .files
        .write(out, png.get_ref())
        .with_context(|| format!("Couldn't write {}", out.display()))?;
    console::print_message(
        MessageType::Info,
        &format!("Saved screenshot to {}", out.display()),
    );
    Ok(())
}
