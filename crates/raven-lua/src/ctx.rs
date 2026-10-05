//! Per-VM context and the calling convention for engine functions.
//!
//! mlua turns errors from Rust callbacks into userdata, and can only yield
//! from Rust through async functions. Roblox scripts expect error values to
//! be plain strings, and engine functions like `wait` to yield the calling
//! coroutine. So every engine function is a raw Rust function wrapped in a
//! small Luau shim. The Rust side returns its results, or a private marker
//! followed by an error message (the shim calls `error(msg, 0)`), a
//! yield marker (the shim calls `coroutine.yield()`; the Rust side has
//! already registered the current thread with whatever resumes it), or a
//! call marker followed by a function and its arguments (the shim calls it
//! from Lua, so it may yield; used by `require` and `Invoke`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Value as LuaValue};

use crate::world::World;

/// Shared state reachable from every callback.
#[derive(Debug)]
pub(crate) struct Ctx {
    pub(crate) world: Rc<RefCell<World>>,
    pub(crate) yield_marker: Table,
    pub(crate) error_marker: Table,
    pub(crate) call_marker: Table,
    /// Wraps a raw function in the shim.
    pub(crate) wrap: Function,
    /// Instance userdata by ID (weak values): one userdata per instance.
    pub(crate) instances: Table,
    /// Method functions by `(owning class, name)`, so `a.Destroy == b.Destroy`.
    pub(crate) methods: RefCell<HashMap<(String, String), Function>>,
    /// EnumItem userdata by `(enum, value)`.
    pub(crate) enum_items: Table,
}

pub(crate) fn ctx(lua: &Lua) -> Rc<Ctx> {
    lua.app_data_ref::<Rc<Ctx>>()
        .expect("raven context installed")
        .clone()
}

/// Run `f` with the world borrowed mutably. Never call into Lua inside `f`.
pub(crate) fn with_world<R>(lua: &Lua, f: impl FnOnce(&mut World) -> R) -> R {
    let ctx = ctx(lua);
    let mut world = ctx.world.borrow_mut();
    f(&mut world)
}

impl Ctx {
    pub(crate) fn install(lua: &Lua, world: Rc<RefCell<World>>) -> mlua::Result<Rc<Self>> {
        let yield_marker = lua.create_table()?;
        let error_marker = lua.create_table()?;
        let call_marker = lua.create_table()?;
        let co: Table = lua.globals().get("coroutine")?;
        let wrap: Function = lua
            .load(
                r#"
                local YIELD, ERR, CALL, yield, rawequal, error = ...
                local function call(f, ...)
                    return f(...)
                end
                local function finish(first, ...)
                    if rawequal(first, YIELD) then
                        return yield()
                    elseif rawequal(first, ERR) then
                        error((...), 0)
                    elseif rawequal(first, CALL) then
                        return call(...)
                    end
                    return first, ...
                end
                return function(f)
                    return function(...)
                        return finish(f(...))
                    end
                end
                "#,
            )
            .set_name("=[raven]")
            .call((
                yield_marker.clone(),
                error_marker.clone(),
                call_marker.clone(),
                co.get::<Function>("yield")?,
                lua.globals().get::<Function>("rawequal")?,
                lua.globals().get::<Function>("error")?,
            ))?;
        let instances = weak_table(lua, "v")?;
        let enum_items = lua.create_table()?;
        let ctx = Rc::new(Self {
            world,
            yield_marker,
            error_marker,
            call_marker,
            wrap,
            instances,
            methods: RefCell::default(),
            enum_items,
        });
        lua.set_app_data(ctx.clone());
        Ok(ctx)
    }
}

pub(crate) fn weak_table(lua: &Lua, mode: &str) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set("__mode", mode)?;
    t.set_metatable(Some(mt))?;
    Ok(t)
}

/// An error raised to scripts as a plain string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError(pub String);

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<mlua::Error> for ScriptError {
    fn from(e: mlua::Error) -> Self {
        Self(lua_error_message(&e))
    }
}

impl From<raven_dom::DomError> for ScriptError {
    fn from(e: raven_dom::DomError) -> Self {
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
pub fn lua_error_message(e: &mlua::Error) -> String {
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
        mlua::Error::CallbackError { cause, .. } => lua_error_message(cause),
        mlua::Error::WithContext { cause, .. } => lua_error_message(cause),
        mlua::Error::FromLuaConversionError { from, to, .. } => {
            format!("Unable to cast {from} to {to}")
        }
        mlua::Error::BadArgument { pos, cause, .. } => {
            format!("invalid argument #{pos} ({})", lua_error_message(cause))
        }
        other => other.to_string(),
    }
}

pub(crate) type ApiResult<T = MultiValue> = Result<T, ScriptError>;

/// What a raw engine function produced.
#[derive(Debug)]
pub(crate) enum Outcome {
    Return(MultiValue),
    /// The current thread was registered to be resumed later; yield it.
    Yield,
    /// Call this function with these arguments from Lua and return its results.
    Call(Function, MultiValue),
}

impl Outcome {
    pub(crate) fn ret(lua: &Lua, v: impl IntoLuaMulti) -> ApiResult<Self> {
        Ok(Self::Return(v.into_lua_multi(lua)?))
    }
}

/// Wrap a raw function in the shim. `f` gets the arguments as passed.
pub(crate) fn api_fn<F>(lua: &Lua, f: F) -> mlua::Result<Function>
where
    F: Fn(&Lua, MultiValue) -> ApiResult<Outcome> + 'static,
{
    let raw = lua.create_function(move |lua, args: MultiValue| {
        let c = ctx(lua);
        Ok(match f(lua, args) {
            Ok(Outcome::Return(v)) => v,
            Ok(Outcome::Yield) => MultiValue::from_iter([LuaValue::Table(c.yield_marker.clone())]),
            Ok(Outcome::Call(f, args)) => {
                let mut out = MultiValue::from_iter([
                    LuaValue::Table(c.call_marker.clone()),
                    LuaValue::Function(f),
                ]);
                out.extend(args);
                out
            }
            Err(e) => MultiValue::from_iter([
                LuaValue::Table(c.error_marker.clone()),
                LuaValue::String(lua.create_string(&e.0)?),
            ]),
        })
    })?;
    ctx(lua).wrap.call(raw)
}

/// Shorthand for functions that never yield.
pub(crate) fn api_ret<F, R>(lua: &Lua, f: F) -> mlua::Result<Function>
where
    F: Fn(&Lua, MultiValue) -> ApiResult<R> + 'static,
    R: IntoLuaMulti,
{
    api_fn(lua, move |lua, args| Outcome::ret(lua, f(lua, args)?))
}
