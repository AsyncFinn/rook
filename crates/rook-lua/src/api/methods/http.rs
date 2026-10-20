//! HttpService: JSON, GUIDs and URL encoding; requests are refused.

use mlua::{Lua, Table, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("HttpService", "JSONEncode") => json_encode,
        ("HttpService", "JSONDecode") => json_decode,
        ("HttpService", "GenerateGUID") => generate_guid,
        ("HttpService", "UrlEncode") => url_encode,
        ("HttpService", "GetAsync" | "PostAsync" | "RequestAsync") => http_disabled,
        _ => return None,
    })
}

fn to_json(v: &LuaValue, depth: usize) -> ApiResult<serde_json::Value> {
    use serde_json::Value as J;
    if depth > 100 {
        return Err(ScriptError("Can't convert to JSON".into()));
    }
    Ok(match v {
        LuaValue::Boolean(b) => J::Bool(*b),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            let n = crate::ctx::args::number_of(v).unwrap_or(0.0);
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
                            crate::ctx::args::format_number(
                                crate::ctx::args::number_of(&k).unwrap_or(0.0),
                            )
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

fn json_encode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let json = to_json(a.get(0), 0)?;
    Outcome::ret(lua, json.to_string())
}

fn json_decode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let json: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| ScriptError("Can't parse JSON".into()))?;
    Outcome::ret(lua, from_json(lua, &json)?)
}

/// A random version 4 UUID, uppercase, in braces unless asked otherwise
/// (creator-docs HttpService:GenerateGUID).
fn generate_guid(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
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

fn url_encode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
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
fn http_disabled(_: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Err(ScriptError(
        "Http requests are not enabled. Enable via game settings".into(),
    ))
}
