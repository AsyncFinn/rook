//! Who users are: names and ids (`users`), friendships (`friends`) and
//! group roles (`groups`), and what they own: badges (`badges`), passes
//! and assets (`inventory`). With no endpoint ([`super::web`]), the
//! session is all there is: its players are the only users, none are
//! friends or in a group, and nobody owns anything.

use mlua::{Lua, MultiValue, Value as LuaValue};
use rook_dom::InstanceId;
use serde_json::{Value as Json, json};

use super::RawMethod;
use super::characters::players_of;
use super::web::{self, WebAnswer, WebRequest};
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Players", "GetUserIdFromNameAsync") => user_id_from_name,
        ("Players", "GetNameFromUserIdAsync") => name_from_user_id,
        ("UserService", "GetUserInfosByUserIdsAsync") => user_infos,
        ("Player", "IsFriendsWith") => is_friends_with,
        ("Player", "GetRankInGroup") => rank_in_group,
        ("Player", "GetRoleInGroup") => role_in_group,
        ("Player", "IsInGroup") => is_in_group,
        ("BadgeService", "UserHasBadgeAsync") => user_has_badge,
        ("MarketplaceService", "UserOwnsGamePassAsync") => owns_game_pass,
        ("MarketplaceService", "PlayerOwnsAsset") => player_owns_asset,
        _ => return None,
    })
}

/// A user in the session: `(user id, name, display name)` of each player.
fn session_users(lua: &Lua) -> Vec<(i64, String, String)> {
    with_world(lua, |w| {
        let Some(players) = w.dom.find_service("Players") else {
            return Vec::new();
        };
        players_of(w, players)
            .into_iter()
            .map(|p| {
                let id = w.dom.number_property(p, "UserId").unwrap_or(0.0) as i64;
                let name = w.dom.name(p).to_owned();
                let display = w
                    .dom
                    .string_property(p, "DisplayName")
                    .filter(|d| !d.is_empty())
                    .unwrap_or_else(|| name.clone());
                (id, name, display)
            })
            .collect()
    })
}

fn id(a: &Args, i: usize) -> ApiResult<i64> {
    Ok(a.number(i)? as i64)
}

/// The `data` array of a 2xx JSON answer.
fn data(answer: WebAnswer) -> Result<Vec<Json>, String> {
    let json = web::json(answer)?;
    Ok(json["data"].as_array().cloned().unwrap_or_default())
}

fn values(lua: &Lua, v: impl mlua::IntoLuaMulti) -> Result<MultiValue, String> {
    v.into_lua_multi(lua).map_err(|e| e.to_string())
}

/// `POST users/v1/usernames/users`.
fn user_id_from_name(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let failed = || "Players:GetUserIdFromNameAsync() failed: Unknown user".to_owned();
    let Some(url) = web::url(lua, "users", "/v1/usernames/users") else {
        let found = session_users(lua)
            .into_iter()
            .find(|(_, n, _)| n.eq_ignore_ascii_case(&name));
        return match found {
            Some((id, ..)) => Outcome::ret(lua, id),
            None => Err(ScriptError(failed())),
        };
    };
    let body = json!({ "usernames": [name], "excludeBannedUsers": true });
    web::call(
        lua,
        WebRequest::post_json(url, &body),
        Box::new(move |lua, answer| {
            let id = data(answer)?.first().and_then(|u| u["id"].as_i64());
            values(lua, id.ok_or_else(failed)?)
        }),
    )
}

/// `GET users/v1/users/{id}`.
fn name_from_user_id(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let id = id(a, 0)?;
    let failed = || "Players:GetNameFromUserIdAsync() failed: Unknown user".to_owned();
    let Some(url) = web::url(lua, "users", &format!("/v1/users/{id}")) else {
        let found = session_users(lua).into_iter().find(|(i, ..)| *i == id);
        return match found {
            Some((_, name, _)) => Outcome::ret(lua, name),
            None => Err(ScriptError(failed())),
        };
    };
    web::call(
        lua,
        WebRequest::get(url),
        Box::new(move |lua, answer| {
            let json = web::json(answer).map_err(|_| failed())?;
            values(lua, json["name"].as_str().ok_or_else(failed)?.to_owned())
        }),
    )
}

