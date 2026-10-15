use std::ops::Mul;

use glam::{Mat3, Quat, Vec3};

use crate::Vector3;

/// Roblox `CFrame`: a position and a rotation matrix.
///
/// Roblox names the rotation components `R00..R22` in row-major order, while
/// `glam::Mat3` stores columns. `RightVector` is the first column
/// `(R00, R10, R20)`, `UpVector` the second, and `LookVector` the negated
/// third (Roblox looks down -Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CFrame {
    pub position: Vec3,
    pub rotation: Mat3,
}

impl Default for CFrame {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl CFrame {
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        rotation: Mat3::IDENTITY,
    };

    /// `CFrame.new(x, y, z)`.
    pub const fn from_position(position: Vec3) -> Self {
        Self {
            position,
            rotation: Mat3::IDENTITY,
        }
    }

    /// `CFrame.new(x, y, z, R00, R01, R02, R10, R11, R12, R20, R21, R22)`.
    pub fn from_components(c: [f32; 12]) -> Self {
        Self {
            position: Vec3::new(c[0], c[1], c[2]),
            rotation: Mat3::from_cols(
                Vec3::new(c[3], c[6], c[9]),
                Vec3::new(c[4], c[7], c[10]),
                Vec3::new(c[5], c[8], c[11]),
            ),
        }
    }

    /// `CFrame:GetComponents()`: position, then rotation rows.
    pub fn components(&self) -> [f32; 12] {
        let r = self.rotation.transpose(); // columns of r are rows of self.rotation
        [
            self.position.x,
            self.position.y,
            self.position.z,
            r.x_axis.x,
            r.x_axis.y,
            r.x_axis.z,
            r.y_axis.x,
            r.y_axis.y,
            r.y_axis.z,
            r.z_axis.x,
            r.z_axis.y,
            r.z_axis.z,
        ]
    }

