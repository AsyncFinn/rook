//! Argument checking with Roblox-style messages.
//!
//! Message texts follow what Roblox prints for engine functions
//! (`Argument 1 missing or nil`, `invalid argument #1 to 'new' (number
//! expected, got string)`). They are Inference until the conformance suite
//! pins them.

use mlua::{AnyUserData, Function, Lua, MultiValue, UserData, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::instance::LuaInstance;
use crate::runtime::ctx::{ApiResult, ScriptError};

pub(crate) struct Args {
    vals: Vec<LuaValue>,
    /// Name used in messages, for example `new` or `FindFirstChild`.
    name: &'static str,
    /// How many of the call's arguments come before `vals[0]`, so messages
    /// number arguments as the script passed them.
    offset: usize,
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
        _ => "userdata".into(),
    }
}

pub(crate) fn number_of(v: &LuaValue) -> Option<f64> {
    match v {
        LuaValue::Integer(i) => Some(*i as f64),
        LuaValue::Number(n) => Some(*n),
        // Lua's string-to-number coercion, hex included.
        LuaValue::String(s) => {
            let s = s.to_str().ok()?;
            let s = s.trim();
            match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                Some(hex) => u64::from_str_radix(hex, 16).ok().map(|n| n as f64),
                None => s.parse().ok(),
            }
        }
        _ => None,
    }
}

/// A number argument of a datatype constructor, read as Roblox reads
/// them: a number, or a string that reads as one; anything else, nil or
/// missing included, is 0. Measured: `Color3.new('New Yeller')` is black
/// and `Vector3.new(true, 2, 3)` is (0, 2, 3), where methods such as
/// `CFrame.Angles` or `Color3:Lerp` error.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Num(pub f64);

impl Num {
    pub(crate) fn f32(self) -> f32 {
        self.0 as f32
    }
}

impl mlua::FromLua for Num {
    fn from_lua(value: LuaValue, _: &Lua) -> mlua::Result<Self> {
        Ok(Self(number_of(&value).unwrap_or(0.0)))
    }
}

impl Args {
    pub(crate) fn new(args: MultiValue, name: &'static str) -> Self {
        Self {
            vals: args.into_vec(),
            name,
            offset: 0,
        }
    }

    /// The arguments from `from` on, still numbered as in the whole call
    /// (`CollectionService:AddTag(nil)` is `Argument 2 missing or nil`,
    /// measured).
    pub(crate) fn shifted(&self, from: usize) -> Self {
        Self {
            vals: self.rest(from),
            name: self.name,
            offset: self.offset + from,
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
        Ok((
            this,
            Self {
                vals,
                name,
                offset: 0,
            },
        ))
    }

    pub(crate) fn get(&self, i: usize) -> &LuaValue {
        self.vals.get(i).unwrap_or(&LuaValue::Nil)
    }

    /// How many arguments were passed, explicit nils included.
    pub(crate) fn len(&self) -> usize {
        self.vals.len()
    }

    pub(crate) fn rest(&self, from: usize) -> Vec<LuaValue> {
        self.vals.get(from..).map(<[_]>::to_vec).unwrap_or_default()
    }

    pub(crate) fn missing(&self, i: usize) -> ScriptError {
        ScriptError(format!("Argument {} missing or nil", self.offset + i + 1))
    }

    pub(crate) fn wrong(&self, i: usize, expected: &str) -> ScriptError {
        ScriptError(format!(
            "invalid argument #{} to '{}' ({expected} expected, got {})",
            self.offset + i + 1,
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

    /// An item of the enum called `enum_name`, as an EnumItem, its number
    /// or its name, giving its value. Measured on
    /// `Players:GetUserThumbnailAsync`: a number that is no item is
    /// `Unable to cast double to ThumbnailSize`, another enum's item
    /// `Unable to cast Material to ThumbnailType`.
    pub(crate) fn enum_value(&self, lua: &Lua, i: usize, enum_name: &str) -> ApiResult<u32> {
        let v = self.get(i);
        if v.is_nil() {
            return Err(self.missing(i));
        }
        crate::types::enums::item_value(lua, v, enum_name).ok_or_else(|| {
            let from = match v {
                LuaValue::Integer(_) | LuaValue::Number(_) => "double".to_owned(),
                LuaValue::UserData(u) => u
                    .borrow::<crate::types::enums::LEnumItem>()
                    .map_or_else(|_| type_name(v), |item| item.enum_name().to_owned()),
                v => type_name(v),
            };
            ScriptError(format!("Unable to cast {from} to {enum_name}"))
        })
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

    /// An `Object` parameter as Roblox casts it: a missing argument errors,
    /// an explicit `nil` is the null object (`None`), anything but an
    /// Instance doesn't cast. Measured: `Debris:AddItem(nil)` and
    /// `workspace:IsDescendantOf(nil)` run, `Debris:AddItem()` is
    /// `Argument 1 missing or nil`. What null means is each method's own.
    pub(crate) fn object(&self, i: usize) -> ApiResult<Option<InstanceId>> {
        if i >= self.len() {
            return Err(self.missing(i));
        }
        self.opt_instance(i)
    }

    /// An `Object` parameter the method then checks is a `class`: null and
    /// other classes are both `None`, as Roblox gives them the same
    /// message (`Parameter 1 must be BasePart in SetPartCollisionGroup.`).
    pub(crate) fn object_of(
        &self,
        lua: &Lua,
        i: usize,
        class: &str,
    ) -> ApiResult<Option<InstanceId>> {
        Ok(self
            .object(i)?
            .filter(|&id| crate::runtime::ctx::with_world(lua, |w| w.dom.is_a(id, class))))
    }

    /// An instance the method requires, for methods whose own check turns
    /// null away as missing (`Humanoid:ApplyDescription(nil)` is
    /// `Argument 1 missing or nil`, measured).
    pub(crate) fn instance(&self, i: usize) -> ApiResult<InstanceId> {
        self.opt_instance(i)?.ok_or_else(|| self.missing(i))
    }

    /// An optional instance: missing and `nil` are both `None`.
    pub(crate) fn opt_instance(&self, i: usize) -> ApiResult<Option<InstanceId>> {
        match self.get(i) {
            LuaValue::Nil => Ok(None),
            LuaValue::UserData(ud) => match ud.borrow::<LuaInstance>() {
                Ok(inst) => Ok(Some(inst.0)),
                Err(_) => Err(cast_to_object()),
            },
            _ => Err(cast_to_object()),
        }
    }

    /// An array of instances (`Instances` in the API dump).
    pub(crate) fn instances(&self, i: usize) -> ApiResult<Vec<InstanceId>> {
        let LuaValue::Table(t) = self.get(i) else {
            return Err(self.wrong(i, "table"));
        };
        t.sequence_values::<LuaValue>()
            .map(|v| match v? {
                LuaValue::UserData(ud) => ud
                    .borrow::<LuaInstance>()
                    .map(|inst| inst.0)
                    .map_err(|_| cast_to_object()),
                _ => Err(cast_to_object()),
            })
            .collect()
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
            LuaValue::Vector(v) => Ok(crate::runtime::ctx::convert::from_lua_vector(*v)),
            LuaValue::Nil => Err(self.missing(i)),
            _ => Err(self.wrong(i, "Vector3")),
        }
    }
}

fn cast_to_object() -> ScriptError {
    ScriptError("Unable to cast value to Object".into())
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
