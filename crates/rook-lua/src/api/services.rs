//! Service members: DataStoreService (in memory), HttpService helpers.

use mlua::{Lua, Table, Value as LuaValue};
use rook_dom::InstanceId;

use super::instance::push_instance;
use super::methods::RawMethod;
use crate::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("DataStoreService", "GetDataStore") => get_data_store,
        ("DataStoreService", "GetGlobalDataStore") => get_global_data_store,
        ("DataStoreService", "GetOrderedDataStore") => get_ordered_data_store,
        ("DataStoreService", "GetRequestBudgetForRequestType") => request_budget,
        ("HttpService", "JSONEncode") => json_encode,
        ("HttpService", "JSONDecode") => json_decode,
        ("HttpService", "GenerateGUID") => generate_guid,
        ("HttpService", "UrlEncode") => url_encode,
        ("HttpService", "GetAsync" | "PostAsync" | "RequestAsync") => http_disabled,
        _ => return None,
    })
}

/// Methods written in Luau. Each source gets the engine's `std` table.
pub(crate) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("TweenService", "Create") => {
            "local t = (...).tween return function(self, ...) return t.create(...) end"
        }
        ("TweenService", "GetValue") => {
            "local t = (...).tween return function(self, alpha, style, direction) return t.getValue(alpha, style, direction) end"
        }
        ("TweenBase", "Play") => "local t = (...).tween return function(self) t.play(self) end",
        ("TweenBase", "Pause") => "local t = (...).tween return function(self) t.pause(self) end",
        ("TweenBase", "Cancel") => "local t = (...).tween return function(self) t.cancel(self) end",
        ("GuiObject", "TweenPosition") => {
            r#"local t = (...).tween
            return function(self, position, direction, style, time, override, callback)
                return t.gui(self, { Position = position }, direction, style, time, override, callback)
            end"#
        }
        ("GuiObject", "TweenSize") => {
            r#"local t = (...).tween
            return function(self, size, direction, style, time, override, callback)
                return t.gui(self, { Size = size }, direction, style, time, override, callback)
            end"#
        }
        ("GuiObject", "TweenSizeAndPosition") => {
            r#"local t = (...).tween
            return function(self, size, position, direction, style, time, override, callback)
                return t.gui(self, { Size = size, Position = position }, direction, style, time, override, callback)
            end"#
        }
        // Invoke waits until a callback is set (creator-docs
        // BindableFunction.OnInvoke); Rook checks once per frame.
        ("BindableFunction", "Invoke") => {
            r#"
            local std = ...
            local wait, get_callback = std.task.wait, std.get_callback
            return function(self, ...)
                local callback = get_callback(self, "OnInvoke")
                while not callback do
                    wait()
                    callback = get_callback(self, "OnInvoke")
                end
                return callback(...)
            end
            "#
        }
        ("GlobalDataStore", "GetAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key)
                return ds.copy(ds.data(self)[ds.key(key)])
            end
            "#
        }
        ("GlobalDataStore", "SetAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key, value)
                ds.data(self)[ds.key(key)] = ds.copy(value, true)
            end
            "#
        }
        ("GlobalDataStore", "UpdateAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key, transform)
                local k = ds.key(key)
                local data = ds.data(self)
                local new = transform(ds.copy(data[k]))
                if new ~= nil then
                    data[k] = ds.copy(new, true)
                end
                return ds.copy(data[k])
            end
            "#
        }
        ("GlobalDataStore", "IncrementAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key, delta)
                local k = ds.key(key)
                local data = ds.data(self)
                local value = (data[k] or 0) + (delta or 1)
                data[k] = value
                return value
            end
            "#
        }
        ("GlobalDataStore", "RemoveAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            return function(self, key)
                local k = ds.key(key)
                local data = ds.data(self)
                local old = data[k]
                data[k] = nil
                return old
            end
            "#
        }
        // Pages are plain tables here (Approximation: Roblox returns a
        // DataStorePages instance; scripts use the same members).
        ("OrderedDataStore", "GetSortedAsync") => {
            r#"
            local std = ...
            local ds = std.datastore
            local sort = std.table.sort
            return function(self, ascending, pageSize, minValue, maxValue)
                local entries = {}
                for key, value in ds.data(self) do
                    if type(value) == "number"
                        and (minValue == nil or value >= minValue)
                        and (maxValue == nil or value <= maxValue) then
                        entries[#entries + 1] = { key = key, value = value }
                    end
                end
                sort(entries, function(a, b)
                    if ascending then return a.value < b.value end
                    return a.value > b.value
                end)
                local size = pageSize or 50
                local pages = { IsFinished = false }
                local start = 1
                local function page()
                    local out = {}
                    for i = start, math.min(start + size - 1, #entries) do
                        out[#out + 1] = entries[i]
                    end
                    return out
                end
                pages.IsFinished = #entries < start + size
                function pages:GetCurrentPage()
                    return page()
                end
                function pages:AdvanceToNextPageAsync()
                    start += size
                    pages.IsFinished = #entries < start + size
                end
                return pages
            end
            "#
        }
        _ => return None,
    })
}

