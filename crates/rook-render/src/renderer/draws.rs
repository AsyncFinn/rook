//! Which draws a frame makes: where each scene entry is drawn and what it
//! waits for, levels of detail, the runs draws are recorded in, and the
//! worlds they belong to.

pub(crate) mod detail;
pub(crate) mod placement;
pub(crate) mod runs;
pub(crate) mod waiting;
pub(crate) mod worlds;

use super::*;
