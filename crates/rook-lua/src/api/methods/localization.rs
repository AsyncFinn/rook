//! LocalizationTables and Translators: the entries a table holds (its
//! `Contents`, as JSON), and translating by key or by source text for one
//! locale. Rook has no automatic translation; text without an entry stays
//! as it is.

use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Lua, Table, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::Value;
use serde::{Deserialize, Serialize};

use super::{RawMethod, mutate, ret_instances};
use crate::api::instance::push_instance;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, with_world};

/// The locale Rook runs in: Roblox's `en-us`.
pub(crate) const LOCALE: &str = "en-us";

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("LocalizationTable", "GetTranslator") => get_translator,
        ("LocalizationTable", "SetEntries") => set_entries,
        ("LocalizationTable", "GetEntries") => get_entries,
        ("LocalizationTable", "SetContents") => set_contents,
        ("LocalizationTable", "GetContents") => get_contents,
        ("Translator", "FormatByKey") => format_by_key,
        ("Translator", "Translate" | "RobloxOnlyTranslate") => translate,
        ("LocalizationService", "GetTranslatorForPlayer" | "GetTranslatorForPlayerAsync") => {
            translator_for_player
        }
        ("LocalizationService", "GetTranslatorForLocaleAsync") => translator_for_locale,
        ("LocalizationService", "GetCorescriptLocalizations") => corescript_localizations,
        _ => return None,
    })
}

/// One entry, as `Contents` saves it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Entry {
    key: String,
    source: String,
    context: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    examples: String,
    values: HashMap<String, String>,
}

impl Entry {
    /// The text for `locale`: its own, its language's, or the source.
    fn text(&self, locale: &str) -> &str {
        let language = locale.split('-').next().unwrap_or(locale);
        self.values
            .get(locale)
            .or_else(|| {
                self.values
                    .iter()
                    .find(|(l, _)| l.split('-').next() == Some(language))
                    .map(|(_, v)| v)
            })
            .map_or(&self.source, |v| v)
    }
}

/// A table's entries, parsed once from `Contents` and kept until it
/// changes.
fn entries(lua: &Lua, table: InstanceId) -> Rc<Vec<Entry>> {
    with_world(lua, |w| {
        if let Some(e) = w.localization.get(&table) {
            return e.clone();
        }
        let contents = w.dom.string_property(table, "Contents").unwrap_or_default();
        let parsed: Rc<Vec<Entry>> = Rc::new(serde_json::from_str(&contents).unwrap_or_default());
        w.localization.insert(table, parsed.clone());
        parsed
    })
}

fn store(lua: &Lua, table: InstanceId, list: Vec<Entry>) -> ApiResult<()> {
    let json = serde_json::to_string(&list).map_err(|e| ScriptError(e.to_string()))?;
    with_world(lua, |w| w.localization.insert(table, Rc::new(list)));
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(table, "Contents", Value::String(json))
    })
}