/// Shared helpers for the DataStore sources, built once.
pub(crate) fn datastore_helpers(lua: &Lua) -> mlua::Result<Table> {
    lua.load(
        r#"
        local error, type, typeof, tostring, pairs, setmetatable = ...
        local stores = setmetatable({}, { __mode = "k" })
        local ds = {}
        function ds.data(store)
            local t = stores[store]
            if not t then
                t = {}
                stores[store] = t
            end
            return t
        end
        function ds.key(key)
            if type(key) ~= "string" and type(key) ~= "number" then
                error("Argument 1 missing or nil", 3)
            end
            return tostring(key)
        end
        -- Data stores keep copies of plain data only (creator-docs Data
        -- stores: tables, strings, numbers, booleans).
        function ds.copy(value, checking, seen)
            local t = type(value)
            if t == "table" then
                seen = seen or {}
                if seen[value] then
                    error("Tables with cycles cannot be stored", 0)
                end
                seen[value] = true
                local out = {}
                for k, v in pairs(value) do
                    out[ds.copy(k, checking, seen)] = ds.copy(v, checking, seen)
                end
                seen[value] = nil
                return out
            elseif t == "string" or t == "number" or t == "boolean" or t == "nil" then
                return value
            elseif checking then
                error(("Cannot store %s in data store. Data stores can only accept valid UTF-8 characters."):format(typeof(value)), 0)
            end
            return nil
        end
        return ds
        "#,
    )
    .set_name("=[rook] DataStore")
    .call((
        lua.globals().get::<LuaValue>("error")?,
        lua.globals().get::<LuaValue>("type")?,
        lua.globals().get::<LuaValue>("typeof")?,
        lua.globals().get::<LuaValue>("tostring")?,
        lua.globals().get::<LuaValue>("pairs")?,
        lua.globals().get::<LuaValue>("setmetatable")?,
    ))
}

// ---- DataStoreService -----------------------------------------------------------------

/// One store object per (kind, name, scope), as Roblox returns the same
/// object for repeated requests. Contents live in memory for this run
/// (Approximation: Rook has no persistent data stores yet).
fn store(lua: &Lua, class: &str, name: &str, scope: &str) -> ApiResult<Outcome> {
    let cache: Table = match lua.named_registry_value::<Option<Table>>("rook.datastores")? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            lua.set_named_registry_value("rook.datastores", t.clone())?;
            t
        }
    };
    let key = format!("{class}\0{name}\0{scope}");
    if let Some(u) = cache.raw_get::<Option<mlua::AnyUserData>>(key.as_str())? {
        return Outcome::ret(lua, u);
    }
    let id = with_world(lua, |w| {
        let id = w.dom.create_by_name(class).ok()?;
        w.dom.set_name(id, name.to_owned()).ok()?;
        Some(id)
    })
    .ok_or_else(|| ScriptError(format!("{class} is not available")))?;
    let ud = push_instance(lua, id)?;
    cache.raw_set(key, ud.clone())?;
    Outcome::ret(lua, ud)
}

fn get_data_store(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let scope = a.opt_string(1)?.unwrap_or_else(|| "global".into());
    store(lua, "DataStore", &name, &scope)
}

