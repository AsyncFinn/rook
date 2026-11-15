//! Lighting on the GPU: local lights in a voxel volume, the sun's shadow
//! map and the sky's cube and BRDF table.

pub(crate) mod local;
pub(crate) mod shadow;
pub(crate) mod sky;

use crate::*;
