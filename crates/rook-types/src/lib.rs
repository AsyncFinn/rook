//! Roblox datatypes for the Rook engine.
//!
//! The math types (`Vector2`, `Vector3`, `CFrame`, `Color3`, `UDim`, `UDim2`)
//! are Rook's own, backed by `glam`. Roblox stores these components as
//! 32-bit floats, so Rook does too. Structured types that are plain data
//! (sequences, fonts, physical properties, and so on) reuse `rbx_types`.
//!
//! [`Value`] is the dynamic property value. It mirrors `rbx_types::Variant`
//! one-to-one so conversion in either direction is lossless.

mod clock;
mod enums;
mod part;
mod value;

pub use clock::{clock_hours, time_of_day};
pub use enums::{MeshType, RollOffMode, RunContext, UserInputState, UserInputType};
pub use part::collision::{Hull, UnionCollision};
pub use part::material::{Material, MaterialSet, TilePattern};
pub use part::mesh::{MeshHost, MeshKind, MeshLook, mesh_part_scale};
pub use part::shape::PartShape;
pub use part::surface::{NormalId, SURFACE_TILE_STUDS, SurfacePattern, SurfaceType, Tiling};
pub use part::truss::{TrussLook, TrussShape, TrussStyle, truss_size};
pub use value::cframe::CFrame;
pub use value::color::{Color3, linear_to_srgb, srgb_to_linear};
pub use value::udim::{UDim, UDim2};
pub use value::vector::{Vector2, Vector3};
pub use value::{InstanceRef, Value};

/// Plain-data Roblox types reused from `rbx_types`.
pub use rbx_types::{
    Attributes, Axes, BinaryString, BrickColor, Color3uint8, ColorSequence, ColorSequenceKeypoint,
    Content, ContentId, CustomPhysicalProperties, Faces, Font, MaterialColors, NetAssetRef,
    NumberRange, NumberSequence, NumberSequenceKeypoint, PhysicalProperties, Ray, Rect, Region3,
    Region3int16, SecurityCapabilities, SharedString, Tags, TerrainMaterials, UniqueId,
    Vector2int16, Vector3int16,
};
