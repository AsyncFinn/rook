//! Turn the probe's readings into Rook values, drop what isn't a real
//! default, and write the generated table.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;
use rbx_types::VariantType as T;
use rook_reflection::{Database, PropertyKind, PropertyType, Serialization};

use super::Gap;
use super::probe::{Raw, Reading};

/// Properties whose value depends on the window, the session, the place
/// or the machine rather than the class, by name on any class.
const SESSION: &[&str] = &[
    "AbsoluteCanvasSize",
    "AbsoluteContentSize",
    "AbsolutePosition",
    "AbsoluteRotation",
    "AbsoluteSize",
    "AbsoluteWindowSize",
    "ViewportSize",
    "UserId",
    "JobId",
    "PlaceId",
    "GameId",
    "CreatorId",
    "CreatorType",
    "PlaceVersion",
    "PrivateServerId",
    "PrivateServerOwnerId",
    "DistributedGameTime",
    "RunState",
    "IsWindows",
    "RobloxLocaleId",
    "SystemLocaleId",
    "ViewportDisplaySize",
    "TopbarInset",
];

/// Classes whose unsaved properties are session, place or Studio state:
/// the camera follows the player, Workspace and Players hold what the
/// place and the server set, Stats counts what's running, and the Studio
/// services hold the user's Studio settings.
const SESSION_CLASSES: &[&str] = &[
    "Camera",
    "Workspace",
    "Players",
    "Stats",
    "StudioService",
    "StudioTestService",
    "DraggerService",
    "Selection",
];

/// Views Studio computes from other properties (mass and extents from the
/// size, axes and orientations from a CFrame, legacy names of renamed
/// properties). A fresh instance's value is wrong once those change, so
/// they need views (rook-lua's `api::properties`), not defaults. Every
/// unsaved BrickColor property is such a view of a Color3 one too.
const VIEWS: &[&str] = &[
    "Mass",
    "AssemblyMass",
    "CenterOfMass",
    "AssemblyCenterOfMass",
    "ExtentsSize",
    "ResizeIncrement",
    "MeshSize",
    "Elasticity",
    "Friction",
    "SpecificGravity",
    "Axis",
    "SecondaryAxis",
    "PrimaryAxis",
    "WorldAxis",
    "WorldSecondaryAxis",
    "WorldRotationAxis",
    "Orientation",
    "Rotation",
    "WorldOrientation",
    "WorldRotation",
    "AttachmentForward",
    "AttachmentRight",
    "AttachmentUp",
    "GripForward",
    "GripRight",
    "GripUp",
    "ContentText",
    "AbsoluteCellSize",
    "ClockTime",
    "Pitch",
    "MaxDistance",
    "MinDistance",
    "IsPaused",
    "maxHealth",
    "DiagonalFieldOfView",
    "MaxAxisFieldOfView",
    "SizeInCells",
    "IsSmooth",
    "ResizeableFaces",
    "CurrentPhysicalProperties",
];

/// Sparkles' `Color` is a view of `SparkleColor`, and a script's `Enabled`
/// the opposite of `Disabled`.
const CLASS_VIEWS: &[(&str, &str)] = &[("Sparkles", "Color"), ("BaseScript", "Enabled")];

