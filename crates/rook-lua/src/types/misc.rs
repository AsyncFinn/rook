//! Random, TweenInfo, Faces, Axes, PhysicalProperties, and the opaque
//! wrapper for value types without a dedicated Lua representation yet.

use super::err;
use mlua::{
    Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue, Vector,
};
use rook_types::{NormalId, Value};

use super::enums::{item_name, item_value};
use crate::ctx::args::number_of;

/// A property value Rook can't yet show as a proper datatype. It still
/// round-trips: reading a property and assigning it back works.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Opaque(pub Value);

impl Opaque {
    fn type_name(&self) -> &'static str {
        match &self.0 {
            Value::Faces(_) => "Faces",
            Value::Axes(_) => "Axes",
            Value::PhysicalProperties(_) => "PhysicalProperties",
            Value::Font(_) => "Font",
            Value::Content(_) => "Content",
            _ => "userdata",
        }
    }
}

impl UserData for Opaque {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(
            MetaMethod::Eq,
            |_, t, o: UserDataRef<Opaque>| Ok(t.0 == o.0),
        );
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(match &t.0 {
                // Observed: the six values as `%.9g`, comma-separated.
                Value::PhysicalProperties(rbx_types::PhysicalProperties::Custom(p)) => [
                    p.density(),
                    p.friction(),
                    p.elasticity(),
                    p.friction_weight(),
                    p.elasticity_weight(),
                    p.acoustic_absorption(),
                ]
                .map(super::fmt_f32)
                .join(", "),
                _ => t.type_name().to_owned(),
            })
        });
        m.add_meta_method(MetaMethod::Type, |_, t, ()| Ok(t.type_name()));
        m.add_meta_method(MetaMethod::Index, |_, t, key: String| {
            if let Value::PhysicalProperties(rbx_types::PhysicalProperties::Custom(p)) = &t.0 {
                let v = match key.as_str() {
                    "Density" => p.density(),
                    "Friction" => p.friction(),
                    "Elasticity" => p.elasticity(),
                    "FrictionWeight" => p.friction_weight(),
                    "ElasticityWeight" => p.elasticity_weight(),
                    "AcousticAbsorption" => p.acoustic_absorption(),
                    _ => {
                        return Err(err(format!(
                            "{key} is not a valid member of PhysicalProperties"
                        )));
                    }
                };
                return Ok(LuaValue::Number(f64::from(v)));
            }
            Err(err(format!(
                "{key} is not a valid member of {}",
                t.type_name()
            )))
        });
    }
}

// Random

/// `Random`, a PCG32 generator. Roblox documents `Random` as PCG-based but
/// not its seeding, so sequences for a given seed differ (Approximation).
#[derive(Debug, Clone)]
pub(crate) struct LRandom {
    state: u64,
    inc: u64,
}

impl LRandom {
    pub(crate) fn new(seed: u64) -> Self {
        let mut r = Self {
            state: 0,
            inc: (0xda3e_39cb_94b9_5bdb << 1) | 1,
        };
        r.next_u32();
        r.state = r.state.wrapping_add(seed);
        r.next_u32();
        r
    }

    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Uniform in [0, 1).
    pub(crate) fn next_f64(&mut self) -> f64 {
        let hi = u64::from(self.next_u32()) << 21;
        let lo = u64::from(self.next_u32()) >> 11;
        ((hi | lo) as f64) / ((1u64 << 53) as f64)
    }
}

