use std::ops::{Add, Div, Mul, Neg, Sub};

/// Roblox `Vector2`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector2(pub glam::Vec2);

/// Roblox `Vector3`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector3(pub glam::Vec3);

impl Vector2 {
    pub const ZERO: Self = Self(glam::Vec2::ZERO);
    pub const ONE: Self = Self(glam::Vec2::ONE);

    pub const fn new(x: f32, y: f32) -> Self {
        Self(glam::Vec2::new(x, y))
    }

    pub fn x(self) -> f32 {
        self.0.x
    }

    pub fn y(self) -> f32 {
        self.0.y
    }

    /// `Vector2.Magnitude`.
    pub fn magnitude(self) -> f32 {
        self.0.length()
    }

    /// `Vector2.Unit`. Like Roblox, the zero vector produces NaN components.
    pub fn unit(self) -> Self {
        Self(self.0 / self.0.length())
    }

    pub fn dot(self, other: Self) -> f32 {
        self.0.dot(other.0)
    }

    /// `Vector2:Cross`, the scalar z component of the 3D cross product.
    pub fn cross(self, other: Self) -> f32 {
        self.0.perp_dot(other.0)
    }

    pub fn lerp(self, goal: Self, alpha: f32) -> Self {
        Self(self.0 + (goal.0 - self.0) * alpha)
    }
}

impl Vector3 {
    pub const ZERO: Self = Self(glam::Vec3::ZERO);
    pub const ONE: Self = Self(glam::Vec3::ONE);
    pub const X_AXIS: Self = Self(glam::Vec3::X);
    pub const Y_AXIS: Self = Self(glam::Vec3::Y);
    pub const Z_AXIS: Self = Self(glam::Vec3::Z);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self(glam::Vec3::new(x, y, z))
    }

    pub fn x(self) -> f32 {
        self.0.x
    }

    pub fn y(self) -> f32 {
        self.0.y
    }

    pub fn z(self) -> f32 {
        self.0.z
    }

    /// `Vector3.Magnitude`.
    pub fn magnitude(self) -> f32 {
        self.0.length()
    }

    /// `Vector3.Unit`. Like Roblox, the zero vector produces NaN components.
    pub fn unit(self) -> Self {
        Self(self.0 / self.0.length())
    }

    pub fn dot(self, other: Self) -> f32 {
        self.0.dot(other.0)
    }

    pub fn cross(self, other: Self) -> Self {
        Self(self.0.cross(other.0))
    }

    pub fn lerp(self, goal: Self, alpha: f32) -> Self {
        Self(self.0 + (goal.0 - self.0) * alpha)
    }

    pub fn min(self, other: Self) -> Self {
        Self(self.0.min(other.0))
    }

    pub fn max(self, other: Self) -> Self {
        Self(self.0.max(other.0))
    }

    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// `Vector3:FuzzyEq`. Roblox's exact tolerance rule is not yet verified;
    /// this compares each component within `epsilon`.
    pub fn fuzzy_eq(self, other: Self, epsilon: f32) -> bool {
        (self.0 - other.0).abs().max_element() <= epsilon
    }
}

macro_rules! impl_vector_ops {
    ($ty:ident) => {
        impl Add for $ty {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl Sub for $ty {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl Mul for $ty {
            type Output = Self;
            fn mul(self, rhs: Self) -> Self {
                Self(self.0 * rhs.0)
            }
        }

        impl Mul<f32> for $ty {
            type Output = Self;
            fn mul(self, rhs: f32) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl Div for $ty {
            type Output = Self;
            fn div(self, rhs: Self) -> Self {
                Self(self.0 / rhs.0)
            }
        }

        impl Div<f32> for $ty {
            type Output = Self;
            fn div(self, rhs: f32) -> Self {
                Self(self.0 / rhs)
            }
        }

        impl Neg for $ty {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }
    };
}

impl_vector_ops!(Vector2);
impl_vector_ops!(Vector3);

impl From<rbx_types::Vector2> for Vector2 {
    fn from(v: rbx_types::Vector2) -> Self {
        Self::new(v.x, v.y)
    }
}

impl From<Vector2> for rbx_types::Vector2 {
    fn from(v: Vector2) -> Self {
        Self::new(v.0.x, v.0.y)
    }
}

impl From<rbx_types::Vector3> for Vector3 {
    fn from(v: rbx_types::Vector3) -> Self {
        Self::new(v.x, v.y, v.z)
    }
}

impl From<Vector3> for rbx_types::Vector3 {
    fn from(v: Vector3) -> Self {
        Self::new(v.0.x, v.0.y, v.0.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic() {
        let a = Vector3::new(1.0, 2.0, 3.0);
        let b = Vector3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vector3::new(5.0, 7.0, 9.0));
        assert_eq!(b - a, Vector3::new(3.0, 3.0, 3.0));
        assert_eq!(a * 2.0, Vector3::new(2.0, 4.0, 6.0));
        assert_eq!(-a, Vector3::new(-1.0, -2.0, -3.0));
        assert_eq!(a.dot(b), 32.0);
        assert_eq!(Vector3::X_AXIS.cross(Vector3::Y_AXIS), Vector3::Z_AXIS);
    }

    #[test]
    fn unit_of_zero_is_nan() {
        assert!(Vector3::ZERO.unit().x().is_nan());
    }

    #[test]
    fn rbx_types_round_trip() {
        let v = Vector3::new(1.5, -2.25, 1e6);
        assert_eq!(Vector3::from(rbx_types::Vector3::from(v)), v);
    }
}
