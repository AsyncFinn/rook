//! Vector3 (native `vector`), Vector2, Vector3int16, Vector2int16.

use mlua::{
    Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, UserDataRef, Value as LuaValue,
    Vector,
};
use rook_types::{Vector2, Vector3};

use super::fmt_f32;
use crate::args::number_of;

use crate::convert::{from_lua_vector as v3, lua_vector as lua_v3};

fn map3(v: Vector, f: impl Fn(f32) -> f32) -> Vector {
    Vector::new(f(v.x()), f(v.y()), f(v.z()))
}

/// Roblox's `math.sign`-style sign: 0 stays 0.
fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn angle(a: Vector3, b: Vector3) -> f32 {
    let d = a.unit().dot(b.unit()).clamp(-1.0, 1.0);
    d.acos()
}

/// Members of Vector3 that aren't components (those are native).
fn vector3_index(lua: &Lua, (v, key): (Vector, String)) -> mlua::Result<LuaValue> {
    let a = v3(v);
    let f = |lua: &Lua, func: mlua::Function| -> mlua::Result<LuaValue> {
        let _ = lua;
        Ok(LuaValue::Function(func))
    };
    match key.as_str() {
        "X" | "x" => Ok(LuaValue::Number(f64::from(a.x()))),
        "Y" | "y" => Ok(LuaValue::Number(f64::from(a.y()))),
        "Z" | "z" => Ok(LuaValue::Number(f64::from(a.z()))),
        "Magnitude" | "magnitude" => Ok(LuaValue::Number(f64::from(a.magnitude()))),
        "Unit" | "unit" => Ok(LuaValue::Vector(lua_v3(a.unit()))),
        name => {
            let methods: mlua::Table = lua.named_registry_value("rook.vector3_methods")?;
            match methods.get::<Option<mlua::Function>>(name)? {
                Some(func) => f(lua, func),
                None => Err(mlua::Error::RuntimeError(format!(
                    "{name} is not a valid member of Vector3"
                ))),
            }
        }
    }
}

