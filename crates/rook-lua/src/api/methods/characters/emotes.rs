//! HumanoidDescription's emotes: the ones a player owns (`GetEmotes`,
//! name to animation asset ids) and the ones on the emote wheel
//! (`GetEquippedEmotes`, slot to name). Both live in the description's
//! own string properties, as Roblox keeps them, so copies, replication and
//! files carry them (docs/behavior/characters/rigs.md#emotes).

use std::collections::BTreeMap;

use mlua::{Lua, Table, Value as LuaValue};
use rook_dom::{Dom, InstanceId};
use rook_types::Value;

use crate::api::methods::mutate;
use crate::ctx::args::{Args, format_number, type_name};
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::signal::SignalKey;
use crate::threads::fire_with;

const OWNED: &str = "EmotesDataInternal";
const EQUIPPED: &str = "EquippedEmotesDataInternal";

/// Owned emotes by name, in name order as Roblox writes them.
type Owned = BTreeMap<String, Vec<i64>>;
/// Equipped emote names by slot.
type Equipped = BTreeMap<i64, String>;

/// Measured: `name^id^id^\` per emote, by name (`Odd^Name^10^20^\Point^507770453^\`).
fn write_owned(owned: &Owned) -> String {
    let mut out = String::new();
    for (name, ids) in owned {
        out.push_str(name);
        for id in ids {
            out.push('^');
            out.push_str(&id.to_string());
        }
        out.push_str("^\\");
    }
    out
}

/// Names may hold `^` (Studio writes `Odd^Name` as it is), so the ids are
/// the whole numbers at the end of an entry and the name is the rest
/// (Inference: how Roblox reads such a name back is unmeasured).
fn read_owned(text: &str) -> Owned {
    let mut owned = Owned::new();
    for entry in text.split('\\').filter(|e| !e.is_empty()) {
        let mut fields: Vec<&str> = entry
            .strip_suffix('^')
            .unwrap_or(entry)
            .split('^')
            .collect();
        let mut ids = Vec::new();
        while fields.len() > 1
            && let Some(id) = fields.last().and_then(|f| f.parse::<i64>().ok())
        {
            ids.push(id);
            fields.pop();
        }
        ids.reverse();
        if !ids.is_empty() {
            owned.insert(fields.join("^"), ids);
        }
    }
    owned
}

/// Measured: `slot^name\` per slot, by slot (`1^Point\3^Wave\`).
fn write_equipped(equipped: &Equipped) -> String {
    let mut out = String::new();
    for (slot, name) in equipped {
        out.push_str(&format!("{slot}^{name}\\"));
    }
    out
}

fn read_equipped(text: &str) -> Equipped {
    (text.split('\\'))
        .filter_map(|entry| {
            let (slot, name) = entry.split_once('^')?;
            Some((slot.parse().ok()?, name.to_owned()))
        })
        .collect()
}

fn owned(dom: &Dom, d: InstanceId) -> Owned {
    read_owned(&dom.string_property(d, OWNED).unwrap_or_default())
}

fn equipped(dom: &Dom, d: InstanceId) -> Equipped {
    read_equipped(&dom.string_property(d, EQUIPPED).unwrap_or_default())
}

fn owned_table(lua: &Lua, owned: &Owned) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for (name, ids) in owned {
        t.raw_set(
            name.as_str(),
            lua.create_sequence_from(ids.iter().map(|&id| id as f64))?,
        )?;
    }
    Ok(t)
}

/// Measured: an array of `{Slot = n, Name = name}`, by slot.
fn equipped_table(lua: &Lua, equipped: &Equipped) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    for (slot, name) in equipped {
        let entry = lua.create_table()?;
        entry.raw_set("Slot", *slot as f64)?;
        entry.raw_set("Name", name.as_str())?;
        t.raw_push(entry)?;
    }
    Ok(t)
}

/// Store `owned` and fire EmotesChanged with it. Measured: every set or
/// add fires, even one that changes nothing.
fn store_owned(lua: &Lua, d: InstanceId, owned: &Owned) -> ApiResult<()> {
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(d, OWNED, Value::String(write_owned(owned)))
    })?;
    fire_with(lua, &SignalKey::event(d, "EmotesChanged"), || {
        owned_table(lua, owned)
            .map(|t| vec![LuaValue::Table(t)])
            .unwrap_or_default()
    });
    Ok(())
}

fn store_equipped(lua: &Lua, d: InstanceId, equipped: &Equipped) -> ApiResult<()> {
    let text = write_equipped(equipped);
    mutate(lua, |w| {
        w.dom.set_property_by_name(d, EQUIPPED, Value::String(text))
    })?;
    fire_with(lua, &SignalKey::event(d, "EquippedEmotesChanged"), || {
        equipped_table(lua, equipped)
            .map(|t| vec![LuaValue::Table(t)])
            .unwrap_or_default()
    });
    Ok(())
}

