//! What the engine asks of a [`crate::Runtime`], by topic: each part is an
//! `impl Runtime` block of the calls one area of the engine makes.

pub(crate) mod characters;
pub(crate) mod content;
pub(crate) mod frame;
pub(crate) mod input;
pub(crate) mod prompts;
pub(crate) mod remotes;
pub(crate) mod scripts;
pub(crate) mod tools;
