use glam::{Mat3, Vec3};
use rook_engine::SavedCamera;
use rook_render::Camera;

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