/// Roblox's names for what a table argument couldn't be cast from.
fn cast_error(v: &LuaValue, to: &str) -> ScriptError {
    ScriptError(match v {
        LuaValue::Nil => "Argument 1 missing or nil".into(),
        LuaValue::Integer(_) | LuaValue::Number(_) => format!("Unable to cast double to {to}"),
        LuaValue::Boolean(_) => format!("Unable to cast bool to {to}"),
        v => format!("Unable to cast {} to {to}", type_name(v)),
    })
}

fn whole(v: &LuaValue) -> Option<f64> {
    match v {
        LuaValue::Integer(i) => Some(*i as f64),
        LuaValue::Number(n) => Some(*n),
        _ => None,
    }
}

pub(super) fn get_emotes(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let owned = with_world(lua, |w| owned(&w.dom, this));
    Outcome::ret(lua, owned_table(lua, &owned)?)
}

pub(super) fn get_equipped_emotes(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let equipped = with_world(lua, |w| equipped(&w.dom, this));
    Outcome::ret(lua, equipped_table(lua, &equipped)?)
}

/// `SetEmotes({[name] = {ids}})`. Measured: number keys become names
/// (`5` is `"5"`); ids that aren't whole numbers are dropped (`1.5`, `"7"`),
/// and so are names left with none.
pub(super) fn set_emotes(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(given) = a.get(0) else {
        return Err(cast_error(a.get(0), "Dictionary"));
    };
    let mut owned = Owned::new();
    for pair in given.pairs::<LuaValue, LuaValue>() {
        let (key, value) = pair?;
        let name = match &key {
            LuaValue::String(s) => s.to_string_lossy(),
            k => match whole(k) {
                Some(n) => format_number(n),
                None => continue,
            },
        };
        let LuaValue::Table(list) = value else {
            continue;
        };
        let ids: Vec<i64> = (list.sequence_values::<LuaValue>())
            .filter_map(|v| whole(&v.ok()?).filter(|n| n.fract() == 0.0))
            .map(|n| n as i64)
            .collect();
        if !ids.is_empty() {
            owned.insert(name, ids);
        }
    }
    store_owned(lua, this, &owned)?;
    Outcome::ret(lua, ())
}

/// `AddEmote(name, id)`: the id joins the name's list once.
pub(super) fn add_emote(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let id = a.number(1)? as i64;
    let mut owned = with_world(lua, |w| owned(&w.dom, this));
    let ids = owned.entry(name).or_default();
    if !ids.contains(&id) {
        ids.push(id);
    }
    store_owned(lua, this, &owned)?;
    Outcome::ret(lua, ())
}

/// `RemoveEmote(name)`. Measured: a name it doesn't have fires nothing.
pub(super) fn remove_emote(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let mut owned = with_world(lua, |w| owned(&w.dom, this));
    if owned.remove(&name).is_some() {
        store_owned(lua, this, &owned)?;
    }
    Outcome::ret(lua, ())
}

/// `SetEquippedEmotes(list)`. Measured: the list is read up to its first
/// nil; a name alone takes the next slot counting names alone from 1, a
/// `{Slot, Name}` table its slot (rounded, any number, even 0 or below),
/// anything else is skipped, and a later entry for a slot replaces an
/// earlier one. A table with no first item but other keys is a Dictionary.
pub(super) fn set_equipped_emotes(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(given) = a.get(0) else {
        return Err(cast_error(a.get(0), "Array"));
    };
    if given.raw_get::<LuaValue>(1)?.is_nil() && !given.is_empty() {
        return Err(ScriptError("Unable to cast Dictionary to Array".into()));
    }
    let mut equipped = Equipped::new();
    let mut alone = 0;
    for value in given.sequence_values::<LuaValue>() {
        match value? {
            LuaValue::String(name) => {
                alone += 1;
                equipped.insert(alone, name.to_string_lossy());
            }
            LuaValue::Table(entry) => {
                let slot = whole(&entry.raw_get("Slot")?);
                let name = entry.raw_get::<LuaValue>("Name")?;
                if let (Some(slot), LuaValue::String(name)) = (slot, name) {
                    equipped.insert(slot.round() as i64, name.to_string_lossy());
                }
            }
            _ => {}
        }
    }
    store_equipped(lua, this, &equipped)?;
    Outcome::ret(lua, ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn studio_text_reads_back() {
        let text = "Odd^Name^10^20^\\Point^507770453^\\Wave^507770239^\\";
        let owned = read_owned(text);
        assert_eq!(owned["Odd^Name"], [10, 20]);
        assert_eq!(write_owned(&owned), text);
        let text = "1^Point\\3^Wave\\";
        let equipped = read_equipped(text);
        assert_eq!(equipped[&3], "Wave");
        assert_eq!(write_equipped(&equipped), text);
    }
}