    /// `CFrame.Angles` / `CFrame.fromEulerAnglesXYZ`. Per Roblox's docs the
    /// rotations apply in Z, Y, X order, so the matrix is `Rx * Ry * Rz`.
    pub fn angles(rx: f32, ry: f32, rz: f32) -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Mat3::from_rotation_x(rx)
                * Mat3::from_rotation_y(ry)
                * Mat3::from_rotation_z(rz),
        }
    }

    /// `CFrame:ToEulerAnglesXYZ()`, the inverse of [`CFrame::angles`].
    pub fn to_euler_angles_xyz(&self) -> (f32, f32, f32) {
        // With R = Rx*Ry*Rz: R02 = sin(ry), R12 = -sin(rx)cos(ry), R22 = cos(rx)cos(ry),
        // R01 = -cos(ry)sin(rz), R00 = cos(ry)cos(rz).
        let [_, _, _, r00, r01, r02, _, _, r12, _, _, r22] = self.components();
        let ry = r02.clamp(-1.0, 1.0).asin();
        let rx = (-r12).atan2(r22);
        let rz = (-r01).atan2(r00);
        (rx, ry, rz)
    }

    /// `CFrame.fromOrientation` / `CFrame.fromEulerAnglesYXZ`: the matrix is
    /// `Ry * Rx * Rz` (Confirmed: creator-docs BasePart.Orientation).
    pub fn from_orientation(rx: f32, ry: f32, rz: f32) -> Self {
        Self {
            position: Vec3::ZERO,
            rotation: Mat3::from_euler(glam::EulerRot::YXZ, ry, rx, rz),
        }
    }

    /// `CFrame:ToOrientation()`, the inverse of [`CFrame::from_orientation`],
    /// as `(rx, ry, rz)` in radians.
    pub fn to_orientation(&self) -> (f32, f32, f32) {
        let (ry, rx, rz) = self.rotation.to_euler(glam::EulerRot::YXZ);
        (rx, ry, rz)
    }

    /// `CFrame.lookAt(at, target, up)`.
    pub fn look_at(at: Vec3, target: Vec3, up: Vec3) -> Self {
        let look = (target - at).normalize();
        let right = look.cross(up).normalize();
        let up = right.cross(look);
        Self {
            position: at,
            rotation: Mat3::from_cols(right, up, -look),
        }
    }

    pub fn right_vector(&self) -> Vector3 {
        Vector3(self.rotation.x_axis)
    }

    pub fn up_vector(&self) -> Vector3 {
        Vector3(self.rotation.y_axis)
    }

    pub fn look_vector(&self) -> Vector3 {
        Vector3(-self.rotation.z_axis)
    }

    /// `CFrame:Inverse()`. Assumes an orthonormal rotation, as Roblox CFrames are.
    pub fn inverse(&self) -> Self {
        let rotation = self.rotation.transpose();
        Self {
            position: -(rotation * self.position),
            rotation,
        }
    }

    /// `CFrame:PointToWorldSpace`, equivalent to `cf * v`.
    pub fn point_to_world_space(&self, point: Vector3) -> Vector3 {
        Vector3(self.position + self.rotation * point.0)
    }

    /// `CFrame:PointToObjectSpace`.
    pub fn point_to_object_space(&self, point: Vector3) -> Vector3 {
        self.inverse().point_to_world_space(point)
    }

    /// `CFrame:VectorToWorldSpace` (rotation only).
    pub fn vector_to_world_space(&self, v: Vector3) -> Vector3 {
        Vector3(self.rotation * v.0)
    }

    /// `CFrame:VectorToObjectSpace` (rotation only).
    pub fn vector_to_object_space(&self, v: Vector3) -> Vector3 {
        Vector3(self.rotation.transpose() * v.0)
    }

    /// `CFrame:ToObjectSpace(other)` = `self:Inverse() * other`.
    pub fn to_object_space(&self, other: CFrame) -> CFrame {
        self.inverse() * other
    }

    /// `CFrame:ToWorldSpace(other)` = `self * other`.
    pub fn to_world_space(&self, other: CFrame) -> CFrame {
        *self * other
    }

    /// `CFrame:Lerp`. Interpolates position linearly and rotation spherically.
    /// Matches Roblox in intent; exact numeric agreement is to be verified
    /// against Studio recordings.
    pub fn lerp(&self, goal: CFrame, alpha: f32) -> CFrame {
        let a = Quat::from_mat3(&self.rotation);
        let b = Quat::from_mat3(&goal.rotation);
        CFrame {
            position: self.position.lerp(goal.position, alpha),
            rotation: Mat3::from_quat(a.slerp(b, alpha)),
        }
    }

    /// `CFrame:Orthonormalize()` via Gram-Schmidt on the right and up
    /// vectors. Roblox's exact algorithm is unverified. A rotation too
    /// degenerate to fix (a zero or parallel axis) is kept as it is.
    pub fn orthonormalize(&self) -> CFrame {
        let right = self.rotation.x_axis.normalize();
        let up = (self.rotation.y_axis - right * right.dot(self.rotation.y_axis)).normalize();
        let back = right.cross(up);
        if !back.is_finite() {
            return *self;
        }
        CFrame {
            position: self.position,
            rotation: Mat3::from_cols(right, up, back),
        }
    }

    /// Half the size of the world-axis-aligned box around a box of
    /// `size` placed at this CFrame.
    pub fn box_reach(&self, size: Vec3) -> Vec3 {
        let half = size * 0.5;
        let r = self.rotation;
        r.x_axis.abs() * half.x + r.y_axis.abs() * half.y + r.z_axis.abs() * half.z
    }

    pub fn fuzzy_eq(&self, other: &CFrame, epsilon: f32) -> bool {
        self.components()
            .iter()
            .zip(other.components())
            .all(|(a, b)| (a - b).abs() <= epsilon)
    }
}

impl Mul for CFrame {
    type Output = CFrame;

    fn mul(self, rhs: CFrame) -> CFrame {
        CFrame {
            position: self.position + self.rotation * rhs.position,
            rotation: self.rotation * rhs.rotation,
        }
    }
}

impl Mul<Vector3> for CFrame {
    type Output = Vector3;

    fn mul(self, rhs: Vector3) -> Vector3 {
        self.point_to_world_space(rhs)
    }
}