fn install_vector3(lua: &Lua) -> mlua::Result<()> {
    let methods = lua.create_table()?;
    methods.set(
        "Dot",
        lua.create_function(|_, (a, b): (Vector, Vector)| Ok(v3(a).dot(v3(b))))?,
    )?;
    methods.set(
        "Cross",
        lua.create_function(|_, (a, b): (Vector, Vector)| Ok(lua_v3(v3(a).cross(v3(b)))))?,
    )?;
    let lerp = lua
        .create_function(|_, (a, b, t): (Vector, Vector, f32)| Ok(lua_v3(v3(a).lerp(v3(b), t))))?;
    methods.set("Lerp", lerp.clone())?;
    methods.set("lerp", lerp)?;
    let fuzzy = lua.create_function(|_, (a, b, eps): (Vector, Vector, Option<f32>)| {
        Ok(v3(a).fuzzy_eq(v3(b), eps.unwrap_or(1e-5)))
    })?;
    methods.set("FuzzyEq", fuzzy.clone())?;
    methods.set("isClose", fuzzy)?;
    methods.set(
        "Max",
        lua.create_function(|_, (a, rest): (Vector, mlua::Variadic<Vector>)| {
            Ok(lua_v3(rest.iter().fold(v3(a), |m, b| m.max(v3(*b)))))
        })?,
    )?;
    methods.set(
        "Min",
        lua.create_function(|_, (a, rest): (Vector, mlua::Variadic<Vector>)| {
            Ok(lua_v3(rest.iter().fold(v3(a), |m, b| m.min(v3(*b)))))
        })?,
    )?;
    methods.set(
        "Abs",
        lua.create_function(|_, a: Vector| Ok(map3(a, f32::abs)))?,
    )?;
    methods.set(
        "Ceil",
        lua.create_function(|_, a: Vector| Ok(map3(a, f32::ceil)))?,
    )?;
    methods.set(
        "Floor",
        lua.create_function(|_, a: Vector| Ok(map3(a, f32::floor)))?,
    )?;
    methods.set(
        "Sign",
        lua.create_function(|_, a: Vector| Ok(map3(a, sign)))?,
    )?;
    methods.set(
        "Angle",
        lua.create_function(|_, (a, b, axis): (Vector, Vector, Option<Vector>)| {
            let (a, b) = (v3(a), v3(b));
            let mut ang = angle(a, b);
            if let Some(axis) = axis
                && a.cross(b).dot(v3(axis)) < 0.0
            {
                ang = -ang;
            }
            Ok(ang)
        })?,
    )?;
    lua.set_named_registry_value("rook.vector3_methods", methods)?;

    let mt = lua.create_table()?;
    mt.set("__index", lua.create_function(vector3_index)?)?;
    mt.set("__type", "Vector3")?;
    lua.set_type_metatable::<Vector>(Some(mt));

    let ctor = lua.create_table()?;
    ctor.set(
        "new",
        lua.create_function(|_, (x, y, z): (Option<f32>, Option<f32>, Option<f32>)| {
            Ok(Vector::new(
                x.unwrap_or(0.0),
                y.unwrap_or(0.0),
                z.unwrap_or(0.0),
            ))
        })?,
    )?;
    ctor.set("zero", Vector::new(0.0, 0.0, 0.0))?;
    ctor.set("one", Vector::new(1.0, 1.0, 1.0))?;
    ctor.set("xAxis", Vector::new(1.0, 0.0, 0.0))?;
    ctor.set("yAxis", Vector::new(0.0, 1.0, 0.0))?;
    ctor.set("zAxis", Vector::new(0.0, 0.0, 1.0))?;
    ctor.set(
        "FromNormalId",
        lua.create_function(|lua, item: LuaValue| {
            let v = super::enums::item_value(lua, &item, "NormalId")
                .ok_or_else(|| mlua::Error::RuntimeError("Unable to cast to NormalId".into()))?;
            Ok(normal_id_vector(v))
        })?,
    )?;
    ctor.set(
        "FromAxis",
        lua.create_function(|lua, item: LuaValue| {
            let v = super::enums::item_value(lua, &item, "Axis")
                .ok_or_else(|| mlua::Error::RuntimeError("Unable to cast to Axis".into()))?;
            Ok(match v {
                0 => Vector::new(1.0, 0.0, 0.0),
                1 => Vector::new(0.0, 1.0, 0.0),
                _ => Vector::new(0.0, 0.0, 1.0),
            })
        })?,
    )?;
    lua.globals().set("Vector3", ctor)?;
    Ok(())
}

/// `Enum.NormalId` values: Right 0, Top 1, Back 2, Left 3, Bottom 4, Front 5.
pub(crate) fn normal_id_vector(v: u32) -> Vector {
    match v {
        0 => Vector::new(1.0, 0.0, 0.0),
        1 => Vector::new(0.0, 1.0, 0.0),
        2 => Vector::new(0.0, 0.0, 1.0),
        3 => Vector::new(-1.0, 0.0, 0.0),
        4 => Vector::new(0.0, -1.0, 0.0),
        _ => Vector::new(0.0, 0.0, -1.0),
    }
}

// ---- Vector2 ----------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LVector2(pub Vector2);

fn mul2(a: &LuaValue, b: &LuaValue, div: bool) -> mlua::Result<LVector2> {
    let as_v = |v: &LuaValue| match v {
        LuaValue::UserData(u) => u.borrow::<LVector2>().ok().map(|x| x.0),
        _ => None,
    };
    let op = |x: Vector2, y: Vector2| if div { x / y } else { x * y };
    let res = match (as_v(a), as_v(b), number_of(a), number_of(b)) {
        (Some(x), Some(y), _, _) => op(x, y),
        (Some(x), None, _, Some(n)) => op(x, Vector2::new(n as f32, n as f32)),
        (None, Some(y), Some(n), _) => op(Vector2::new(n as f32, n as f32), y),
        _ => {
            return Err(mlua::Error::RuntimeError(
                "invalid argument #2 (Vector2 expected)".into(),
            ));
        }
    };
    Ok(LVector2(res))
}