impl UserData for LRandom {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Random");
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method_mut("NextNumber", |_, t, (a, b): (Option<f64>, Option<f64>)| {
            let (a, b) = (a.unwrap_or(0.0), b.unwrap_or(1.0));
            Ok(a + (b - a) * t.next_f64())
        });
        m.add_method_mut("NextInteger", |_, t, (a, b): (f64, f64)| {
            let (lo, hi) = (a.min(b) as i64, a.max(b) as i64);
            let span = (hi - lo + 1) as f64;
            Ok(lo + (t.next_f64() * span) as i64)
        });
        m.add_method_mut("NextUnitVector", |_, t, ()| {
            let z = 2.0 * t.next_f64() - 1.0;
            let a = std::f64::consts::TAU * t.next_f64();
            let r = (1.0 - z * z).sqrt();
            Ok(Vector::new(
                (r * a.cos()) as f32,
                (r * a.sin()) as f32,
                z as f32,
            ))
        });
        m.add_method_mut("Shuffle", |_, t, tb: mlua::Table| {
            let n = tb.raw_len();
            for i in (2..=n).rev() {
                let j = 1 + (t.next_f64() * i as f64) as usize;
                let (a, b): (LuaValue, LuaValue) = (tb.raw_get(i)?, tb.raw_get(j)?);
                tb.raw_set(i, b)?;
                tb.raw_set(j, a)?;
            }
            Ok(())
        });
        m.add_method("Clone", |_, t, ()| Ok(t.clone()));
    }
}

// TweenInfo

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LTweenInfo {
    pub(crate) time: f64,
    pub(crate) easing_style: u32,
    pub(crate) easing_direction: u32,
    pub(crate) repeat_count: i64,
    pub(crate) reverses: bool,
    pub(crate) delay_time: f64,
}

impl UserData for LTweenInfo {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "TweenInfo");
        f.add_field_method_get("Time", |_, t| Ok(t.time));
        f.add_field_method_get("EasingStyle", |lua, t| {
            super::enums::enum_item(lua, "EasingStyle", t.easing_style)
        });
        f.add_field_method_get("EasingDirection", |lua, t| {
            super::enums::enum_item(lua, "EasingDirection", t.easing_direction)
        });
        f.add_field_method_get("RepeatCount", |_, t| Ok(t.repeat_count));
        f.add_field_method_get("Reverses", |_, t| Ok(t.reverses));
        f.add_field_method_get("DelayTime", |_, t| Ok(t.delay_time));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LTweenInfo>| {
            Ok(*t == *o)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!(
                "Time:{} DelayTime:{} RepeatCount:{} Reverses:{} EasingDirection:{} EasingStyle:{}",
                t.time,
                t.delay_time,
                t.repeat_count,
                t.reverses,
                t.easing_direction,
                t.easing_style
            ))
        });
    }
}

/// The value of `Enum.<enum_name>.<item>`, an item the database has.
fn default_item(enum_name: &str, item: &str) -> u32 {
    rook_reflection::database()
        .enum_by_name(enum_name)
        .and_then(|e| e.item_value(item))
        .expect("enum item")
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Luau calls it through mlua, which wants a Result"
)]
fn tween_info_new(lua: &Lua, args: MultiValue) -> mlua::Result<LTweenInfo> {
    let a = args.into_vec();
    let get = |i: usize| a.get(i).cloned().unwrap_or(LuaValue::Nil);
    Ok(LTweenInfo {
        time: number_of(&get(0)).unwrap_or(1.0),
        // EasingStyle.Quad and EasingDirection.Out are the documented defaults.
        easing_style: item_value(lua, &get(1), "EasingStyle")
            .unwrap_or_else(|| default_item("EasingStyle", "Quad")),
        easing_direction: item_value(lua, &get(2), "EasingDirection")
            .unwrap_or_else(|| default_item("EasingDirection", "Out")),
        repeat_count: number_of(&get(3)).unwrap_or(0.0) as i64,
        reverses: matches!(get(4), LuaValue::Boolean(true)),
        delay_time: number_of(&get(5)).unwrap_or(0.0),
    })
}

// Faces, Axes, PhysicalProperties

