//! What the mouse does to GUI: which objects are under it, a Modal button freeing a held mouse, and ScrollingFrames moved by the wheel or a dragged bar; and the focused TextBox's caret blinking.

pub(crate) mod caret;
pub(crate) mod hits;
pub(crate) mod modal;
pub(crate) mod scrolling;
