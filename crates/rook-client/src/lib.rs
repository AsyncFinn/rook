//! A client with no operating system of its own, so every app shares one
//! frame loop and the app supplies only the host
//! (docs/design/architecture.md#crate-layout).

mod bench;
mod game;
mod record;
mod render_thread;
mod screenshot;
mod start;
mod stats;
#[cfg(feature = "window")]
pub mod window;

pub use bench::{Bench, bench};
pub use game::{Game, Loading, Setup, place_name};
pub use record::record;
pub use screenshot::screenshot;
pub use start::{Start, prepare};

/// Put the GPU in the log and in any crash report.
fn note_gpu(renderer: &rook_render::Renderer) {
    rook_engine::logging::set_context("GPU", renderer.gpu().to_owned());
    tracing::debug!(target: "render", "GPU: {}", renderer.gpu());
}
