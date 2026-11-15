//! Script errors: what a failed API call raises, and the message Luau
//! shows for one, worded as Roblox words it.

/// An error raised to scripts as a plain string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError(pub String);

impl From<mlua::Error> for ScriptError {
    fn from(e: mlua::Error) -> Self {
        Self(lua_error_message(&e))
    }
}

impl From<rook_dom::DomError> for ScriptError {
    fn from(e: rook_dom::DomError) -> Self {
        Self(e.to_string())
    }
}

impl From<String> for ScriptError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ScriptError {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

/// The message a script would see for an mlua error.
pub(crate) fn lua_error_message(e: &mlua::Error) -> String {
    match e {
        mlua::Error::RuntimeError(s) | mlua::Error::SyntaxError { message: s, .. } => {
            // mlua appends a traceback to errors from resumed threads.
            s.split(
                "
stack traceback:",
            )
            .next()
            .unwrap_or(s)
            .to_owned()
        }
        mlua::Error::CallbackError { cause, .. } | mlua::Error::WithContext { cause, .. } => {
            lua_error_message(cause)
        }
        mlua::Error::FromLuaConversionError { from, to, .. } => cast_message(from, to, 2, None),
        mlua::Error::BadArgument { to, pos, cause, .. } => match &**cause {
            mlua::Error::FromLuaConversionError {
                from, to: target, ..
            } => cast_message(from, target, *pos, to.as_deref()),
            other => format!("invalid argument #{pos} ({})", lua_error_message(other)),
        },
        other => other.to_string(),
    }
}

/// A value of Lua type `from` where a method wanted a `target`, as Studio
/// words it: Vector3 and CFrame arguments are checked by their own bindings,
/// other types' by the reflection layer's casts (docs/behavior/scripting/
/// datatype-errors.md). `method` is mlua's `LType.Name`: a userdata argument
/// doesn't say what it wanted, and those measured wanted their own type.
pub(super) fn cast_message(from: &str, target: &str, pos: usize, method: Option<&str>) -> String {
    let own = method
        .and_then(|m| m.split('.').next())
        .map(|t| t.strip_prefix('L').unwrap_or(t));
    let target = match target {
        "vector" => "Vector3",
        "userdata" => own.unwrap_or(target),
        "f32" => "float",
        "f64" => "double",
        other => other,
    };
    let from = if from == "integer" { "number" } else { from };
    if matches!(target, "Vector3" | "CFrame") {
        return format!("invalid argument #{pos} ({target} expected, got {from})");
    }
    let from = if from == "number" { "double" } else { from };
    format!("Unable to cast {from} to {target}")
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
