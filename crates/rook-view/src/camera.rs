//! The camera a place opens with.

use glam::{Mat3, Vec3};
use rook_engine::DataModel;
use rook_render::Camera;
use rook_types::CFrame;

/// `Workspace.CurrentCamera` as the place was saved with it. Plain values,
/// so a server thread can hand it to the window's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SavedCamera {
    pub cframe: CFrame,
    /// Vertical, in degrees.
    pub fov_y: f32,
}

impl SavedCamera {
    pub fn read(dm: &DataModel) -> Option<Self> {
        let world = dm.runtime().world();
        let dom = &world.dom;
        let camera = dm.current_camera()?;
        Some(Self {
            cframe: dom.cframe_property(camera, "CFrame")?,
            fov_y: dom.float_property(camera, "FieldOfView").unwrap_or(70.0),
        })
    }
}

/// The saved camera, or a view of the origin.
pub fn initial_camera(saved: Option<SavedCamera>) -> Camera {
    let Some(SavedCamera { cframe: cf, fov_y }) = saved else {
        return Camera::look_at(Vec3::new(0.0, 20.0, 40.0), Vec3::ZERO);
    };
    Camera {
        position: cf.position,
        rotation: Mat3::from_cols(
            cf.rotation.x_axis.normalize(),
            cf.rotation.y_axis.normalize(),
            cf.rotation.z_axis.normalize(),
        ),
        fov_y,
        focus: None,
    }
}
