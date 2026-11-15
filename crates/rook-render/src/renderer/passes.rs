//! The frame's passes: the main pass and the shadows and sky before it,
//! transparent parts and where world GUI cuts in among them, Highlights,
//! the tone pass and ViewportFrames' pictures.

pub(crate) mod cuts;
pub(crate) mod draw;
pub(crate) mod highlights;
pub(crate) mod tonemap;
pub(crate) mod transparent;
pub(crate) mod viewports;

use super::*;