pub(super) fn write(
    db: &Database,
    gaps: &[Gap],
    edit: &Reading,
    client: &Reading,
    again: &Reading,
    out: &Path,
) -> anyhow::Result<()> {
    // (declaring class, property) -> every probed class's value as Rust source.
    let mut groups: BTreeMap<(&str, &str), Vec<(&str, String)>> = BTreeMap::new();
    let mut unsupported: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut conflicts = Vec::new();
    let mut moving = Vec::new();
    for gap in gaps {
        let class = db.class(gap.class).name;
        if SESSION_CLASSES.contains(&class) {
            continue;
        }
        for &p in &gap.properties {
            let prop = db.property(p);
            let owner = db.class(prop.owner).name;
            // Legacy names the database migrates to newer properties are
            // views of those (rook-lua's `content_view` for content).
            let migrated = matches!(
                prop.kind,
                PropertyKind::Canonical(Serialization::Migrate { .. })
            );
            if migrated
                || SESSION.contains(&prop.name)
                || VIEWS.contains(&prop.name)
                || CLASS_VIEWS.contains(&(owner, prop.name))
                || matches!(prop.ty, PropertyType::Value(T::Ref | T::BrickColor))
            {
                continue;
            }
            let key = (gap.class, p);
            let source = |r: &Reading| rust(db, prop.ty, r.values.get(&key)?);
            let (e, c) = (source(edit), source(client));
            if let Some(raw) = client.values.get(&key).or(edit.values.get(&key))
                && c.is_none()
                && e.is_none()
            {
                unsupported
                    .entry(raw[0].clone())
                    .or_default()
                    .push(format!("{class}.{}", prop.name));
            }
            if c.is_some() && c != source(again) {
                moving.push(format!("{class}.{}", prop.name));
                continue;
            }
            if let (Some(e), Some(c)) = (&e, &c)
                && e != c
            {
                conflicts.push(format!("{class}.{}: Edit {e}, Client {c}", prop.name));
            }
            // A running game sees the client's value; services the client
            // lacks keep Edit's.
            let Some(value) = c.or(e) else { continue };
            groups
                .entry((owner, prop.name))
                .or_default()
                .push((class, value));
        }
    }

    // class -> property -> value, under the declaring class when several
    // classes have the property and all agree.
    let mut table: BTreeMap<&str, BTreeMap<&str, String>> = BTreeMap::new();
    for ((owner, property), values) in &groups {
        let zero = zero(db, owner, property);
        if values.len() > 1 && values.iter().all(|(_, v)| *v == values[0].1) {
            if Some(&values[0].1) != zero.as_ref() {
                table
                    .entry(owner)
                    .or_default()
                    .insert(property, values[0].1.clone());
            }
            continue;
        }
        for (class, v) in values {
            if Some(v) != zero.as_ref() {
                table.entry(class).or_default().insert(property, v.clone());
            }
        }
    }

    let count: usize = table.values().map(BTreeMap::len).sum();
    std::fs::write(out, source(&edit.version, &table))
        .with_context(|| format!("writing {}", out.display()))?;
    let status = std::process::Command::new("rustfmt")
        .args(["--edition", "2024"])
        .arg(out)
        .status()
        .context("running rustfmt")?;
    anyhow::ensure!(status.success(), "rustfmt failed on {}", out.display());

    println!(
        "{count} defaults from Studio {} -> {}",
        edit.version,
        out.display()
    );
    println!(
        "\nEdit and Client differ ({}; Client kept):",
        conflicts.len()
    );
    for c in &conflicts {
        println!("  {c}");
    }
    println!(
        "\nChanged between two client reads ({}; skipped):",
        moving.len()
    );
    for m in &moving {
        println!("  {m}");
    }
    println!("\nTypes Rook can't store yet (skipped):");
    for (ty, names) in &unsupported {
        println!("  {ty}: {}", names.join(", "));
    }
    Ok(())
}

/// Rook's zero value for `owner.property`, as Rust source. A Content
/// property without one reads as no content, as an empty one does.
fn zero(db: &Database, owner: &str, property: &str) -> Option<String> {
    let class = db.class_by_name(owner)?;
    let prop = db.find_property(class.id, property)?;
    let v = match prop.ty {
        PropertyType::Value(T::Content) => rook_types::Value::Content(rook_types::Content::none()),
        ty => rook_dom::zero_value(db, ty)?,
    };
    Some(value_source(&v))
}

/// A probe reading as Rust source for a Rook value of the property's type,
/// or None when Rook can't store it.
fn rust(db: &Database, ty: PropertyType, raw: &Raw) -> Option<String> {
    use rook_types::{Color3, UDim, UDim2, Value, Vector2, Vector3};
    let n = |i: usize| -> Option<f64> {
        let s = raw.get(i)?;
        if s.contains("nan") {
            return Some(f64::NAN);
        }
        s.parse().ok()
    };
    let f = |i: usize| n(i).map(|x| x as f32);
    let v = match (ty, raw[0].as_str()) {
        (PropertyType::Enum(e), "EnumItem") => {
            let value: u32 = raw[1].parse().ok()?;
            db.enum_info(e).item_name(value)?;
            Value::Enum(value)
        }
        (PropertyType::Value(t), tag) => match (t, tag) {
            (T::Bool, "boolean") => Value::Bool(raw[1] == "true"),
            (T::Float32, "number") => Value::Float32(f(1)?),
            (T::Float64, "number") => Value::Float64(n(1)?),
            (T::Int32, "number") => Value::Int32(n(1)? as i32),
            (T::Int64, "number") => Value::Int64(n(1)? as i64),
            (T::String, "string") => Value::String(raw[1].clone()),
            (T::ContentId, "string") => Value::ContentId(raw[1].clone().into()),
            (T::Vector3, "Vector3") => Value::Vector3(Vector3::new(f(1)?, f(2)?, f(3)?)),
            (T::Vector2, "Vector2") => Value::Vector2(Vector2::new(f(1)?, f(2)?)),
            (T::Color3, "Color3") => Value::Color3(Color3::new(f(1)?, f(2)?, f(3)?)),
            (T::UDim, "UDim") => Value::UDim(UDim::new(f(1)?, n(2)? as i32)),
            (T::UDim2, "UDim2") => {
                Value::UDim2(UDim2::new(f(1)?, n(2)? as i32, f(3)?, n(4)? as i32))
            }
            (T::CFrame, "CFrame") => {
                let mut c = [0.0; 12];
                for (i, x) in c.iter_mut().enumerate() {
                    *x = f(i + 1)?;
                }
                Value::CFrame(rook_types::CFrame::from_components(c))
            }
            (T::Content, "Content") => Value::Content(match raw[1].as_str() {
                "Uri" => rook_types::Content::from_uri(raw.get(2)?.clone()),
                "None" => rook_types::Content::none(),
                _ => return None,
            }),
            (T::NumberRange, "NumberRange") => {
                Value::NumberRange(rbx_types::NumberRange::new(f(1)?, f(2)?))
            }
            _ => return None,
        },
        _ => return None,
    };
    Some(value_source(&v))
}