impl UserData for LVector2 {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Vector2");
        f.add_field_method_get("X", |_, t| Ok(t.0.x()));
        f.add_field_method_get("Y", |_, t| Ok(t.0.y()));
        f.add_field_method_get("x", |_, t| Ok(t.0.x()));
        f.add_field_method_get("y", |_, t| Ok(t.0.y()));
        f.add_field_method_get("Magnitude", |_, t| Ok(t.0.magnitude()));
        f.add_field_method_get("magnitude", |_, t| Ok(t.0.magnitude()));
        f.add_field_method_get("Unit", |_, t| Ok(LVector2(t.0.unit())));
        f.add_field_method_get("unit", |_, t| Ok(LVector2(t.0.unit())));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("Dot", |_, t, o: UserDataRef<LVector2>| Ok(t.0.dot(o.0)));
        m.add_method("Cross", |_, t, o: UserDataRef<LVector2>| Ok(t.0.cross(o.0)));
        for name in ["Lerp", "lerp"] {
            m.add_method(name, |_, t, (o, a): (UserDataRef<LVector2>, f32)| {
                Ok(LVector2(t.0.lerp(o.0, a)))
            });
        }
        m.add_method(
            "FuzzyEq",
            |_, t, (o, e): (UserDataRef<LVector2>, Option<f32>)| {
                Ok((t.0.0 - o.0.0).length() <= e.unwrap_or(1e-5))
            },
        );
        m.add_method(
            "Max",
            |_, t, rest: mlua::Variadic<UserDataRef<LVector2>>| {
                Ok(LVector2(Vector2(
                    rest.iter().fold(t.0.0, |m, b| m.max(b.0.0)),
                )))
            },
        );
        m.add_method(
            "Min",
            |_, t, rest: mlua::Variadic<UserDataRef<LVector2>>| {
                Ok(LVector2(Vector2(
                    rest.iter().fold(t.0.0, |m, b| m.min(b.0.0)),
                )))
            },
        );
        m.add_method("Abs", |_, t, ()| Ok(LVector2(Vector2(t.0.0.abs()))));
        m.add_method("Ceil", |_, t, ()| Ok(LVector2(Vector2(t.0.0.ceil()))));
        m.add_method("Floor", |_, t, ()| Ok(LVector2(Vector2(t.0.0.floor()))));
        m.add_method("Sign", |_, t, ()| {
            Ok(LVector2(Vector2::new(sign(t.0.x()), sign(t.0.y()))))
        });
        m.add_method(
            "Angle",
            |_, t, (o, signed): (UserDataRef<LVector2>, Option<bool>)| {
                let a = t.0.unit();
                let b = o.0.unit();
                let ang = a.dot(b).clamp(-1.0, 1.0).acos();
                Ok(if signed.unwrap_or(false) && a.cross(b) < 0.0 {
                    -ang
                } else {
                    ang
                })
            },
        );
        m.add_meta_method(MetaMethod::Add, |_, t, o: UserDataRef<LVector2>| {
            Ok(LVector2(t.0 + o.0))
        });
        m.add_meta_method(MetaMethod::Sub, |_, t, o: UserDataRef<LVector2>| {
            Ok(LVector2(t.0 - o.0))
        });
        m.add_meta_function(MetaMethod::Mul, |_, (a, b): (LuaValue, LuaValue)| {
            mul2(&a, &b, false)
        });
        m.add_meta_function(MetaMethod::Div, |_, (a, b): (LuaValue, LuaValue)| {
            mul2(&a, &b, true)
        });
        m.add_meta_function(MetaMethod::IDiv, |_, (a, b): (LuaValue, LuaValue)| {
            let r = mul2(&a, &b, true)?;
            Ok(LVector2(Vector2(r.0.0.floor())))
        });
        m.add_meta_method(MetaMethod::Unm, |_, t, ()| Ok(LVector2(Vector2(-t.0.0))));
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LVector2>| {
            Ok(t.0 == o.0)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!("{}, {}", fmt_f32(t.0.x()), fmt_f32(t.0.y())))
        });
    }
}

// ---- int16 vectors -------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LVector3int16(pub rbx_types::Vector3int16);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LVector2int16(pub rbx_types::Vector2int16);

