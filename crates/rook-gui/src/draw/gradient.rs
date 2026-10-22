//! UIGradient: an object's colors and opacity multiplied along a line
//! across it (docs/behavior/gui/modern.md#uigradient).

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{ColorSequence, GuiGradient, GuiRamp, NumberSequence};
use rustc_hash::FxHasher;

use crate::layout::{Placed, turn};

/// The gradient of `holder`'s first enabled UIGradient (`holder` a GUI
/// object or a UIStroke) over a box placed like `p`, `size` big as drawn.
/// Measured: `t` runs from 0 to 1 along `Rotation` (clockwise, 0 left to
/// right) through the box's center, over the length of that line inside
/// the box, held past its ends; `Offset` moves it by that share of the
/// box's size.
pub(super) fn of(dom: &Dom, holder: InstanceId, p: Placed, size: Vec2) -> Option<GuiGradient> {
    let g = (dom.children(holder).iter().copied()).find(|&c| {
        dom.class_name(c) == "UIGradient" && dom.bool_property(c, "Enabled") != Some(false)
    })?;
    let color = dom.color_sequence_property(g, "Color")?;
    let transparency = dom.number_sequence_property(g, "Transparency")?;
    let turned = dom
        .float_property(g, "Rotation")
        .unwrap_or(0.0)
        .to_radians();
    let (sin, cos) = turned.sin_cos();
    let span = |side: f32, share: f32| {
        if share.abs() < 1e-6 {
            f32::INFINITY
        } else {
            side / share.abs()
        }
    };
    let size = size.abs();
    let length = span(size.x, cos).min(span(size.y, sin)).max(1e-3);
    let offset = dom
        .vector2_property(g, "Offset")
        .map_or(Vec2::ZERO, |v| v.0);
    Some(GuiGradient {
        ramp: ramp(&color, &transparency),
        origin: p.center + turn(p.rotation, offset * size),
        step: turn(p.rotation, Vec2::new(cos, sin) / length),
    })
}

/// The ramp of a gradient's sequences. Measured: colors blend in a gamma
/// of 2 (halfway between black and white is √0.5), opacity in a straight
/// line.
fn ramp(color: &ColorSequence, transparency: &NumberSequence) -> Arc<GuiRamp> {
    let mut hash = FxHasher::default();
    for k in &color.keypoints {
        [k.time, k.color.r, k.color.g, k.color.b]
            .map(f32::to_bits)
            .hash(&mut hash);
    }
    for k in &transparency.keypoints {
        [k.time, k.value].map(f32::to_bits).hash(&mut hash);
    }
    let last = (GuiRamp::TEXELS - 1) as f32;
    let texels = std::array::from_fn(|i| {
        let t = i as f32 / last;
        let rgb = color_at(color, t);
        let opacity = 1.0 - number_at(transparency, t);
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [byte(rgb[0]), byte(rgb[1]), byte(rgb[2]), byte(opacity)]
    });
    Arc::new(GuiRamp {
        key: hash.finish(),
        texels,
    })
}

/// The two keypoints around `t` and how far from the first to the second.
fn around<K>(keys: &[K], time: impl Fn(&K) -> f32, t: f32) -> Option<(&K, &K, f32)> {
    let after = keys.iter().position(|k| time(k) >= t)?;
    let (a, b) = (&keys[after.saturating_sub(1)], &keys[after]);
    let gap = time(b) - time(a);
    let f = if gap > 0.0 { (t - time(a)) / gap } else { 1.0 };
    Some((a, b, f.clamp(0.0, 1.0)))
}

fn color_at(s: &ColorSequence, t: f32) -> [f32; 3] {
    let Some((a, b, f)) = around(&s.keypoints, |k| k.time, t) else {
        return s
            .keypoints
            .last()
            .map_or([1.0; 3], |k| [k.color.r, k.color.g, k.color.b]);
    };
    let mix = |x: f32, y: f32| (x * x + (y * y - x * x) * f).sqrt();
    [
        mix(a.color.r, b.color.r),
        mix(a.color.g, b.color.g),
        mix(a.color.b, b.color.b),
    ]
}

fn number_at(s: &NumberSequence, t: f32) -> f32 {
    let Some((a, b, f)) = around(&s.keypoints, |k| k.time, t) else {
        return s.keypoints.last().map_or(0.0, |k| k.value);
    };
    a.value + (b.value - a.value) * f
}
