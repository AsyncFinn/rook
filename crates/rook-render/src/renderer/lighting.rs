//! Lighting on the GPU besides the sun's shading in the shaders: local
//! lights ([`local`]) and the sun's shadow map ([`shadow`]).

pub(super) mod local;
pub(crate) mod shadow;

use super::*;
