//! Rook's build settings as constants: `webBase` and `webDomains` from
//! `rook-build.toml`, or a distributor's file over it, read by `build.rs`
//! when Rook is built (docs/design/data-and-logging.md#build-settings).
//! Nothing on the machine running Rook changes them; in builds with
//! developer overrides, [`crate::settings`]' `webBase` and `webDomains`
//! win over them.

include!(concat!(env!("OUT_DIR"), "/build_config.rs"));
