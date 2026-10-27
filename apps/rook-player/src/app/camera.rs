//! The free-flying camera, and following the game's when a script drives
//! it.

use crate::*;

/// Yaw/pitch camera driven by the mouse and keyboard.
#[derive(Debug)]
pub(crate) struct FlyCamera {
    pub(crate) camera: Camera,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    /// Studs per second.
    pub(crate) speed: f32,
}

impl FlyCamera {
    pub(crate) fn new(camera: Camera) -> Self {
        let f = camera.forward();
        Self {
            yaw: (-f.x).atan2(-f.z),
            pitch: f.y.clamp(-1.0, 1.0).asin(),
            camera,
            speed: 40.0,
        }
    }

    pub(crate) fn update_rotation(&mut self) {
        self.pitch = self.pitch.clamp(-1.55, 1.55);
        self.camera.rotation = Mat3::from_euler(EulerRot::YXZ, self.yaw, self.pitch, 0.0);
    }

    /// Show what the game's camera shows; flying resumes from there.
    pub(crate) fn follow(&mut self, view: rook_engine::CameraView) {
        self.camera.position = view.cframe.position;
        self.camera.rotation = view.cframe.rotation;
        self.camera.fov_y = view.fov;
        let f = self.camera.forward();
        self.yaw = (-f.x).atan2(-f.z);
        self.pitch = f.y.clamp(-1.0, 1.0).asin();
    }
}

impl App {
    /// Fly the window's camera with the keys, and give the game it as
    /// Workspace.CurrentCamera, as Studio's editor camera does.
    pub(crate) fn fly(&mut self, dt: f32, typing: bool) {
        let mut dir = Vec3::ZERO;
        let rot = self.camera.camera.rotation;
        for (key, v) in [
            (KeyCode::KeyW, -rot.z_axis),
            (KeyCode::KeyS, rot.z_axis),
            (KeyCode::KeyA, -rot.x_axis),
            (KeyCode::KeyD, rot.x_axis),
            (KeyCode::KeyE, Vec3::Y),
            (KeyCode::Space, Vec3::Y),
            (KeyCode::KeyQ, -Vec3::Y),
        ] {
            // Keys typed into a TextBox don't fly the camera.
            if self.keys.contains(&key) && !typing {
                dir += v;
            }
        }
        let boost = if self.keys.contains(&KeyCode::ShiftLeft) {
            4.0
        } else {
            1.0
        };
        self.camera.camera.position += dir.normalize_or_zero() * self.camera.speed * boost * dt;
        if let Some(cam) = self.game.view().current_camera() {
            let c = &self.camera.camera;
            self.game.view().set_property(
                cam,
                "CFrame",
                Value::CFrame(CFrame {
                    position: c.position,
                    rotation: c.rotation,
                }),
            );
        }
    }
}
