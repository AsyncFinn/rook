//! Roblox datatypes for the Rook engine.
//!
//! The math types (`Vector2`, `Vector3`, `CFrame`, `Color3`, `UDim`, `UDim2`)
//! are Rook's own, backed by `glam`. Roblox stores these components as
//! 32-bit floats, so Rook does too. Structured types that are plain data
//! (sequences, fonts, physical properties, and so on) reuse `rbx_types`.
//!
//! [`Value`] is the dynamic property value. It mirrors `rbx_types::Variant`
//! one-to-one so conversion in either direction is lossless.

mod cframe;
mod color;
mod udim;
mod value;
mod vector;

pub use cframe::CFrame;
pub use color::Color3;
pub use udim::{UDim, UDim2};
pub use value::{InstanceRef, Value, ValueError};
pub use vector::{Vector2, Vector3};

/// Plain-data Roblox types reused from `rbx_types`.
pub use rbx_types::{
    Attributes, Axes, BinaryString, BrickColor, Color3uint8, ColorSequence, ColorSequenceKeypoint,
    Content, ContentId, Faces, Font, MaterialColors, NetAssetRef, NumberRange, NumberSequence,
    NumberSequenceKeypoint, PhysicalProperties, Ray, Rect, Region3, Region3int16,
    SecurityCapabilities, SharedString, Tags, UniqueId, Vector2int16, Vector3int16,
};