#[expect(
    clippy::unnecessary_wraps,
    reason = "Luau calls it through mlua, which wants a Result"
)]
fn faces_new(lua: &Lua, args: MultiValue) -> mlua::Result<Opaque> {
    use rbx_types::Faces;
    let mut faces = Faces::empty();
    for v in args {
        let flag = match item_value(lua, &v, "NormalId").and_then(NormalId::from_u32) {
            Some(NormalId::Right) => Faces::RIGHT,
            Some(NormalId::Top) => Faces::TOP,
            Some(NormalId::Back) => Faces::BACK,
            Some(NormalId::Left) => Faces::LEFT,
            Some(NormalId::Bottom) => Faces::BOTTOM,
            Some(NormalId::Front) => Faces::FRONT,
            None => continue,
        };
        faces = Faces::from_bits(faces.bits() | flag.bits()).unwrap_or(faces);
    }
    Ok(Opaque(Value::Faces(faces)))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Luau calls it through mlua, which wants a Result"
)]
fn axes_new(lua: &Lua, args: MultiValue) -> mlua::Result<Opaque> {
    use rbx_types::Axes;
    let mut axes = Axes::empty();
    for v in args {
        let flag = match item_name(lua, &v, "Axis") {
            Some("X") => Axes::X,
            Some("Y") => Axes::Y,
            Some("Z") => Axes::Z,
            _ => match item_value(lua, &v, "NormalId").and_then(NormalId::from_u32) {
                Some(NormalId::Right | NormalId::Left) => Axes::X,
                Some(NormalId::Top | NormalId::Bottom) => Axes::Y,
                Some(NormalId::Back | NormalId::Front) => Axes::Z,
                None => continue,
            },
        };
        axes = Axes::from_bits(axes.bits() | flag.bits()).unwrap_or(axes);
    }
    Ok(Opaque(Value::Axes(axes)))
}

/// `PhysicalProperties.new(material)` or `(density, friction, elasticity
/// [, frictionWeight, elasticityWeight [, acousticAbsorption]])`, with the
/// values clamped (observed).
fn physical_properties_new(lua: &Lua, args: MultiValue) -> mlua::Result<Opaque> {
    use rook_physics::contacts::materials::{self, Properties};
    let a = args.into_vec();
    let p = if a.len() == 1
        && let Some(material) = item_value(lua, &a[0], "Material")
    {
        materials::of_material(material)
    } else {
        if !matches!(a.len(), 3 | 5 | 6) {
            return Err(err(format!(
                "Invalid number of arguments: {}. Four ways to call PhysicalProperties.new:\n\
                 \t\t\t\t\t1. new(material: Material)  -- returns default properties for the given material\n\
                 \t\t\t\t\t2. new(density: number, friction: number, elasticity: number)\n\
                 \t\t\t\t\t3. new(density: number, friction: number, elasticity: number, frictionWeight: number, elasticityWeight: number)\n\
                 \t\t\t\t\t4. new(density: number, friction: number, elasticity: number, frictionWeight: number, elasticityWeight: number, acousticAbsorption: number)\n",
                a.len()
            )));
        }
        let n = |i: usize, d: f32| a.get(i).and_then(number_of).map_or(d, |v| v as f32);
        Properties {
            density: n(0, 0.7),
            friction: n(1, 0.3),
            elasticity: n(2, 0.5),
            friction_weight: n(3, 1.0),
            elasticity_weight: n(4, 1.0),
            acoustic_absorption: n(5, 1.0),
        }
        .clamped()
    };
    let custom = rbx_types::CustomPhysicalProperties::new(
        p.density,
        p.friction,
        p.elasticity,
        p.friction_weight,
        p.elasticity_weight,
        p.acoustic_absorption,
    );
    Ok(Opaque(Value::PhysicalProperties(
        rbx_types::PhysicalProperties::Custom(custom),
    )))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    let random = lua.create_table()?;
    random.set(
        "new",
        lua.create_function(|_, seed: Option<f64>| {
            let seed = seed.map_or_else(
                || {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_nanos() as u64)
                },
                |s| s as i64 as u64,
            );
            Ok(LRandom::new(seed))
        })?,
    )?;
    g.set("Random", random)?;

    let ti = lua.create_table()?;
    ti.set("new", lua.create_function(tween_info_new)?)?;
    g.set("TweenInfo", ti)?;

    let faces = lua.create_table()?;
    faces.set("new", lua.create_function(faces_new)?)?;
    g.set("Faces", faces)?;
    let axes = lua.create_table()?;
    axes.set("new", lua.create_function(axes_new)?)?;
    g.set("Axes", axes)?;
    let pp = lua.create_table()?;
    pp.set("new", lua.create_function(physical_properties_new)?)?;
    g.set("PhysicalProperties", pp)?;
    Ok(())
}
