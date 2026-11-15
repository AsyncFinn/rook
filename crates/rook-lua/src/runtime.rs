//! What runs scripts: the `World` a DataModel's scripts share, the
//! `scheduler` and Luau `threads` it resumes, `signal`s and their
//! connections, the `ctx` that hands script calls the World, compiled
//! `bytecode`, and what `collect`ion and `removal` free.

pub(crate) mod bytecode;
pub(crate) mod collect;
pub(crate) mod ctx;
pub(crate) mod removal;
pub mod scheduler;
pub(crate) mod signal;
pub(crate) mod threads;
pub(crate) mod world;
