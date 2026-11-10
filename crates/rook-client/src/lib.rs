//! A Rook client without an operating system of its own: the game a
//! player sees (a play session whose server runs on its own thread, or the
//! server alone), the frame loop that steps it and draws it on a render
//! thread, and the runs that need no window (a screenshot, `--bench`, a
//! recording). An app supplies the host: [`window::run`] with an event
//! loop where there is a window, the platform (`rook_platform`) and its
//! own options (docs/design/architecture.md#crate-layout).

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
