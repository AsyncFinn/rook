//! HttpService: JSON, GUIDs and URL encoding; scripts' requests are
//! refused. CoreScripts' requests to Roblox's web domains
//! (`HttpService:RequestInternal`, HttpRbxApiService) go out only where
//! the domain has an endpoint ([`super::web`]), and fail as offline
//! otherwise, which the CoreScripts handle.

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use super::web::{self, WebAnswer, WebRequest};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("HttpService", "JSONEncode") => json_encode,
        ("HttpService", "JSONDecode") => json_decode,
        ("HttpService", "GenerateGUID") => generate_guid,
        ("HttpService", "UrlEncode") => url_encode,
        ("HttpService", "GetAsync" | "PostAsync" | "RequestAsync") => http_disabled,
        ("HttpRbxApiService", "GetAsync") => |l, _, a| {
            let url = api_url(l, &a.string(0)?);
            rbx_api(l, "GET", url, None)
        },
        ("HttpRbxApiService", "GetAsyncFullUrl") => |l, _, a| {
            let url = web::rewrite(l, &a.string(0)?);
            rbx_api(l, "GET", url, None)
        },
        ("HttpRbxApiService", "PostAsync") => |l, _, a| {
            let url = api_url(l, &a.string(0)?);
            rbx_api(
                l,
                "POST",
                url,
                Some((a.string(1)?, content_type(l, a.get(3)))),
            )
        },
        ("HttpRbxApiService", "PostAsyncFullUrl") => |l, _, a| {
            let url = web::rewrite(l, &a.string(0)?);
            rbx_api(
                l,
                "POST",
                url,
                Some((a.string(1)?, content_type(l, a.get(3)))),
            )
        },
        ("HttpRbxApiService", "RequestAsync") => rbx_request,
        _ => return None,
    })
}

/// `RequestInternal(options)` gives a request whose `Start(callback)`
/// calls back with `(sent, response)` from another thread.
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("HttpService", "RequestInternal") => {
            r#"local std = ...
            local task, send = std.task, std.web_request
            return function(_, options)
                local request = {}
                function request:Start(callback)
                    task.spawn(function()
                        callback(send(options))
                    end)
                end
                function request:Cancel() end
                return request
            end"#
        }
        _ => return None,
    })
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let std: Table = lua.named_registry_value("rook.std")?;
    std.set("web_request", api_fn(lua, request_internal)?)
}

/// A path on the legacy `api` domain, which HttpRbxApiService's
/// relative paths (`assets/1/versions`) are on.
fn api_url(lua: &Lua, path: &str) -> Option<String> {
    web::url(lua, "api", &format!("/{}", path.trim_start_matches('/')))
}

/// An `Enum.HttpContentType` as a MIME type; JSON by default.
fn content_type(lua: &Lua, v: &LuaValue) -> &'static str {
    match crate::types::enums::item_name(lua, v, "HttpContentType") {
        Some("ApplicationUrlEncoded") => "application/x-www-form-urlencoded",
        Some("ApplicationXml") => "application/xml",
        Some("TextPlain") => "text/plain",
        Some("TextXml") => "text/xml",
        _ => "application/json",
    }
}

/// HttpRbxApiService's answer: the body of a 2xx response, raised
/// otherwise.
fn rbx_api(
    lua: &Lua,
    method: &str,
    url: Option<String>,
    body: Option<(String, &str)>,
) -> ApiResult<Outcome> {
    let Some(url) = url else {
        return Err(ScriptError("HttpError: ConnectFail".into()));
    };
    let mut request = WebRequest::get(url);
    request.method = method.to_owned();
    if let Some((body, kind)) = body {
        request
            .headers
            .push(("Content-Type".into(), kind.to_owned()));
        request.body = body.into_bytes();
    }
    web::call(
        lua,
        request,
        Box::new(|lua, answer| {
            let response = answer.map_err(|e| format!("HttpError: ConnectFail ({e})"))?;
            if !(200..300).contains(&response.status) {
                return Err(format!("HTTP {}", response.status));
            }
            let body = lua
                .create_string(&response.body)
                .map_err(|e| e.to_string())?;
            body.into_lua_multi(lua).map_err(|e| e.to_string())
        }),
    )
}

/// `RequestAsync({Url, Method, Body, Headers})`: the body, as `rbx_api`.
fn rbx_request(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(options) = a.get(0) else {
        return Err(a.wrong(0, "table"));
    };
    let request = request_of(options)?;
    let url = web::rewrite(lua, &request.url);
    let body = (!request.body.is_empty()).then(|| {
        let kind = request
            .headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            .map_or("application/json", |(_, v)| v.as_str());
        (String::from_utf8_lossy(&request.body).into_owned(), kind)
    });
    rbx_api(lua, &request.method, url, body)
}

/// A request from `{Url, Method, Body, Headers}` (Method defaults to GET).
fn request_of(options: &Table) -> ApiResult<WebRequest> {
    let url: String = options.get("Url")?;
    let method: Option<String> = options.get("Method")?;
    let body: Option<mlua::LuaString> = options.get("Body")?;
    let mut request = WebRequest::get(url);
    if let Some(m) = method {
        request.method = m.to_ascii_uppercase();
    }
    if let Some(b) = body {
        request.body = b.as_bytes().to_vec();
    }
    if let Some(headers) = options.get::<Option<Table>>("Headers")? {
        for pair in headers.pairs::<String, String>() {
            request.headers.push(pair?);
        }
    }
    Ok(request)
}

/// `send(options)` for RequestInternal: `(sent, response)`, never raising.
fn request_internal(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "RequestInternal");
    let LuaValue::Table(options) = a.get(0) else {
        return Err(a.wrong(0, "table"));
    };
    let mut request = request_of(options)?;
    let Some(url) = web::rewrite(lua, &request.url) else {
        return Ok(Outcome::Return(response(lua, Err("offline".into()))?));
    };
    request.url = url;
    web::call(
        lua,
        request,
        Box::new(|lua, answer| response(lua, answer).map_err(|e| e.to_string())),
    )
}

/// `(true, {StatusCode, Body, Headers, HttpError = "OK"})` for an answer,
/// `(false, {StatusCode = 0, Body = "", HttpError = "ConnectFail"})` for
/// none.
fn response(lua: &Lua, answer: WebAnswer) -> mlua::Result<MultiValue> {
    let t = lua.create_table()?;
    let Ok(r) = answer else {
        t.set("StatusCode", 0)?;
        t.set("Body", "")?;
        t.set("HttpError", "ConnectFail")?;
        return (false, t).into_lua_multi(lua);
    };
    t.set("StatusCode", r.status)?;
    t.set("Body", lua.create_string(&r.body)?)?;
    let headers = lua.create_table()?;
    for (name, value) in &r.headers {
        headers.set(name.as_str(), value.as_str())?;
    }
    t.set("Headers", headers)?;
    t.set("HttpError", "OK")?;
    (true, t).into_lua_multi(lua)
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
