//! Argument checking with Roblox-style messages.
//!
//! Message texts follow what Roblox prints for engine functions
//! (`Argument 1 missing or nil`, `invalid argument #1 to 'new' (number
//! expected, got string)`). They are Inference until the conformance suite
//! pins them.

use mlua::{AnyUserData, Function, Lua, MultiValue, UserData, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::instance::LuaInstance;
use crate::ctx::{ApiResult, ScriptError};

pub(crate) struct Args {
    vals: Vec<LuaValue>,
    /// Name used in messages, for example `new` or `FindFirstChild`.
    name: &'static str,
}

/// The type name scripts would see from `typeof`.
pub(crate) fn type_name(v: &LuaValue) -> String {
    match v {
        LuaValue::Nil => "nil".into(),
        LuaValue::Boolean(_) => "boolean".into(),
        LuaValue::Integer(_) | LuaValue::Number(_) => "number".into(),
        LuaValue::String(_) => "string".into(),
        LuaValue::Table(_) => "table".into(),
        LuaValue::Function(_) => "function".into(),
        LuaValue::Thread(_) => "thread".into(),
        LuaValue::Vector(_) => "Vector3".into(),
        LuaValue::Buffer(_) => "buffer".into(),
        LuaValue::UserData(ud) => ud
            .metatable()
            .and_then(|mt| mt.get::<String>("__type"))
            .unwrap_or_else(|_| "userdata".into()),
        LuaValue::LightUserData(_) => "userdata".into(),
        _ => "userdata".into(),
    }
}

pub(crate) fn number_of(v: &LuaValue) -> Option<f64> {
    match v {
        LuaValue::Integer(i) => Some(*i as f64),
        LuaValue::Number(n) => Some(*n),
        // Lua's string-to-number coercion.
        LuaValue::String(s) => s.to_str().ok().and_then(|s| s.trim().parse().ok()),
        _ => None,
    }
}

impl Args {
    pub(crate) fn new(args: MultiValue, name: &'static str) -> Self {
        Self {
            vals: args.into_vec(),
            name,
        }
    }

    /// For methods: take `self` as an instance.
    pub(crate) fn method(args: MultiValue, name: &'static str) -> ApiResult<(InstanceId, Self)> {
        let mut vals = args.into_vec();
        let this = match vals.first() {
            Some(LuaValue::UserData(ud)) => ud.borrow::<LuaInstance>().ok().map(|i| i.0),
            _ => None,
        };
        let Some(this) = this else {
            return Err(ScriptError(format!(
                "Expected ':' not '.' calling member function {name}"
            )));
        };
        vals.remove(0);
        Ok((this, Self { vals, name }))
    }

    pub(crate) fn get(&self, i: usize) -> &LuaValue {
        self.vals.get(i).unwrap_or(&LuaValue::Nil)
    }

    pub(crate) fn rest(&self, from: usize) -> Vec<LuaValue> {
        self.vals.get(from..).map(<[_]>::to_vec).unwrap_or_default()
    }

    pub(crate) fn into_vec(self) -> Vec<LuaValue> {
        self.vals
    }

    fn missing(&self, i: usize) -> ScriptError {
        ScriptError(format!("Argument {} missing or nil", i + 1))
    }

    pub(crate) fn wrong(&self, i: usize, expected: &str) -> ScriptError {
        ScriptError(format!(
            "invalid argument #{} to '{}' ({expected} expected, got {})",
            i + 1,
            self.name,
            type_name(self.get(i))
        ))
    }

    pub(crate) fn number(&self, i: usize) -> ApiResult<f64> {
        match self.get(i) {
            LuaValue::Nil => Err(self.missing(i)),
            v => number_of(v).ok_or_else(|| self.wrong(i, "number")),
        }
    }

    pub(crate) fn opt_number(&self, i: usize) -> ApiResult<Option<f64>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            v => number_of(v)
                .map(Some)
                .ok_or_else(|| self.wrong(i, "number")),
        }
    }

    pub(crate) fn number_or(&self, i: usize, default: f64) -> ApiResult<f64> {
        Ok(self.opt_number(i)?.unwrap_or(default))
    }

    pub(crate) fn string(&self, i: usize) -> ApiResult<String> {
        self.opt_string(i)?.ok_or_else(|| self.missing(i))
    }

    pub(crate) fn opt_string(&self, i: usize) -> ApiResult<Option<String>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            LuaValue::String(s) => Ok(Some(s.to_string_lossy())),
            LuaValue::Integer(n) => Ok(Some(n.to_string())),
            LuaValue::Number(n) => Ok(Some(format_number(*n))),
            _ => Err(self.wrong(i, "string")),
        }
    }

    /// Lua truthiness, as Roblox uses for optional boolean arguments.
    pub(crate) fn truthy(&self, i: usize) -> bool {
        !matches!(self.get(i), LuaValue::Nil | LuaValue::Boolean(false))
    }

    pub(crate) fn instance(&self, i: usize) -> ApiResult<InstanceId> {
        match self.get(i) {
            LuaValue::Nil => Err(self.missing(i)),
            _ => self
                .opt_instance(i)?
                .ok_or_else(|| ScriptError("Unable to cast value to Object".into())),
        }
    }

    pub(crate) fn opt_instance(&self, i: usize) -> ApiResult<Option<InstanceId>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            LuaValue::UserData(ud) => match ud.borrow::<LuaInstance>() {
                Ok(inst) => Ok(Some(inst.0)),
                Err(_) => Err(ScriptError("Unable to cast value to Object".into())),
            },
            _ => Err(ScriptError("Unable to cast value to Object".into())),
        }
    }

    pub(crate) fn function(&self, i: usize) -> ApiResult<Function> {
        match self.get(i) {
            LuaValue::Function(f) => Ok(f.clone()),
            LuaValue::Nil => Err(self.missing(i)),
            _ => Err(self.wrong(i, "function")),
        }
    }

    pub(crate) fn userdata<T: UserData + Clone + 'static>(
        &self,
        i: usize,
        expected: &str,
    ) -> ApiResult<T> {
        self.opt_userdata(i, expected)?
            .ok_or_else(|| self.missing(i))
    }

    pub(crate) fn opt_userdata<T: UserData + Clone + 'static>(
        &self,
        i: usize,
        expected: &str,
    ) -> ApiResult<Option<T>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            LuaValue::UserData(ud) => borrow_clone::<T>(ud)
                .map(Some)
                .ok_or_else(|| self.wrong(i, expected)),
            _ => Err(self.wrong(i, expected)),
        }
    }

    pub(crate) fn vector3(&self, i: usize) -> ApiResult<rook_types::Vector3> {
        match self.get(i) {
            LuaValue::Vector(v) => Ok(rook_types::Vector3::new(v.x(), v.y(), v.z())),
            LuaValue::Nil => Err(self.missing(i)),
            _ => Err(self.wrong(i, "Vector3")),
        }
    }
}

pub(crate) fn borrow_clone<T: UserData + Clone + 'static>(ud: &AnyUserData) -> Option<T> {
    ud.borrow::<T>().ok().map(|v| v.clone())
}

/// Format a number the way Luau's `tostring` does.
pub(crate) fn format_number(n: f64) -> String {
    if n.is_nan() {
        return "nan".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if n == n.trunc() && n.abs() < 1e15 {
        return format!("{}", n as i64);
    }
    // Luau prints the shortest representation that round-trips.
    let s = format!("{n}");
    if s.len() > 20 { format!("{n:e}") } else { s }
}

/// Convert anything to a string the way Luau's `tostring` would, for
/// values that don't need metamethods.
pub(crate) fn display_value(lua: &Lua, v: &LuaValue) -> String {
    let tostring: mlua::Result<Function> = lua.globals().get("tostring");
    match tostring.and_then(|f| f.call::<mlua::LuaString>(v.clone())) {
        Ok(s) => s.to_string_lossy(),
        Err(_) => type_name(v),
    }
}
