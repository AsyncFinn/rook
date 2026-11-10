use std::sync::Arc;

use glam::Vec3;
use rook_content::Resolver;
use rook_engine::{ScreenPoint, console};
use rook_lua::MessageType;
use rook_render::Camera;
use rook_scene::SceneCache;

use crate::game::Game;
use crate::stats::thousands;

#[derive(Debug, Clone)]
pub struct Start {
    pub width: u32,
    pub height: u32,
    /// Where the camera starts, instead of the place's saved camera.
    pub camera: Option<Vec3>,
    pub look_at: Option<Vec3>,
    /// Vertical field of view in degrees, instead of the saved camera's.
    pub fov: Option<f32>,
    /// Seconds the place's scripts run before it's shown.
    pub seconds: f64,
    /// Points the player clicks after `seconds`, in window pixels.
    pub clicks: Vec<ScreenPoint>,
}

impl Default for Start {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            camera: None,
            look_at: None,
            fov: None,
            seconds: 0.0,
            clicks: Vec::new(),
        }
    }
}

/// Get a loaded game ready to show: run `seconds`, click, pick the camera,
/// build the scene (files near the camera load first) and say what
/// loaded.
pub fn prepare(game: &mut Game, start: &Start, content: &Arc<Resolver>) -> (SceneCache, Camera) {
    let load_time = game.load_start.elapsed().as_secs_f64();
    // GUI lays out in the window it's drawn in.
    game.set_viewport(start.width, start.height);
    if start.seconds > 0.0 {
        game.run_for(start.seconds);
    }
    for point in &start.clicks {
        for event in point.click() {
            game.input(event);
            game.run_for(0.15);
        }
    }
    if !start.clicks.is_empty() {
        game.run_for(1.0);
    }
    // The camera the place was saved with (clients start with a fresh one).
    let mut camera = game.saved_camera;
    if let Some(pos) = start.camera {
        let target = start.look_at.unwrap_or(pos + camera.forward());
        camera = Camera {
            fov_y: camera.fov_y,
            ..Camera::look_at(pos, target)
        };
    } else if let Some(target) = start.look_at {
        camera = Camera {
            fov_y: camera.fov_y,
            ..Camera::look_at(camera.position, target)
        };
    } else if let Some(view) = game.view().scripted_camera() {
        // What the window would show: the game's camera.
        camera = Camera {
            position: view.cframe.position,
            rotation: view.cframe.rotation,
            fov_y: view.fov,
            focus: Some(view.focus),
        };
    }
    if let Some(fov) = start.fov {
        camera.fov_y = fov;
    }
    let scene = SceneCache::for_data_model(game.view_mut(), content, camera.position);
    let terrain = scene.terrain_meshes();
    let terrain = if terrain > 0 {
        format!(", terrain in {} meshes", thousands(terrain))
    } else {
        String::new()
    };
    console::print_message(
        MessageType::Info,
        &format!(
            "Loaded {} in {load_time:.2} s ({} parts{terrain})",
            game.name,
            thousands(scene.scene.parts.len() - scene.terrain_meshes())
        ),
    );
    (scene, camera)
}