impl From<rbx_types::CFrame> for CFrame {
    fn from(cf: rbx_types::CFrame) -> Self {
        let p = cf.position;
        let m = cf.orientation; // m.x, m.y, m.z are rows
        Self::from_components([
            p.x, p.y, p.z, m.x.x, m.x.y, m.x.z, m.y.x, m.y.y, m.y.z, m.z.x, m.z.y, m.z.z,
        ])
    }
}

impl From<CFrame> for rbx_types::CFrame {
    fn from(cf: CFrame) -> Self {
        let c = cf.components();
        rbx_types::CFrame::new(
            rbx_types::Vector3::new(c[0], c[1], c[2]),
            rbx_types::Matrix3::new(
                rbx_types::Vector3::new(c[3], c[4], c[5]),
                rbx_types::Vector3::new(c[6], c[7], c[8]),
                rbx_types::Vector3::new(c[9], c[10], c[11]),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    const EPS: f32 = 1e-5;

    #[test]
    fn components_round_trip() {
        let c = [1.0, 2.0, 3.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        assert_eq!(CFrame::from_components(c).components(), c);
    }

    #[test]
    fn right_vector_is_first_column() {
        // Rows: (R00 R01 R02) = (0 -1 0), (R10 R11 R12) = (1 0 0), (R20 R21 R22) = (0 0 1)
        let cf =
            CFrame::from_components([0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(cf.right_vector(), Vector3::new(0.0, 1.0, 0.0));
        assert_eq!(cf.up_vector(), Vector3::new(-1.0, 0.0, 0.0));
        assert_eq!(cf.look_vector(), Vector3::new(0.0, 0.0, -1.0));
    }

    #[test]
    fn inverse_cancels() {
        let cf = CFrame::from_position(Vec3::new(5.0, -2.0, 9.0)) * CFrame::angles(0.3, 1.1, -0.7);
        assert!((cf * cf.inverse()).fuzzy_eq(&CFrame::IDENTITY, EPS));
        assert!((cf.inverse() * cf).fuzzy_eq(&CFrame::IDENTITY, EPS));
    }

    #[test]
    fn euler_round_trip() {
        let (rx, ry, rz) = (0.4, -0.9, 1.3);
        let (a, b, c) = CFrame::angles(rx, ry, rz).to_euler_angles_xyz();
        assert!((a - rx).abs() < EPS && (b - ry).abs() < EPS && (c - rz).abs() < EPS);
    }

    #[test]
    fn angles_y_rotates_look_toward_minus_x() {
        // Rotating 90 degrees about +Y turns the default look (-Z) to -X.
        let look = CFrame::angles(0.0, FRAC_PI_2, 0.0).look_vector();
        assert!(look.fuzzy_eq(Vector3::new(-1.0, 0.0, 0.0), EPS));
    }

    #[test]
    fn look_at_points_at_target() {
        let cf = CFrame::look_at(Vec3::ZERO, Vec3::new(0.0, 0.0, -10.0), Vec3::Y);
        assert!(cf.fuzzy_eq(&CFrame::IDENTITY, EPS));
        let cf = CFrame::look_at(Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 2.0, 3.0), Vec3::Y);
        assert!(cf.look_vector().fuzzy_eq(Vector3::X_AXIS, EPS));
    }

    #[test]
    fn point_space_conversions() {
        let cf =
            CFrame::from_position(Vec3::new(10.0, 0.0, 0.0)) * CFrame::angles(0.0, FRAC_PI_2, 0.0);
        let world = cf * Vector3::new(0.0, 0.0, -1.0);
        assert!(world.fuzzy_eq(Vector3::new(9.0, 0.0, 0.0), EPS));
        assert!(
            cf.point_to_object_space(world)
                .fuzzy_eq(Vector3::new(0.0, 0.0, -1.0), EPS)
        );
    }

    #[test]
    fn rbx_types_round_trip_preserves_rows() {
        let cf = CFrame::from_position(Vec3::new(1.0, 2.0, 3.0)) * CFrame::angles(0.2, 0.5, 0.9);
        let rbx: rbx_types::CFrame = cf.into();
        let c = cf.components();
        assert_eq!(rbx.orientation.x.y, c[4]); // R01
        assert_eq!(rbx.orientation.y.x, c[6]); // R10
        assert_eq!(CFrame::from(rbx), cf);
    }
}
