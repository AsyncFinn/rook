//! `print`, `warn` and the other output, with the script and line that
//! called them.

use super::*;

pub(super) fn log(lua: &Lua, kind: MessageType, args: &MultiValue) {
    let text = args
        .iter()
        .map(|v| display_value(lua, v))
        .collect::<Vec<_>>()
        .join(" ");
    let source = caller(lua);
    with_world(lua, |w| w.push_log_from(kind, text, source));
}

/// The script and line that called an engine function, skipping Rook's
/// own wrapper frames.
pub(super) fn caller(lua: &Lua) -> Option<String> {
    for level in 1..8 {
        let found = lua.inspect_stack(level, |d| {
            let src = d.source().short_src.map(std::borrow::Cow::into_owned);
            (src, d.current_line())
        })?;
        if let (Some(src), Some(line)) = found
            && !src.starts_with('[')
        {
            return Some(crate::world::short_source(&src, line));
        }
    }
    None
}
