//! Per-VM context and the calling convention for engine functions.
//!
//! mlua turns errors from Rust callbacks into userdata, and can only yield
//! from Rust through async functions. Roblox scripts expect error values to
//! be plain strings, and engine functions like `wait` to yield the calling
//! coroutine. So every engine function is a raw Rust function wrapped in a
//! small Luau shim. The Rust side returns its single result directly, or a
//! private marker: an error marker followed by the message (the shim calls
//! `error(msg, 0)`), a yield marker (the shim calls `coroutine.yield()`;
//! the Rust side has already registered the current thread with whatever
//! resumes it), a call marker followed by a function whose arguments are
//! pending (the shim calls it from Lua, so it may yield; used by `require`
//! and `Invoke`), or a multi marker when there are several results pending.
//! Pending values are fetched with a second call; the shim takes two
//! results, never varargs, which cost more than most engine functions.

pub(crate) mod args;
pub(crate) mod convert;

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Value as LuaValue};

use rook_dom::InstanceId;
use rook_reflection::{ClassId, EnumId, PropertyId};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::api::instance::MemberTable;
use crate::world::World;

/// A plain stored property read: the property and its enum, if any.
pub(crate) type FastRead = (PropertyId, Option<EnumId>);

/// Shared state reachable from every callback.
#[derive(Debug)]
pub(crate) struct Ctx {
    pub(crate) world: Rc<RefCell<World>>,
    yield_marker: Table,
    error_marker: Table,
    call_marker: Table,
    multi_marker: Table,
    /// Returned for no results, which the shim turns back into none (one
    /// nil would change `select('#', ...)`).
    none_marker: Table,
    /// Results or call arguments the shim fetches after seeing a marker.
    pending: RefCell<MultiValue>,
    /// Wraps a raw function in the shim.
    pub(crate) wrap: Function,
    /// Instance userdata by ID (weak values): one userdata per instance.
    pub(crate) instances: Table,
    /// The reverse (weak keys): each instance userdata's light userdata key.
    pub(crate) instance_keys: Table,
    /// Method functions by `(owning class, name)`, so `a.Destroy == b.Destroy`.
    pub(crate) methods: RefCell<FxHashMap<(&'static str, &'static str), Function>>,
    /// Member tables by class, whether the class is known, and who looks.
    pub(crate) member_tables:
        RefCell<FxHashMap<(ClassId, bool, crate::api::instance::Identity), MemberTable>>,
    /// Each instance userdata's method cache (weak keys): a table shared by
    /// every instance of its class (and whether the class is known), which
    /// `__index` fills with the class's methods as scripts look them up.
    pub(crate) method_caches: Table,
    /// The shared method cache tables, by class.
    pub(crate) class_method_caches: RefCell<FxHashMap<(ClassId, bool), Table>>,
    /// Runtime-only property values by instance key, created when engine
    /// Luau code first sets one.
    pub(crate) runtime_props: RefCell<Option<Table>>,
    /// The instances that have runtime-only values, so reads of every
    /// other instance skip the table above.
    pub(crate) runtime_prop_ids: RefCell<FxHashSet<InstanceId>>,
    /// EnumItem userdata by `(enum, value)`.
    pub(crate) enum_items: Table,
    /// Small ids for member names that are plain stored properties on
    /// some class, which the `inst.Key` shim passes instead of the name.
    pub(crate) member_ids: RefCell<FxHashMap<&'static str, u32>>,
    /// For each class and member id read so far: the stored property it
    /// reads (and its enum), when reading it is a plain DOM read.
    pub(crate) fast_reads: RefCell<FxHashMap<(ClassId, u32), FastRead>>,
    /// Each instance userdata's `Name` as the `inst.Key` shim last read it
    /// (weak keys), dropped as the instance is renamed.
    pub(crate) names: Table,
}

pub(crate) fn ctx(lua: &Lua) -> Rc<Ctx> {
    lua.app_data_ref::<Rc<Ctx>>()
        .expect("rook context installed")
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
        let multi_marker = lua.create_table()?;
        let none_marker = lua.create_table()?;
        let markers = lua.create_table()?;
        for m in [
            &yield_marker,
            &error_marker,
            &call_marker,
            &multi_marker,
            &none_marker,
        ] {
            markers.raw_set(m, true)?;
        }
        let take = lua.create_function(|lua, ()| Ok(ctx(lua).pending.take()))?;
        let co: Table = lua.globals().get("coroutine")?;
        let wrap: Function = lua
            .load(
                r#"
                local MARKERS, YIELD, CALL, MULTI, NONE, take, yield, error = ...
                return function(f)
                    return function(...)
                        local r, x = f(...)
                        if MARKERS[r] then
                            if r == NONE then
                                return
                            elseif r == MULTI then
                                return take()
                            elseif r == YIELD then
                                return yield()
                            elseif r == CALL then
                                return x(take())
                            end
                            error(x, 0)
                        end
                        return r
                    end
                end
                "#,
            )
            .set_name("=[rook]")
            .call((
                markers,
                yield_marker.clone(),
                call_marker.clone(),
                multi_marker.clone(),
                none_marker.clone(),
                take,
                co.get::<Function>("yield")?,
                lua.globals().get::<Function>("error")?,
            ))?;
        let instances = weak_table(lua, "v")?;
        let instance_keys = weak_table(lua, "k")?;
        let method_caches = weak_table(lua, "k")?;
        let enum_items = lua.create_table()?;
        let ctx = Rc::new(Self {
            world,
            yield_marker,
            error_marker,
            call_marker,
            multi_marker,
            none_marker,
            pending: RefCell::default(),
            wrap,
            instances,
            instance_keys,
            methods: RefCell::default(),
            member_tables: RefCell::default(),
            method_caches,
            class_method_caches: RefCell::default(),
            runtime_props: RefCell::default(),
            runtime_prop_ids: RefCell::default(),
            enum_items,
            member_ids: RefCell::default(),
            fast_reads: RefCell::default(),
            names: weak_table(lua, "k")?,
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
        profiling::scope!("api");
        let c = ctx(lua);
        let marker = |m: &Table| LuaValue::Table(m.clone());
        Ok(match f(lua, args) {
            Ok(Outcome::Return(v)) if v.is_empty() => {
                MultiValue::from_iter([marker(&c.none_marker)])
            }
            Ok(Outcome::Return(v)) if v.len() == 1 => v,
            Ok(Outcome::Return(v)) => {
                *c.pending.borrow_mut() = v;
                MultiValue::from_iter([marker(&c.multi_marker)])
            }
            Ok(Outcome::Yield) => MultiValue::from_iter([marker(&c.yield_marker)]),
            Ok(Outcome::Call(f, args)) => {
                *c.pending.borrow_mut() = args;
                MultiValue::from_iter([marker(&c.call_marker), LuaValue::Function(f)])
            }
            Err(e) => MultiValue::from_iter([
                marker(&c.error_marker),
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
