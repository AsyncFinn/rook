//! The free-flying camera.

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
}
