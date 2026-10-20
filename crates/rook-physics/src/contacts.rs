//! What happens where parts meet: friction and elasticity mixed the way
//! Roblox mixes them ([`mixing`]), and Touched and TouchEnded
//! ([`touches`]).

pub mod groups;
pub mod materials;
mod mixing;
mod touches;

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::sync::Mutex;

use rapier3d::prelude::*;
use rook_dom::InstanceId;

use crate::contacts::materials::{Properties, mix};

pub(crate) use mixing::{Mixer, hooks_for, pack, pack_velocity};
pub use touches::Touch;
pub(crate) use touches::{Collector, Tracker};