fn wrap16(x: f64) -> i16 {
    // Values wrap like a C cast through int.
    (x as i64) as i16
}

impl UserData for LVector3int16 {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Vector3int16");
        f.add_field_method_get("X", |_, t| Ok(t.0.x));
        f.add_field_method_get("Y", |_, t| Ok(t.0.y));
        f.add_field_method_get("Z", |_, t| Ok(t.0.z));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        let op = |f: fn(i16, i16) -> i16| {
            move |_: &Lua, t: &LVector3int16, o: UserDataRef<LVector3int16>| {
                Ok(LVector3int16(rbx_types::Vector3int16::new(
                    f(t.0.x, o.0.x),
                    f(t.0.y, o.0.y),
                    f(t.0.z, o.0.z),
                )))
            }
        };
        m.add_meta_method(MetaMethod::Add, op(i16::wrapping_add));
        m.add_meta_method(MetaMethod::Sub, op(i16::wrapping_sub));
        m.add_meta_method(MetaMethod::Mul, op(i16::wrapping_mul));
        m.add_meta_method(MetaMethod::Div, op(int16_div));
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LVector3int16>| {
            Ok(t.0 == o.0)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!("{}, {}, {}", t.0.x, t.0.y, t.0.z))
        });
    }
}

/// Integer division for Vector2int16 and Vector3int16; dividing by zero
/// gives 0 (Inference: Rook avoids the panic; Roblox's result is untested).
fn int16_div(a: i16, b: i16) -> i16 {
    a.checked_div(b).unwrap_or(0)
}

impl UserData for LVector2int16 {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Vector2int16");
        f.add_field_method_get("X", |_, t| Ok(t.0.x));
        f.add_field_method_get("Y", |_, t| Ok(t.0.y));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        let op = |f: fn(i16, i16) -> i16| {
            move |_: &Lua, t: &LVector2int16, o: UserDataRef<LVector2int16>| {
                Ok(LVector2int16(rbx_types::Vector2int16::new(
                    f(t.0.x, o.0.x),
                    f(t.0.y, o.0.y),
                )))
            }
        };
        m.add_meta_method(MetaMethod::Add, op(i16::wrapping_add));
        m.add_meta_method(MetaMethod::Sub, op(i16::wrapping_sub));
        m.add_meta_method(MetaMethod::Mul, op(i16::wrapping_mul));
        m.add_meta_method(MetaMethod::Div, op(int16_div));
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LVector2int16>| {
            Ok(t.0 == o.0)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!("{}, {}", t.0.x, t.0.y))
        });
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    install_vector3(lua)?;

    let v2 = lua.create_table()?;
    v2.set(
        "new",
        lua.create_function(|_, (x, y): (Option<f32>, Option<f32>)| {
            Ok(LVector2(Vector2::new(x.unwrap_or(0.0), y.unwrap_or(0.0))))
        })?,
    )?;
    v2.set("zero", LVector2(Vector2::ZERO))?;
    v2.set("one", LVector2(Vector2::ONE))?;
    v2.set("xAxis", LVector2(Vector2::new(1.0, 0.0)))?;
    v2.set("yAxis", LVector2(Vector2::new(0.0, 1.0)))?;
    lua.globals().set("Vector2", v2)?;

    let v3i = lua.create_table()?;
    v3i.set(
        "new",
        lua.create_function(|_, (x, y, z): (Option<f64>, Option<f64>, Option<f64>)| {
            Ok(LVector3int16(rbx_types::Vector3int16::new(
                wrap16(x.unwrap_or(0.0)),
                wrap16(y.unwrap_or(0.0)),
                wrap16(z.unwrap_or(0.0)),
            )))
        })?,
    )?;
    lua.globals().set("Vector3int16", v3i)?;

    let v2i = lua.create_table()?;
    v2i.set(
        "new",
        lua.create_function(|_, (x, y): (Option<f64>, Option<f64>)| {
            Ok(LVector2int16(rbx_types::Vector2int16::new(
                wrap16(x.unwrap_or(0.0)),
                wrap16(y.unwrap_or(0.0)),
            )))
        })?,
    )?;
    lua.globals().set("Vector2int16", v2i)?;
    Ok(())
}