/// `POST users/v1/users`: `{Id, Username, DisplayName, HasVerifiedBadge}`
/// for each user found (creator-docs UserService).
fn user_infos(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(ids) = a.get(0) else {
        return Err(a.wrong(0, "Array"));
    };
    let ids: Vec<i64> = ids
        .clone()
        .sequence_values::<f64>()
        .filter_map(Result::ok)
        .map(|n| n as i64)
        .collect();
    let infos = |lua: &Lua, users: Vec<(i64, String, String, bool)>| -> mlua::Result<_> {
        let list = lua.create_table()?;
        for (id, name, display, verified) in users {
            let t = lua.create_table()?;
            t.set("Id", id)?;
            t.set("Username", name)?;
            t.set("DisplayName", display)?;
            t.set("HasVerifiedBadge", verified)?;
            list.push(t)?;
        }
        Ok(list)
    };
    let Some(url) = web::url(lua, "users", "/v1/users") else {
        let users = session_users(lua)
            .into_iter()
            .filter(|(id, ..)| ids.contains(id))
            .map(|(id, name, display)| (id, name, display, false))
            .collect();
        return Outcome::ret(lua, infos(lua, users)?);
    };
    let body = json!({ "userIds": ids, "excludeBannedUsers": false });
    web::call(
        lua,
        WebRequest::post_json(url, &body),
        Box::new(move |lua, answer| {
            let users = data(answer)?
                .iter()
                .map(|u| {
                    (
                        u["id"].as_i64().unwrap_or(0),
                        u["name"].as_str().unwrap_or("").to_owned(),
                        u["displayName"].as_str().unwrap_or("").to_owned(),
                        u["hasVerifiedBadge"].as_bool().unwrap_or(false),
                    )
                })
                .collect();
            values(lua, infos(lua, users).map_err(|e| e.to_string())?)
        }),
    )
}

/// A Player's user id.
fn player_id(lua: &Lua, player: InstanceId) -> i64 {
    with_world(lua, |w| w.dom.number_property(player, "UserId")).unwrap_or(0.0) as i64
}

/// Ask `domain` for `path` and answer with what `decode` makes of its
/// `data` array, or with `local` when the domain has no endpoint.
fn ask<T: mlua::IntoLuaMulti + 'static>(
    lua: &Lua,
    domain: &str,
    path: &str,
    local: T,
    decode: impl FnOnce(Vec<Json>) -> T + 'static,
) -> ApiResult<Outcome> {
    let Some(url) = web::url(lua, domain, path) else {
        return Outcome::ret(lua, local);
    };
    web::call(
        lua,
        WebRequest::get(url),
        Box::new(move |lua, answer| values(lua, decode(data(answer)?))),
    )
}

/// `GET friends/v1/users/{id}/friends/statuses?userIds={other}`.
fn is_friends_with(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (me, other) = (player_id(lua, this), id(a, 0)?);
    let path = format!("/v1/users/{me}/friends/statuses?userIds={other}");
    ask(lua, "friends", &path, false, |d| {
        d.iter().any(|s| s["status"] == "Friends")
    })
}

/// Answer with what `decode` makes of the player's role in group
/// `a[0]`, from `GET groups/v2/users/{id}/groups/roles`.
fn in_group<T: mlua::IntoLuaMulti + 'static>(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
    local: T,
    decode: fn(Option<&Json>) -> T,
) -> ApiResult<Outcome> {
    let group = id(a, 0)?;
    let path = format!("/v2/users/{}/groups/roles", player_id(lua, this));
    ask(lua, "groups", &path, local, move |data| {
        let role = data
            .iter()
            .find(|r| r["group"]["id"].as_i64() == Some(group))
            .map(|r| &r["role"]);
        decode(role)
    })
}

/// The rank, 0 outside the group (creator-docs Player:GetRankInGroup).
fn rank_in_group(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    in_group(lua, this, a, 0, |r| {
        r.and_then(|r| r["rank"].as_i64()).unwrap_or(0)
    })
}

/// The role's name, "Guest" outside the group (creator-docs
/// Player:GetRoleInGroup).
fn role_in_group(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    in_group(lua, this, a, "Guest".to_owned(), |r| {
        r.and_then(|r| r["name"].as_str())
            .unwrap_or("Guest")
            .to_owned()
    })
}

fn is_in_group(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    in_group(lua, this, a, false, |r| r.is_some())
}

/// `GET badges/v1/users/{id}/badges/awarded-dates?badgeIds={badge}`.
fn user_has_badge(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (user, badge) = (id(a, 0)?, id(a, 1)?);
    let path = format!("/v1/users/{user}/badges/awarded-dates?badgeIds={badge}");
    ask(lua, "badges", &path, false, |d| !d.is_empty())
}

/// `GET inventory/v1/users/{id}/items/GamePass/{pass}`.
fn owns_game_pass(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (user, pass) = (id(a, 0)?, id(a, 1)?);
    let path = format!("/v1/users/{user}/items/GamePass/{pass}");
    ask(lua, "inventory", &path, false, |d| !d.is_empty())
}

/// `GET inventory/v1/users/{id}/items/Asset/{asset}` for a Player.
fn player_owns_asset(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let player = a.instance(0)?;
    let asset = id(a, 1)?;
    let path = format!("/v1/users/{}/items/Asset/{asset}", player_id(lua, player));
    ask(lua, "inventory", &path, false, |d| !d.is_empty())
}
