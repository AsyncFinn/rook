//! Per-frame work counters: what each per-frame system touched, what
//! changed, and how big its collection is, so "a small change, a
//! whole-collection cost" shows as a number rather than a timing.
//!
//! Modeled on Unreal's stat counters (`DECLARE_DWORD_COUNTER_STAT`,
//! `INC_DWORD_STAT_BY`, `SET_DWORD_STAT`): each thread adds into its own
//! block, and the frame's reader sums every block and resets them, as
//! Unreal's stats thread merges each thread's packets. Built only with the
//! `counters` feature (the apps' default, left out of `cargo dist`, as
//! Unreal leaves stats out of Shipping): without it [`ENABLED`] is false
//! and the [`count!`](crate::count) and [`gauge!`](crate::gauge) macros
//! compile to nothing, their arguments unevaluated.

use std::sync::atomic::{AtomicU64, Ordering};

pub const ENABLED: bool = cfg!(feature = "counters");

/// What a counter means in its frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Work done: summed over the frame.
    Touched,
    /// What changed, which the work answers: summed over the frame.
    Changed,
    /// How big a collection is: the largest set in the frame.
    Size,
    /// Neither, shown for itself: summed over the frame.
    Info,
}

macro_rules! counters {
    ($($(#[$doc:meta])* $name:ident: $kind:ident, $label:literal;)*) => {
        /// Every counter, in the order the bench prints them.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(usize)]
        pub enum Counter {
            $($(#[$doc])* $name,)*
        }

        impl Counter {
            pub const ALL: &[Counter] = &[$(Counter::$name,)*];

            pub const fn label(self) -> &'static str {
                match self {
                    $(Counter::$name => $label,)*
                }
            }

            pub const fn kind(self) -> Kind {
                match self {
                    $(Counter::$name => Kind::$kind,)*
                }
            }
        }
    };
}

counters! {
    /// Scene entries whose look was built again (`rook-scene`).
    ViewEntriesRewritten: Touched, "view: entries rewritten";
    /// Scene entries placed at a new CFrame, their look kept.
    ViewEntriesMoved: Touched, "view: entries moved";
    /// Parts placed, changed or freed, as render changes name them.
    ViewPartsChanged: Changed, "view: parts changed";
    /// Parts that only moved, as render changes name them.
    ViewPartsMoved: Changed, "view: parts moved";
    ViewEntries: Size, "view: scene entries";

    /// Instance slots written to the GPU (`rook-render`): whole chunks
    /// and scattered slots.
    RenderSlotsUploaded: Touched, "render: instance slots uploaded";
    RenderBytesUploaded: Info, "render: instance bytes uploaded";
    /// Scene entries the renderer was handed as changed or moved.
    RenderEntriesChanged: Changed, "render: entries changed";
    /// Draw calls the world GUI's canvases took.
    RenderGuiCalls: Info, "render: world gui draw calls";
    /// World canvases drawn: placed, near enough and in view.
    RenderGuiCanvases: Info, "render: world gui canvases drawn";
    RenderSlots: Size, "render: instance slots";

    /// Instances a tree walk visited (`Dom::descendants`).
    DomWalked: Touched, "dom: instances walked";
    /// Journal entries written this frame, every DataModel's.
    JournalWritten: Changed, "journal: entries written";
    DomInstances: Size, "dom: instances";

    /// Journal entries each reader read.
    JournalReadRender: Info, "journal read: render changes";
    JournalReadPhysics: Info, "journal read: physics";
    JournalReadGui: Info, "journal read: gui";
    JournalReadSignals: Info, "journal read: signals";
    JournalReadScripts: Info, "journal read: scripts";
    JournalReadReplication: Info, "journal read: replication";
    JournalReadAudio: Info, "journal read: audio graph";
    JournalReadStyles: Info, "journal read: styles";
    /// The bridges' shared watcher (sounds, unions, explosions...).
    JournalReadBridges: Info, "journal read: bridges";

    /// Light-voxel tiles filled again.
    LightTilesRefilled: Touched, "lights: tiles refilled";
    /// Blockers and lights that changed, which the fill answers, and the
    /// tiles the volume moved onto.
    LightSourcesChanged: Changed, "lights: blockers, lights and tiles moved onto";
    LightTiles: Size, "lights: tiles";

    /// GUI objects whose layout was worked out again.
    GuiNodesLaidOut: Touched, "gui: nodes laid out";
    /// GUI objects whose properties or place in the tree changed.
    GuiObjectsChanged: Changed, "gui: objects changed";
    GuiNodes: Size, "gui: nodes";
    /// World canvases (SurfaceGuis, BillboardGuis) placed again: only
    /// those whose adornee moved.
    GuiCanvasesPlaced: Touched, "gui: world canvases placed";
    GuiCanvases: Size, "gui: world canvases";

    /// Physics bodies written or read back.
    PhysicsBodiesTouched: Touched, "physics: bodies touched";
    /// Colliders made, rebuilt or moved.
    PhysicsCollidersTouched: Touched, "physics: colliders touched";
    /// Parts whose physical state changed in the journal.
    PhysicsPartsChanged: Changed, "physics: parts changed";
    PhysicsParts: Size, "physics: parts";
    /// Moves of assemblies another machine simulates, played back on
    /// this one, into the physics world: each step's for those near
    /// something simulated here, one after the frame for the rest.
    PhysicsPlayedMoves: Info, "physics: played assembly moves";

    /// Sounds whose playback state was worked out again.
    SoundsUpdated: Touched, "audio: sounds updated";
    SoundsChanged: Changed, "audio: sounds changed";
    Sounds: Size, "audio: sounds";

    /// Particles moved on (`rook-render`): only emitters in view step.
    ParticlesStepped: Touched, "particles: stepped";
    /// Particles drawn, at most Studio's 16,000 a frame.
    ParticlesDrawn: Info, "particles: drawn";
    Particles: Size, "particles: alive";

    /// Draws and runs recorded, per pass.
    ShadowDraws: Info, "draws: shadow pass draws";
    ShadowRuns: Info, "draws: shadow pass runs";
    MainDraws: Info, "draws: main pass draws";
    MainRuns: Info, "draws: main pass runs";
    /// Places the main pass breaks off its runs to draw world canvases
    /// sorted among the transparent parts.
    MainCuts: Info, "draws: main pass canvas cuts";
}

/// The (touched, changed, collection) triples the bench checks for a
/// frame that touched much of a collection while little changed.
pub const CHECKS: &[(Counter, Counter, Counter)] = &[
    (
        Counter::ViewEntriesRewritten,
        Counter::ViewPartsChanged,
        Counter::ViewEntries,
    ),
    (
        Counter::ViewEntriesMoved,
        Counter::ViewPartsMoved,
        Counter::ViewEntries,
    ),
    (
        Counter::RenderSlotsUploaded,
        Counter::RenderEntriesChanged,
        Counter::RenderSlots,
    ),
    (
        Counter::DomWalked,
        Counter::JournalWritten,
        Counter::DomInstances,
    ),
    (
        Counter::LightTilesRefilled,
        Counter::LightSourcesChanged,
        Counter::LightTiles,
    ),
    (
        Counter::GuiNodesLaidOut,
        Counter::GuiObjectsChanged,
        Counter::GuiNodes,
    ),
    (
        Counter::PhysicsBodiesTouched,
        Counter::PhysicsPartsChanged,
        Counter::PhysicsParts,
    ),
    (
        Counter::PhysicsCollidersTouched,
        Counter::PhysicsPartsChanged,
        Counter::PhysicsParts,
    ),
    (
        Counter::SoundsUpdated,
        Counter::SoundsChanged,
        Counter::Sounds,
    ),
];

const COUNT: usize = Counter::ALL.len();

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counts(pub [u64; COUNT]);

impl Default for Counts {
    fn default() -> Self {
        Self([0; COUNT])
    }
}

impl std::ops::Index<Counter> for Counts {
    type Output = u64;

    fn index(&self, c: Counter) -> &u64 {
        &self.0[c as usize]
    }
}

type Block = [AtomicU64; COUNT];

fn new_block() -> &'static Block {
    // A thread's block lives as long as the program: the frame's reader
    // sums it after the thread is gone too. Threads are few and long-lived
    // (the engine's, the server's, the worker pool's).
    let block: &'static Block = Box::leak(Box::new(std::array::from_fn(|_| AtomicU64::new(0))));
    BLOCKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(block);
    block
}

static BLOCKS: std::sync::Mutex<Vec<&'static Block>> = std::sync::Mutex::new(Vec::new());

thread_local! {
    static MINE: &'static Block = new_block();
}

/// Add `n` to `c` (use [`count!`](crate::count), which leaves this out of
/// builds without counters).
#[inline]
pub fn add(c: Counter, n: u64) {
    MINE.with(|b| b[c as usize].fetch_add(n, Ordering::Relaxed));
}

/// Raise size `c` to `n` for this frame (use [`gauge!`](crate::gauge)).
#[inline]
pub fn raise(c: Counter, n: u64) {
    MINE.with(|b| b[c as usize].fetch_max(n, Ordering::Relaxed));
}

fn drain(block: &Block, into: &mut Counts) {
    for (i, slot) in block.iter().enumerate() {
        let v = slot.swap(0, Ordering::Relaxed);
        if Counter::ALL[i].kind() == Kind::Size {
            into.0[i] = into.0[i].max(v);
        } else {
            into.0[i] += v;
        }
    }
}

/// The frame's counts from every thread, which start again from zero.
pub fn take() -> Counts {
    let mut out = Counts::default();
    for block in BLOCKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
    {
        drain(block, &mut out);
    }
    out
}

/// This thread's counts alone, which start again from zero: for tests,
/// which run side by side on threads of their own.
pub fn take_local() -> Counts {
    let mut out = Counts::default();
    MINE.with(|b| drain(b, &mut out));
    out
}

/// Add to a work counter: `count!(DomWalked, n)`. Nothing without the
/// `counters` feature, `n` not even evaluated.
#[macro_export]
macro_rules! count {
    ($counter:ident, $n:expr) => {
        if $crate::counters::ENABLED {
            $crate::counters::add($crate::counters::Counter::$counter, ($n) as u64);
        }
    };
}

/// Set a collection's size for this frame (the largest set wins):
/// `gauge!(ViewEntries, len)`.
#[macro_export]
macro_rules! gauge {
    ($counter:ident, $n:expr) => {
        if $crate::counters::ENABLED {
            $crate::counters::raise($crate::counters::Counter::$counter, ($n) as u64);
        }
    };
}

#[cfg(all(test, feature = "counters"))]
mod tests {
    use super::*;

    #[test]
    fn counts_sum_and_sizes_keep_the_largest() {
        take_local();
        crate::count!(DomWalked, 3);
        crate::count!(DomWalked, 4);
        crate::gauge!(DomInstances, 10);
        crate::gauge!(DomInstances, 7);
        let c = take_local();
        assert_eq!(c[Counter::DomWalked], 7);
        assert_eq!(c[Counter::DomInstances], 10);
        assert_eq!(take_local()[Counter::DomWalked], 0);
    }

    #[test]
    fn every_check_pairs_the_right_kinds() {
        for &(touched, changed, size) in CHECKS {
            assert_eq!(touched.kind(), Kind::Touched, "{touched:?}");
            assert_eq!(changed.kind(), Kind::Changed, "{changed:?}");
            assert_eq!(size.kind(), Kind::Size, "{size:?}");
        }
    }
}
