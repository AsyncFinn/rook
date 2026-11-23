//! HttpService: JSON, GUIDs and URL encoding; scripts' requests are
//! refused. CoreScripts' requests to Roblox's web domains
//! (`HttpService:RequestInternal`, HttpRbxApiService) go out only where
//! the domain has an endpoint ([`super::requests`]), and fail as offline
//! otherwise, which the CoreScripts handle.

use mlua::{IntoLuaMulti, Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use super::requests::{self as web, WebAnswer, WebRequest};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("HttpService", "JSONEncode") => super::json::encode,
        ("HttpService", "JSONDecode") => super::json::decode,
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

/// A random version 4 UUID in lowercase hex, in braces unless asked
/// otherwise (measured in Studio 0.748, which casts the argument to a
/// boolean: `GenerateGUID("x")` raises `Unable to cast string to bool`).
fn generate_guid(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let braces = a.bool_or(0, true)?;
    let math: Table = lua.globals().get("math")?;
    let random: mlua::Function = math.get("random")?;
    let mut bytes = [0u8; 16];
    for b in &mut bytes {
        *b = random.call::<u8>((0, 255))?;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
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

/// Measured in Studio 0.748: only ASCII letters and digits are kept,
/// `-`, `_`, `.` and `~` escaped too.
fn url_encode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    Outcome::ret(lua, out)
}

/// Scripts' HTTP requests are refused, as in a place that hasn't enabled
/// them.
fn http_disabled(_: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Err(ScriptError(
        "Http requests are not enabled. Enable via game settings".into(),
    ))
}