fn get_global_data_store(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    store(lua, "GlobalDataStore", "GlobalDataStore", "global")
}

fn get_ordered_data_store(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let scope = a.opt_string(1)?.unwrap_or_else(|| "global".into());
    store(lua, "OrderedDataStore", &name, &scope)
}

/// No request limits locally.
fn request_budget(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 60)
}

// ---- HttpService ------------------------------------------------------------------------------

fn to_json(v: &LuaValue, depth: usize) -> ApiResult<serde_json::Value> {
    use serde_json::Value as J;
    if depth > 100 {
        return Err(ScriptError("Can't convert to JSON".into()));
    }
    Ok(match v {
        LuaValue::Nil => J::Null,
        LuaValue::Boolean(b) => J::Bool(*b),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            let n = crate::args::number_of(v).unwrap_or(0.0);
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                J::Number((n as i64).into())
            } else {
                serde_json::Number::from_f64(n).map_or(J::Null, J::Number)
            }
        }
        LuaValue::String(s) => J::String(s.to_string_lossy()),
        LuaValue::Table(t) => {
            let len = t.raw_len();
            let count = t.clone().pairs::<LuaValue, LuaValue>().count();
            if len > 0 && count == len {
                let mut out = Vec::with_capacity(len);
                for i in 1..=len {
                    out.push(to_json(&t.raw_get::<LuaValue>(i)?, depth + 1)?);
                }
                J::Array(out)
            } else {
                let mut out = serde_json::Map::new();
                for pair in t.clone().pairs::<LuaValue, LuaValue>() {
                    let (k, v) = pair?;
                    let key = match &k {
                        LuaValue::String(s) => s.to_string_lossy(),
                        LuaValue::Integer(_) | LuaValue::Number(_) => {
                            crate::args::format_number(crate::args::number_of(&k).unwrap_or(0.0))
                        }
                        _ => return Err(ScriptError("Can't convert to JSON".into())),
                    };
                    out.insert(key, to_json(&v, depth + 1)?);
                }
                J::Object(out)
            }
        }
        _ => J::Null,
    })
}

fn from_json(lua: &Lua, v: &serde_json::Value) -> mlua::Result<LuaValue> {
    use serde_json::Value as J;
    Ok(match v {
        J::Null => LuaValue::Nil,
        J::Bool(b) => LuaValue::Boolean(*b),
        J::Number(n) => LuaValue::Number(n.as_f64().unwrap_or(0.0)),
        J::String(s) => LuaValue::String(lua.create_string(s)?),
        J::Array(a) => {
            let t = lua.create_table_with_capacity(a.len(), 0)?;
            for (i, x) in a.iter().enumerate() {
                t.raw_set(i + 1, from_json(lua, x)?)?;
            }
            LuaValue::Table(t)
        }
        J::Object(o) => {
            let t = lua.create_table_with_capacity(0, o.len())?;
            for (k, x) in o {
                t.raw_set(k.as_str(), from_json(lua, x)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

fn json_encode(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let json = to_json(a.get(0), 0)?;
    Outcome::ret(lua, json.to_string())
}

fn json_decode(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| ScriptError("Can't parse JSON".into()))?;
    Outcome::ret(lua, from_json(lua, &json)?)
}

/// A random version 4 UUID, uppercase, in braces unless asked otherwise
/// (creator-docs HttpService:GenerateGUID).
fn generate_guid(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let braces = !matches!(a.get(0), LuaValue::Boolean(false));
    let math: Table = lua.globals().get("math")?;
    let random: mlua::Function = math.get("random")?;
    let mut bytes = [0u8; 16];
    for b in &mut bytes {
        *b = random.call::<u8>((0, 255))?;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    let guid = format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    );
    Outcome::ret(lua, if braces { format!("{{{guid}}}") } else { guid })
}

fn url_encode(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Outcome::ret(lua, out)
}

/// HTTP requests are off unless the place enables them; Rook doesn't
/// make network requests on behalf of archived places yet.
fn http_disabled(_: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Err(ScriptError(
        "Http requests are not enabled. Enable via game settings".into(),
    ))
}