fn float<F: std::fmt::Debug + Into<f64> + Copy>(x: F, ty: &str) -> String {
    let wide: f64 = x.into();
    if wide.is_nan() {
        format!("{ty}::NAN")
    } else if wide.is_infinite() {
        format!("{ty}::{}INFINITY", if wide < 0.0 { "NEG_" } else { "" })
    } else {
        format!("{x:?}")
    }
}

/// `v` as a Rust expression.
fn value_source(v: &rook_types::Value) -> String {
    use rook_types::Value;
    let f = |x: f32| float(x, "f32");
    match v {
        Value::Bool(b) => format!("Value::Bool({b})"),
        Value::Float32(x) => format!("Value::Float32({})", f(*x)),
        Value::Float64(x) => format!("Value::Float64({})", float(*x, "f64")),
        Value::Int32(x) => format!("Value::Int32({x})"),
        Value::Int64(x) => format!("Value::Int64({x})"),
        Value::Enum(x) => format!("Value::Enum({x})"),
        Value::String(s) => format!("Value::String({s:?}.into())"),
        Value::ContentId(c) => format!("Value::ContentId({:?}.into())", c.as_str()),
        Value::Vector3(p) => format!(
            "Value::Vector3(rook_types::Vector3::new({}, {}, {}))",
            f(p.0.x),
            f(p.0.y),
            f(p.0.z)
        ),
        Value::Vector2(p) => format!(
            "Value::Vector2(rook_types::Vector2::new({}, {}))",
            f(p.0.x),
            f(p.0.y)
        ),
        Value::Color3(c) => format!(
            "Value::Color3(rook_types::Color3::new({}, {}, {}))",
            f(c.r),
            f(c.g),
            f(c.b)
        ),
        Value::UDim(u) => format!(
            "Value::UDim(rook_types::UDim::new({}, {}))",
            f(u.scale),
            u.offset
        ),
        Value::UDim2(u) => format!(
            "Value::UDim2(rook_types::UDim2::new({}, {}, {}, {}))",
            f(u.x.scale),
            u.x.offset,
            f(u.y.scale),
            u.y.offset
        ),
        Value::CFrame(c) => {
            let parts: Vec<String> = c.components().iter().map(|x| f(*x)).collect();
            format!(
                "Value::CFrame(rook_types::CFrame::from_components([{}]))",
                parts.join(", ")
            )
        }
        Value::Content(c) => match c.as_uri() {
            Some(uri) => format!("Value::Content(rook_types::Content::from_uri({uri:?}))"),
            None => "Value::Content(rook_types::Content::none())".to_owned(),
        },
        Value::NumberRange(r) => format!(
            "Value::NumberRange(rbx_types::NumberRange::new({}, {}))",
            f(r.min),
            f(r.max)
        ),
        Value::Ref(None) => "Value::Ref(None)".to_owned(),
        other => unreachable!("the probe makes no {other:?}"),
    }
}

fn source(version: &str, table: &BTreeMap<&str, BTreeMap<&str, String>>) -> String {
    let mut s = format!(
        "//! Property defaults measured in Studio {version} for properties the\n\
         //! reflection database has none for (it lists only ones that save),\n\
         //! where they differ from the zero value. Keyed by class, or by the\n\
         //! declaring class when several classes have it and all agree. See\n\
         //! docs/behavior/property-defaults.md.\n\
         //!\n\
         //! Generated by `rook-tools studio-defaults`; don't edit by hand.\n\n\
         use rook_types::Value;\n\n\
         /// Studio's default for `class.property` in a running game.\n\
         pub(super) fn measured(class: &str, property: &str) -> Option<Value> {{\n\
         Some(match class {{\n"
    );
    // Arms with the same value share one, as Clippy asks.
    let mut classes: Vec<(Vec<&str>, String)> = Vec::new();
    for (class, props) in table {
        let mut body = String::from("match property {\n");
        for (names, value) in merged(props.iter().map(|(p, v)| (*p, v.as_str()))) {
            let _ = writeln!(body, "{} => {value},", patterns(&names));
        }
        body.push_str("_ => return None,\n}");
        match classes.iter_mut().find(|(_, b)| *b == body) {
            Some((names, _)) => names.push(class),
            None => classes.push((vec![class], body)),
        }
    }
    for (names, body) in &classes {
        let _ = writeln!(s, "{} => {body},", patterns(names));
    }
    s.push_str("_ => return None,\n})\n}\n");
    s
}

/// `(name, value)` pairs grouped by value, in first-seen order.
fn merged<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<(Vec<&'a str>, &'a str)> {
    let mut out: Vec<(Vec<&str>, &str)> = Vec::new();
    for (name, value) in pairs {
        match out.iter_mut().find(|(_, v)| *v == value) {
            Some((names, _)) => names.push(name),
            None => out.push((vec![name], value)),
        }
    }
    out
}

fn patterns(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("{n:?}")).collect();
    quoted.join(" | ")
}