/// Every table a translator looks in: its own, or for one from
/// LocalizationService, the game's tables there.
fn tables_of(lua: &Lua, translator: InstanceId) -> Vec<InstanceId> {
    with_world(lua, |w| match w.translators.get(&translator) {
        Some(Some(table)) => vec![*table],
        _ => w
            .dom
            .find_service("LocalizationService")
            .map(|s| {
                w.dom
                    .descendants(s)
                    .into_iter()
                    .filter(|&d| w.dom.class_name(d) == "LocalizationTable")
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn new_translator(lua: &Lua, table: Option<InstanceId>, locale: &str) -> ApiResult<Outcome> {
    let id = with_world(lua, |w| -> Result<InstanceId, rook_dom::DomError> {
        let id = w.dom.create_by_name("Translator")?;
        w.dom
            .set_property_by_name(id, "LocaleId", Value::String(locale.to_owned()))?;
        w.translators.insert(id, table);
        Ok(id)
    })?;
    Outcome::ret(lua, push_instance(lua, id)?)
}

fn get_translator(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let locale = a.opt_string(0)?.unwrap_or_else(|| LOCALE.to_owned());
    new_translator(lua, Some(this), &locale)
}

fn translator_for_player(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    new_translator(lua, None, LOCALE)
}

fn translator_for_locale(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    new_translator(lua, None, &a.string(0)?)
}

/// The CoreScripts' own tables (clients running them).
fn corescript_localizations(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let tables = with_world(lua, |w| {
        w.core_scripts
            .map(|folder| {
                w.dom
                    .children(folder)
                    .iter()
                    .copied()
                    .filter(|&c| w.dom.class_name(c) == "LocalizationTable")
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    ret_instances(lua, &tables)
}

/// `{ {Key=, Source=, Context=, Example=, Values={locale=text}} }`.
fn set_entries(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let LuaValue::Table(list) = a.get(0) else {
        return Err(a.wrong(0, "table"));
    };
    let mut out = Vec::new();
    for item in list.clone().sequence_values::<Table>() {
        let item = item?;
        let text = |k: &str| item.get::<Option<String>>(k).map(Option::unwrap_or_default);
        let mut values = HashMap::new();
        if let Some(v) = item.get::<Option<Table>>("Values")? {
            for pair in v.pairs::<String, String>() {
                let (locale, value) = pair?;
                values.insert(locale, value);
            }
        }
        out.push(Entry {
            key: text("Key")?,
            source: text("Source")?,
            context: text("Context")?,
            examples: text("Example")?,
            values,
        });
    }
    store(lua, this, out)?;
    Outcome::ret(lua, ())
}

fn get_entries(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let list = lua.create_table()?;
    for (i, e) in entries(lua, this).iter().enumerate() {
        let t = lua.create_table()?;
        t.set("Key", e.key.as_str())?;
        t.set("Source", e.source.as_str())?;
        t.set("Context", e.context.as_str())?;
        t.set("Example", e.examples.as_str())?;
        let values = lua.create_table()?;
        for (locale, text) in &e.values {
            values.set(locale.as_str(), text.as_str())?;
        }
        t.set("Values", values)?;
        list.raw_set(i + 1, t)?;
    }
    Outcome::ret(lua, list)
}

fn set_contents(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let json = a.string(0)?;
    let list: Vec<Entry> =
        serde_json::from_str(&json).map_err(|e| ScriptError(format!("SetContents: {e}")))?;
    store(lua, this, list)?;
    Outcome::ret(lua, ())
}

fn get_contents(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let json =
        serde_json::to_string(&*entries(lua, this)).map_err(|e| ScriptError(e.to_string()))?;
    Outcome::ret(lua, json)
}

fn locale_of(lua: &Lua, translator: InstanceId) -> String {
    with_world(lua, |w| w.dom.string_property(translator, "LocaleId"))
        .unwrap_or_else(|| LOCALE.to_owned())
}

/// The entry's text with `{1}` or `{name}` (any `:format` after the name
/// dropped) replaced from `args`. Confirmed: creator-docs
/// Translator:FormatByKey; a key no table has is an error.
fn format_by_key(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let locale = locale_of(lua, this);
    let text = tables_of(lua, this).into_iter().find_map(|t| {
        entries(lua, t)
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.text(&locale).to_owned())
    });
    let Some(text) = text else {
        return Err(ScriptError(format!(
            "Translator.FormatByKey failed to find a translation for key {key} in locale {locale}"
        )));
    };
    let args = match a.get(1) {
        LuaValue::Table(t) => Some(t.clone()),
        _ => None,
    };
    Outcome::ret(lua, substitute(&text, args.as_ref())?)
}

fn substitute(text: &str, args: Option<&Table>) -> mlua::Result<String> {
    let Some(args) = args else {
        return Ok(text.to_owned());
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('}') else {
            rest = &rest[open..];
            break;
        };
        let name = &rest[open + 1..open + close];
        let name = name.split(':').next().unwrap_or(name).trim();
        let value: LuaValue = match name.parse::<i64>() {
            Ok(i) => args.get(i)?,
            Err(_) => args.get(name)?,
        };
        match value {
            LuaValue::Nil => out.push_str(&rest[open..=open + close]),
            LuaValue::String(s) => out.push_str(&s.to_string_lossy()),
            LuaValue::Integer(n) => out.push_str(&n.to_string()),
            LuaValue::Number(n) => out.push_str(&crate::ctx::args::format_number(n)),
            other => out.push_str(&format!("{other:?}")),
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// The entry whose source is `text`, in `locale`; `text` itself without
/// one.
fn translate(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(1)?;
    let locale = locale_of(lua, this);
    let found = tables_of(lua, this).into_iter().find_map(|t| {
        entries(lua, t)
            .iter()
            .find(|e| e.source == text)
            .map(|e| e.text(&locale).to_owned())
    });
    Outcome::ret(lua, found.unwrap_or(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positional_and_named_arguments() {
        let lua = Lua::new();
        let args = lua.create_table().unwrap();
        args.raw_set(1, "Ana").unwrap();
        args.set("count", 3).unwrap();
        assert_eq!(
            substitute("{1} has {count:int} {missing}", Some(&args)).unwrap(),
            "Ana has 3 {missing}"
        );
    }
}
