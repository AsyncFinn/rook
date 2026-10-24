//! Text filtering: `TextService:FilterStringAsync` and its
//! TextFilterResult, and Chat's older filtering members. With no endpoint
//! for the `textfilter` domain ([`super::web`]) text passes through
//! unchanged: a local session has no one to filter for. Roblox filters
//! through a service of its own with no public API, so the request is
//! Rook's (`docs/design/web-endpoints.md`).

use mlua::Lua;
use rook_dom::InstanceId;
use serde_json::json;

use super::RawMethod;
use super::web::{self, WebRequest};
use crate::api::instance::push_instance;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};

/// A text as filtered for chat between users, and for everyone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Filtered {
    chat: String,
    broadcast: String,
}

impl Filtered {
    fn unchanged(text: String) -> Self {
        Self {
            chat: text.clone(),
            broadcast: text,
        }
    }
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("TextService", "FilterStringAsync") => filter_string,
        ("TextFilterResult", "GetChatForUserAsync") => |l, t, _| result(l, t, false),
        ("TextFilterResult", "GetNonChatStringForUserAsync") => |l, t, _| result(l, t, false),
        ("TextFilterResult", "GetNonChatStringForBroadcastAsync") => |l, t, _| result(l, t, true),
        ("Chat", "FilterStringAsync") => |l, _, a| chat_filter(l, a, false),
        ("Chat", "FilterStringForBroadcast") => |l, _, a| chat_filter(l, a, true),
        _ => return None,
    })
}

/// Filter `text` from `user` and answer with what `answer` makes of it.
fn filter(
    lua: &Lua,
    text: String,
    user: i64,
    context: &str,
    answer: fn(&Lua, Filtered) -> mlua::Result<mlua::MultiValue>,
) -> ApiResult<Outcome> {
    let Some(url) = web::url(lua, "textfilter", "/v1/filter-text") else {
        return Ok(Outcome::Return(answer(lua, Filtered::unchanged(text))?));
    };
    let body = json!({ "text": text, "userId": user, "context": context });
    web::call(
        lua,
        WebRequest::post_json(url, &body),
        Box::new(move |lua, reply| {
            let json = web::json(reply)?;
            let field = |name: &str| json[name].as_str().map(str::to_owned);
            let filtered = Filtered {
                chat: field("chat").ok_or("no chat in the answer")?,
                broadcast: field("broadcast").ok_or("no broadcast in the answer")?,
            };
            answer(lua, filtered).map_err(|e| e.to_string())
        }),
    )
}

/// `FilterStringAsync(text, fromUserId, context)`: a TextFilterResult.
fn filter_string(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let user = a.number(1)? as i64;
    let context =
        crate::types::enums::item_name(lua, a.get(2), "TextFilterContext").unwrap_or("PrivateChat");
    filter(lua, text, user, context, |lua, filtered| {
        let result = with_world(lua, |w| {
            let id = w.dom.create_by_name("TextFilterResult").expect("a class");
            w.filter_results.insert(id, filtered);
            id
        });
        mlua::IntoLuaMulti::into_lua_multi(push_instance(lua, result)?, lua)
    })
}

fn result(lua: &Lua, this: InstanceId, broadcast: bool) -> ApiResult<Outcome> {
    let text = with_world(lua, |w| {
        w.filter_results.get(&this).map(|f| {
            if broadcast {
                f.broadcast.clone()
            } else {
                f.chat.clone()
            }
        })
    });
    Outcome::ret(lua, text.unwrap_or_default())
}

/// `Chat:FilterStringAsync(text, from, to)` and
/// `FilterStringForBroadcast(text, from)`: the text as filtered.
fn chat_filter(lua: &Lua, a: &Args, broadcast: bool) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let from = a.instance(1)?;
    let user = with_world(lua, |w| w.dom.number_property(from, "UserId")).unwrap_or(0.0) as i64;
    if broadcast {
        filter(lua, text, user, "PublicChat", |lua, f| {
            mlua::IntoLuaMulti::into_lua_multi(f.broadcast, lua)
        })
    } else {
        filter(lua, text, user, "PrivateChat", |lua, f| {
            mlua::IntoLuaMulti::into_lua_multi(f.chat, lua)
        })
    }
}
