//! Errors as Roblox's output prints them, with the stack the error
//! carries or the thread's own.

use super::*;

/// Print an error the way Roblox's output does:
///
/// ```text
/// Workspace.Script:3: attempt to index nil with 'X'
/// Stack Begin
/// Script 'Workspace.Script', Line 3
/// Stack End
/// ```
pub(crate) fn report_error(lua: &Lua, thread: &Thread, e: &mlua::Error) {
    let message = lua_error_message(e);
    let mut text = message;
    let stack = mlua_traceback(e)
        .map(|tb| parse_traceback(&tb))
        .filter(|l| !l.is_empty())
        .or_else(|| stack_lines(lua, thread));
    if let Some(stack) = stack {
        text.push_str("\nStack Begin");
        for line in stack {
            text.push('\n');
            text.push_str(&line);
        }
        text.push_str("\nStack End");
    }
    let source = error_source(&text);
    with_world(lua, |w| {
        w.errors += 1;
        w.push_log_from(MessageType::Error, text, source);
    });
}

/// `Workspace.Script:3: message` → `Script:3`; otherwise the first stack line.
fn error_source(text: &str) -> Option<String> {
    let first = text.lines().next()?;
    if let Some((loc, _)) = first.split_once(": ")
        && let Some((chunk, line)) = loc.rsplit_once(':')
        && let Ok(line) = line.parse()
    {
        return Some(crate::runtime::world::short_source(chunk, line));
    }
    let frame = text.lines().find(|l| l.starts_with("Script '"))?;
    let chunk = frame.strip_prefix("Script '")?.split('\'').next()?;
    let line = frame
        .rsplit_once("Line ")?
        .1
        .split(' ')
        .next()?
        .parse()
        .ok()?;
    Some(crate::runtime::world::short_source(chunk, line))
}

/// The traceback mlua attached to an error, if any.
fn mlua_traceback(e: &mlua::Error) -> Option<String> {
    match e {
        mlua::Error::RuntimeError(s) => s
            .split_once(
                "
stack traceback:",
            )
            .map(|(_, tb)| tb.to_owned()),
        mlua::Error::CallbackError { traceback, cause } => {
            mlua_traceback(cause).or_else(|| (!traceback.is_empty()).then(|| traceback.clone()))
        }
        mlua::Error::WithContext { cause, .. } => mlua_traceback(cause),
        _ => None,
    }
}

/// Parse Lua-style traceback lines (`chunk:3: in function 'f'`) into
/// Roblox's `Script 'chunk', Line 3 - function f`. Engine frames are dropped.
fn parse_traceback(tb: &str) -> Vec<String> {
    tb.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (location, rest) = line.split_once(": in ")?;
            let (chunk, number) = location.rsplit_once(':')?;
            if chunk.starts_with('[') {
                return None;
            }
            let number: u32 = number.parse().ok()?;
            let suffix = rest
                .strip_prefix("function '")
                .and_then(|f| f.strip_suffix('\''))
                .map(|f| format!(" - function {f}"))
                .unwrap_or_default();
            Some(format!("Script '{chunk}', Line {number}{suffix}"))
        })
        .collect()
}

/// Turn Luau's `debug.traceback(thread)` into Roblox's stack lines.
fn stack_lines(lua: &Lua, thread: &Thread) -> Option<Vec<String>> {
    let traceback: Function = lua.named_registry_value("rook.traceback").ok()?;
    let tb: String = traceback.call(thread.clone()).ok()?;
    let lines: Vec<String> = tb
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            // "Workspace.Script:3 function foo" or "Workspace.Script:3"
            let (location, rest) = line.split_once(' ').unwrap_or((line, ""));
            let (chunk, number) = location.rsplit_once(':')?;
            if chunk.starts_with('[') {
                return None;
            }
            let number: u32 = number.parse().ok()?;
            let suffix = rest
                .strip_prefix("function ")
                .map(|f| format!(" - function {f}"))
                .unwrap_or_default();
            Some(format!("Script '{chunk}', Line {number}{suffix}"))
        })
        .collect();
    (!lines.is_empty()).then_some(lines)
}
