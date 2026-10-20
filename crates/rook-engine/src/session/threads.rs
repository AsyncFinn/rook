//! How many background threads Rook uses, decided once from the machine's
//! cores and handed to content decoding and physics as plain counts.
//!
//! Some threads must stay responsive whatever the machine: the engine
//! thread (which also records rendering and runs the window) and the
//! server thread. wgpu's driver threads and the audio callback are light.
//! The rest of the cores go to background work: decoding content and
//! physics' worker pool. A weak machine gets few background threads, so
//! loading doesn't starve the frame; a strong one gets many, so it loads
//! faster.
//!
//! The `threads` setting pretends the machine has `n` logical cores, to
//! test how Rook behaves on a smaller (or bigger) one.

use std::sync::Once;

/// Threads set aside before handing out the rest: the engine thread and
/// the server thread.
const RESERVED: usize = 2;

/// Decode threads at most. Content decoding is a burst while a place
/// loads; past this, more threads mostly contend for memory bandwidth.
/// Roblox clamps its own automatic thread count the same way
/// (FIntTaskSchedulerAutoThreadLimit 8 in Studio's local flags).
const MAX_DECODE: usize = 8;

/// The background threads for a machine with some number of logical
/// cores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub cores: usize,
}

impl Budget {
    /// This machine's budget: its logical cores, or the `threads` setting.
    pub fn of_this_machine() -> Self {
        let cores = crate::settings::threads()
            .unwrap_or_else(|| std::thread::available_parallelism().map_or(2, usize::from));
        Self::for_cores(cores)
    }

    pub fn for_cores(cores: usize) -> Self {
        Self {
            cores: cores.max(1),
        }
    }

    /// Threads decoding meshes, images and sounds: the cores left after
    /// the reserved threads, at least one and at most [`MAX_DECODE`].
    /// They run at below-normal priority, so on a small machine they take
    /// what the frame leaves.
    pub fn decode(self) -> usize {
        self.cores.saturating_sub(RESERVED).clamp(1, MAX_DECODE)
    }

    /// Physics' worker pool: half the cores. The server thread waits for
    /// the pool during a step, so its share overlaps the server's reserved
    /// thread; the other half is left to the client, the renderer and
    /// decoding.
    pub fn physics(self) -> usize {
        (self.cores / 2).max(1)
    }
}

/// Size content decoding and physics' pool from this machine's budget.
/// Runs once per process, before the first DataModel; tools that load
/// content without a DataModel call it themselves.
pub fn size_threads() {
    static SIZED: Once = Once::new();
    SIZED.call_once(|| {
        let budget = Budget::of_this_machine();
        rook_content::set_decode_threads(budget.decode());
        rook_physics::size_worker_pool(budget.physics());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_machines_get_one_decode_thread_and_big_ones_eight() {
        let decode = |cores| Budget::for_cores(cores).decode();
        let physics = |cores| Budget::for_cores(cores).physics();
        assert_eq!(
            [0, 1, 2, 3, 4, 6, 12, 32].map(decode),
            [1, 1, 1, 1, 2, 4, 8, 8]
        );
        assert_eq!([0, 1, 2, 3, 4, 12].map(physics), [1, 1, 1, 1, 2, 6]);
    }
}
