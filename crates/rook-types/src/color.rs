/// Roblox `Color3`, with components nominally in `0..=1`.
///
/// Roblox does not clamp `Color3.new` inputs, so neither does Rook.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color3 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Color3 {
    pub const fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    /// `Color3.fromRGB`.
    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        )
    }

    /// Convert to 8-bit channels, rounding to nearest and clamping.
    pub fn to_rgb(self) -> (u8, u8, u8) {
        // Truncation is intended: the value is clamped to 0..=255 first.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        (ch(self.r), ch(self.g), ch(self.b))
    }

    /// `Color3.fromHSV`.
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.rem_euclid(1.0) * 6.0;
        let i = h.floor();
        let f = h - i;
        let p = v * (1.0 - s);
        let q = v * (1.0 - s * f);
        let t = v * (1.0 - s * (1.0 - f));
        match i as i32 {
            0 => Self::new(v, t, p),
            1 => Self::new(q, v, p),
            2 => Self::new(p, v, t),
            3 => Self::new(p, q, v),
            4 => Self::new(t, p, v),
            _ => Self::new(v, p, q),
        }
    }

    /// `Color3:ToHSV()`.
    pub fn to_hsv(self) -> (f32, f32, f32) {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        let delta = max - min;
        let s = if max == 0.0 { 0.0 } else { delta / max };
        let h = if delta == 0.0 {
            0.0
        } else if max == self.r {
            ((self.g - self.b) / delta).rem_euclid(6.0) / 6.0
        } else if max == self.g {
            ((self.b - self.r) / delta + 2.0) / 6.0
        } else {
            ((self.r - self.g) / delta + 4.0) / 6.0
        };
        (h, s, max)
    }

    pub fn lerp(self, goal: Self, alpha: f32) -> Self {
        Self::new(
            self.r + (goal.r - self.r) * alpha,
            self.g + (goal.g - self.g) * alpha,
            self.b + (goal.b - self.b) * alpha,
        )
    }
}

impl From<rbx_types::Color3> for Color3 {
    fn from(c: rbx_types::Color3) -> Self {
        Self::new(c.r, c.g, c.b)
    }
}

impl From<Color3> for rbx_types::Color3 {
    fn from(c: Color3) -> Self {
        Self::new(c.r, c.g, c.b)
    }
}

impl From<rbx_types::Color3uint8> for Color3 {
    fn from(c: rbx_types::Color3uint8) -> Self {
        Self::from_rgb(c.r, c.g, c.b)
    }
}

/// Rounded and clamped to 0..=255 ([`Color3::to_rgb`]).
impl From<Color3> for rbx_types::Color3uint8 {
    fn from(c: Color3) -> Self {
        let (r, g, b) = c.to_rgb();
        Self::new(r, g, b)
    }
}

impl From<rbx_types::BrickColor> for Color3 {
    fn from(c: rbx_types::BrickColor) -> Self {
        c.to_color3uint8().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_round_trip() {
        for v in [0u8, 1, 127, 128, 254, 255] {
            assert_eq!(Color3::from_rgb(v, v, v).to_rgb(), (v, v, v));
        }
    }

    #[test]
    fn hsv_round_trip() {
        let c = Color3::from_rgb(200, 40, 90);
        let (h, s, v) = c.to_hsv();
        assert_eq!(Color3::from_hsv(h, s, v).to_rgb(), c.to_rgb());
    }

    #[test]
    fn brick_color_medium_stone_grey() {
        let c: Color3 = rbx_types::BrickColor::from_number(194).unwrap().into();
        assert_eq!(c.to_rgb(), (163, 162, 165));
    }
}
