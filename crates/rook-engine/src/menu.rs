//! Rook's own in-game menu: the logo button at the top left and the menu
//! it (or Escape) opens, with the player's settings, Leave and Resume. It
//! is Luau (`menu/menu.luau`) shipped in the engine, which a client runs
//! as a CoreScript under CoreGui when it doesn't run Roblox's CoreScripts
//! (whose menu takes its place). The `menu` setting turns it off. See
//! docs/behavior/gui/menu.md.

use std::sync::{Arc, OnceLock};

use rook_lua::Compiled;

use crate::DataModel;

/// The menu's source.
pub const SOURCE: &str = include_str!("menu/menu.luau");

/// The menu compiled once a run, as the engine's own code.
fn bytecode() -> Option<Arc<[u8]>> {
    static CODE: OnceLock<Option<Arc<[u8]>>> = OnceLock::new();
    CODE.get_or_init(|| {
        match mlua::chunk::Compiler::new()
            .set_optimization_level(2)
            .compile(SOURCE)
        {
            Ok(code) => Some(Arc::from(code)),
            Err(e) => {
                tracing::error!("Rook's menu doesn't compile: {e}");
                None
            }
        }
    })
    .clone()
}

/// Start the menu on a client, before its LocalScripts start: a
/// CoreScript, `RookMenuScript`, under CoreGui, which puts the `RookMenu`
/// ScreenGui there. Problems are logged; the place runs on without it.
pub(crate) fn install(dm: &DataModel) {
    let Some(code) = bytecode() else {
        return;
    };
    let script = {
        let mut w = dm.runtime().world_mut();
        let w = &mut *w;
        let journaling = w.dom.journal().is_enabled();
        w.dom.journal_mut().set_enabled(false);
        let made = (|| {
            let core_gui = w.dom.get_service("CoreGui").map_err(|e| e.to_string())?;
            let script = w
                .dom
                .create_by_name("CoreScript")
                .map_err(|e| e.to_string())?;
            w.dom
                .set_name(script, "RookMenuScript".into())
                .map_err(|e| e.to_string())?;
            w.dom
                .set_parent(script, Some(core_gui))
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(script)
        })();
        w.dom.journal_mut().set_enabled(journaling);
        match made {
            Ok(script) => {
                w.bytecode.insert(script, Compiled { code, core: true });
                Some(script)
            }
            Err(e) => {
                w.push_log(rook_lua::MessageType::Error, format!("Rook's menu: {e}"));
                None
            }
        }
    };
    if let Some(script) = script {
        dm.runtime().run_script(script);
    }
}
