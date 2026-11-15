//! Faces, Axes and PhysicalProperties, kept as Rook's own values behind
//! one userdata type per kind, and the opaque wrapper for values scripts
//! have no datatype for.

use crate::types::err;
use crate::types::members::{Wording, not_a_member};
use mlua::{
    IntoLua, Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue,
};
use rook_types::{NormalId, Value};

use crate::runtime::ctx::args::number_of;
use crate::types::enums::{item_name, item_value};

/// A property value with no Lua datatype of its own, as one of the kinds
/// below. Each kind is its own userdata type, so `typeof` names it as
/// Roblox does. It still round-trips: reading a property and assigning it
/// back works.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Opaque<const KIND: u8>(pub Value);

const FACES: u8 = 0;
const AXES: u8 = 1;
const PHYSICAL_PROPERTIES: u8 = 2;
/// A value scripts have no datatype for.
const OTHER: u8 = 3;

pub(crate) type LFaces = Opaque<FACES>;
pub(crate) type LAxes = Opaque<AXES>;
pub(crate) type LPhysicalProperties = Opaque<PHYSICAL_PROPERTIES>;
pub(crate) type LOther = Opaque<OTHER>;

impl<const KIND: u8> Opaque<KIND> {
    const NAME: &'static str = match KIND {
        FACES => "Faces",
        AXES => "Axes",
        PHYSICAL_PROPERTIES => "PhysicalProperties",
        _ => "userdata",
    };
}

/// `v` as the userdata for its kind.
pub(crate) fn opaque(lua: &Lua, v: Value) -> mlua::Result<LuaValue> {
    match v {
        Value::Faces(_) => Opaque::<FACES>(v).into_lua(lua),
        Value::Axes(_) => Opaque::<AXES>(v).into_lua(lua),
        Value::PhysicalProperties(_) => Opaque::<PHYSICAL_PROPERTIES>(v).into_lua(lua),
        _ => Opaque::<OTHER>(v).into_lua(lua),
    }
}

impl<const KIND: u8> UserData for Opaque<KIND> {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", Self::NAME);
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<Self>| Ok(t.0 == o.0));
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
                .map(crate::types::fmt_f32)
                .join(", "),
                // Observed: the members set, in this order.
                Value::Faces(f) => {
                    use rbx_types::Faces as F;
                    let all = [
                        (F::RIGHT, "Right"),
                        (F::TOP, "Top"),
                        (F::BACK, "Back"),
                        (F::LEFT, "Left"),
                        (F::BOTTOM, "Bottom"),
                        (F::FRONT, "Front"),
                    ];
                    let set = all.iter().filter(|(flag, _)| f.contains(*flag));
                    set.map(|(_, name)| *name).collect::<Vec<_>>().join(", ")
                }
                Value::Axes(a) => {
                    use rbx_types::Axes as A;
                    let all = [(A::X, "X"), (A::Y, "Y"), (A::Z, "Z")];
                    let set = all.iter().filter(|(flag, _)| a.contains(*flag));
                    set.map(|(_, name)| *name).collect::<Vec<_>>().join(", ")
                }
                _ => Self::NAME.to_owned(),
            })
        });
        m.add_meta_method(MetaMethod::Index, |_, t, key: LuaValue| {
            let unknown = || not_a_member(&key, Self::NAME, Wording::Quoted);
            let LuaValue::String(name) = &key else {
                return Err(unknown());
            };
            if let Value::PhysicalProperties(rbx_types::PhysicalProperties::Custom(p)) = &t.0 {
                let v = match &*name.to_string_lossy() {
                    "Density" => p.density(),
                    "Friction" => p.friction(),
                    "Elasticity" => p.elasticity(),
                    "FrictionWeight" => p.friction_weight(),
                    "ElasticityWeight" => p.elasticity_weight(),
                    "AcousticAbsorption" => p.acoustic_absorption(),
                    _ => return Err(unknown()),
                };
                return Ok(LuaValue::Number(f64::from(v)));
            }
            Err(unknown())
        });
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Luau calls it through mlua, which wants a Result"
)]
fn faces_new(lua: &Lua, args: MultiValue) -> mlua::Result<LFaces> {
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
    Ok(Opaque::<FACES>(Value::Faces(faces)))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Luau calls it through mlua, which wants a Result"
)]
fn axes_new(lua: &Lua, args: MultiValue) -> mlua::Result<LAxes> {
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
    Ok(Opaque::<AXES>(Value::Axes(axes)))
}

/// `PhysicalProperties.new(material)` or `(density, friction, elasticity
/// [, frictionWeight, elasticityWeight [, acousticAbsorption]])`, with the
/// values clamped (observed).
fn physical_properties_new(lua: &Lua, args: MultiValue) -> mlua::Result<LPhysicalProperties> {
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
    Ok(Opaque::<PHYSICAL_PROPERTIES>(Value::PhysicalProperties(
        rbx_types::PhysicalProperties::Custom(custom),
    )))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
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
