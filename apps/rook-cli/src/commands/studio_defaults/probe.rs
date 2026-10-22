//! The Luau probe: create each class (or find each service), read the
//! listed properties with `pcall`, and send back what each read as text.

use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::Context;
use rook_reflection::{ClassId, Database, PropertyId};

use super::Gap;
use crate::commands::studio::Studio;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    Edit,
    Client,
}

/// A read value: its `typeof` followed by its components as text.
pub(super) type Raw = Vec<String>;

pub(super) struct Reading {
    /// Studio's `version()`.
    pub(super) version: String,
    pub(super) values: HashMap<(ClassId, PropertyId), Raw>,
}

/// Properties per `execute_luau` call, keeping each answer well under the
/// MCP's 100k character limit.
const CHUNK: usize = 600;

pub(super) fn read(
    studio: &mut Studio,
    db: &Database,
    gaps: &[Gap],
    side: Side,
) -> anyhow::Result<Reading> {
    let datamodel = match side {
        Side::Edit => "Edit",
        Side::Client => "Client",
    };
    let mut reading = Reading {
        version: String::new(),
        values: HashMap::new(),
    };
    let mut start = 0;
    while start < gaps.len() {
        let mut end = start;
        let mut size = 0;
        while end < gaps.len() && (end == start || size + gaps[end].properties.len() <= CHUNK) {
            size += gaps[end].properties.len();
            end += 1;
        }
        let answer = studio
            .run(&script(db, &gaps[start..end]), datamodel)
            .with_context(|| {
                format!(
                    "probing {datamodel} from {}",
                    db.class(gaps[start].class).name
                )
            })?;
        let rows: Vec<Vec<String>> = serde_json::from_str(&answer).with_context(|| {
            format!(
                "probe answer: {}",
                answer.chars().take(300).collect::<String>()
            )
        })?;
        for mut row in rows {
            anyhow::ensure!(row.len() >= 3, "short probe row {row:?}");
            let raw = row.split_off(2);
            let (class, property) = (&row[0], &row[1]);
            if class == "version" {
                reading.version.clone_from(property);
                continue;
            }
            let class = db
                .class_by_name(class)
                .context("probe returned an unknown class")?;
            if let Some(prop) = db.find_property(class.id, property) {
                reading.values.insert((class.id, prop.id), raw);
            }
        }
        eprintln!("{datamodel}: {end}/{} classes", gaps.len());
        start = end;
    }
    Ok(reading)
}

/// The probe for `gaps`. Services are only found, never created: the place
/// gains none, and `GetService("NetworkServer")` on a client tears down
/// its replicator and crashes Studio (0.743). Created instances are
/// destroyed after reading.
fn script(db: &Database, gaps: &[Gap]) -> String {
    let mut list = String::new();
    for gap in gaps {
        let names: Vec<String> = gap
            .properties
            .iter()
            .map(|&p| format!("{:?}", db.property(p).name))
            .collect();
        let _ = writeln!(
            list,
            "    {{{:?}, {}, {{{}}}}},",
            db.class(gap.class).name,
            gap.service,
            names.join(", ")
        );
    }
    format!(
        r#"local probes = {{
{list}}}
local function num(x)
    return string.format("%.17g", x)
end
local function encode(v)
    local t = typeof(v)
    if t == "boolean" then return {{t, tostring(v)}}
    elseif t == "number" then return {{t, num(v)}}
    elseif t == "string" then return {{t, v}}
    elseif t == "EnumItem" then return {{t, tostring(v.Value), tostring(v.EnumType)}}
    elseif t == "Vector3" then return {{t, num(v.X), num(v.Y), num(v.Z)}}
    elseif t == "Vector2" then return {{t, num(v.X), num(v.Y)}}
    elseif t == "Color3" then return {{t, num(v.R), num(v.G), num(v.B)}}
    elseif t == "UDim" then return {{t, num(v.Scale), num(v.Offset)}}
    elseif t == "UDim2" then
        return {{t, num(v.X.Scale), num(v.X.Offset), num(v.Y.Scale), num(v.Y.Offset)}}
    elseif t == "CFrame" then
        local out = {{t}}
        for _, c in {{v:GetComponents()}} do
            table.insert(out, num(c))
        end
        return out
    elseif t == "BrickColor" then return {{t, tostring(v.Number)}}
    elseif t == "NumberRange" then return {{t, num(v.Min), num(v.Max)}}
    elseif t == "Instance" then return {{t, v.ClassName}}
    elseif t == "Content" then return {{t, v.SourceType.Name, v.Uri or ""}}
    else return {{t, tostring(v)}}
    end
end
local out = {{{{"version", version(), ""}}}}
for i, probe in probes do
    local class, service, props = probe[1], probe[2], probe[3]
    local ok, inst = pcall(function()
        if service then
            return game:FindService(class)
        end
        return Instance.new(class)
    end)
    if ok and inst then
        for _, p in props do
            local read, v = pcall(function()
                return inst[p]
            end)
            if read then
                local row = {{class, p}}
                for _, part in encode(v) do
                    table.insert(row, part)
                end
                table.insert(out, row)
            end
        end
        if not service then
            inst:Destroy()
        end
    end
    if i % 25 == 0 then
        task.wait()
    end
end
return game:GetService("HttpService"):JSONEncode(out)
"#,
    )
}
