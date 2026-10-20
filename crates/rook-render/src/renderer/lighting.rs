//! Lighting on the GPU besides the sun's shading in the shaders: local
//! lights ([`local`]), the sun's shadow map ([`shadow`]) and the sky's
//! reflection cube ([`sky`]).

pub(super) mod local;
pub(crate) mod shadow;
pub(crate) mod sky;

use super::*;
