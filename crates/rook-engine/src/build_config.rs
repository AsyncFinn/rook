//! Rook's build settings as constants: `rook-build.toml`, or a
//! distributor's file over it, read by `build.rs` when Rook is built
//! (docs/design/data-and-logging.md#build-settings). Nothing on the
//! machine running Rook changes them; [`crate::settings`] lets developer
//! overrides win over them in builds that allow those.

include!(concat!(env!("OUT_DIR"), "/build_config.rs"));
