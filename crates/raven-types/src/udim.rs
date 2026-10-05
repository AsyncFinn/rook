use std::ops::{Add, Sub};

/// Roblox `UDim`: a scale fraction plus a pixel offset.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UDim {
    pub scale: f32,
    pub offset: i32,
}

/// Roblox `UDim2`: a `UDim` per axis.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UDim2 {
    pub x: UDim,
    pub y: UDim,
}

impl UDim {
    pub const fn new(scale: f32, offset: i32) -> Self {
        Self { scale, offset }
    }

    /// Resolve against a parent length in pixels.
    pub fn resolve(self, parent: f32) -> f32 {
        self.scale * parent + self.offset as f32
    }
}

impl UDim2 {
    pub const fn new(x_scale: f32, x_offset: i32, y_scale: f32, y_offset: i32) -> Self {
        Self {
            x: UDim::new(x_scale, x_offset),
            y: UDim::new(y_scale, y_offset),
        }
    }

    /// `UDim2.fromScale`.
    pub const fn from_scale(x: f32, y: f32) -> Self {
        Self::new(x, 0, y, 0)
    }

    /// `UDim2.fromOffset`.
    pub const fn from_offset(x: i32, y: i32) -> Self {
        Self::new(0.0, x, 0.0, y)
    }

    /// `UDim2:Lerp`. Roblox rounds the interpolated offsets; the exact
    /// rounding rule is to be confirmed against Studio.
    pub fn lerp(self, goal: Self, alpha: f32) -> Self {
        let lerp_udim = |a: UDim, b: UDim| {
            UDim::new(
                a.scale + (b.scale - a.scale) * alpha,
                (a.offset as f32 + (b.offset - a.offset) as f32 * alpha).round() as i32,
            )
        };
        Self {
            x: lerp_udim(self.x, goal.x),
            y: lerp_udim(self.y, goal.y),
        }
    }
}

impl Add for UDim {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.scale + rhs.scale, self.offset.wrapping_add(rhs.offset))
    }
}

impl Sub for UDim {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.scale - rhs.scale, self.offset.wrapping_sub(rhs.offset))
    }
}

impl Add for UDim2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for UDim2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl From<rbx_types::UDim> for UDim {
    fn from(u: rbx_types::UDim) -> Self {
        Self::new(u.scale, u.offset)
    }
}

impl From<UDim> for rbx_types::UDim {
    fn from(u: UDim) -> Self {
        Self::new(u.scale, u.offset)
    }
}

impl From<rbx_types::UDim2> for UDim2 {
    fn from(u: rbx_types::UDim2) -> Self {
        Self {
            x: u.x.into(),
            y: u.y.into(),
        }
    }
}

impl From<UDim2> for rbx_types::UDim2 {
    fn from(u: UDim2) -> Self {
        Self::new(u.x.into(), u.y.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_against_parent() {
        let u = UDim2::new(0.5, 10, 0.25, -4);
        assert_eq!(u.x.resolve(200.0), 110.0);
        assert_eq!(u.y.resolve(100.0), 21.0);
    }

    #[test]
    fn arithmetic() {
        assert_eq!(
            UDim2::new(0.5, 1, 0.5, 2) + UDim2::new(0.25, 3, 0.0, -2),
            UDim2::new(0.75, 4, 0.5, 0)
        );
    }
}
