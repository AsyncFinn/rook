//! CFrame.

use super::err;
use glam::{EulerRot, Mat3, Quat, Vec3};
use mlua::{
    Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue, Variadic, Vector,
};
use rook_types::{CFrame, Vector3};

use super::fmt_f32;
use crate::ctx::args::number_of;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LCFrame(pub CFrame);

use crate::ctx::convert::{from_lua_vector as v3, lua_vector as lv3};

fn g(v: Vector) -> Vec3 {
    v3(v).0
}

fn lv(v: Vec3) -> Vector {
    lv3(Vector3(v))
}

fn cf(position: Vec3, rotation: Mat3) -> LCFrame {
    LCFrame(CFrame { position, rotation })
}

/// `CFrame.new(...)` with every overload from the creator-docs page.
fn new(_: &Lua, args: MultiValue) -> mlua::Result<LCFrame> {
    let args = args.into_vec();
    let vec_at = |i: usize| match args.get(i) {
        Some(LuaValue::Vector(v)) => Some(g(*v)),
        _ => None,
    };
    let nums: Option<Vec<f32>> = args
        .iter()
        .map(|v| number_of(v).map(|n| n as f32))
        .collect();
    match args.len() {
        0 => Ok(LCFrame(CFrame::IDENTITY)),
        1 => vec_at(0)
            .map(|p| cf(p, Mat3::IDENTITY))
            .ok_or_else(|| err("invalid argument #1 to 'new' (Vector3 expected)")),
        2 => match (vec_at(0), vec_at(1)) {
            (Some(at), Some(target)) => Ok(LCFrame(CFrame::look_at(at, target, Vec3::Y))),
            _ => Err(err("invalid argument #2 to 'new' (Vector3 expected)")),
        },
        3 | 7 | 12 => {
            let n = nums.ok_or_else(|| err("invalid argument to 'new' (number expected)"))?;
            let pos = Vec3::new(n[0], n[1], n[2]);
            Ok(match n.len() {
                3 => cf(pos, Mat3::IDENTITY),
                7 => {
                    let q = Quat::from_xyzw(n[3], n[4], n[5], n[6]).normalize();
                    cf(pos, Mat3::from_quat(q))
                }
                _ => {
                    let mut c = [0.0; 12];
                    c.copy_from_slice(&n);
                    LCFrame(CFrame::from_components(c))
                }
            })
        }
        n => Err(err(format!("Invalid number of arguments: {n}"))),
    }
}

fn from_matrix(
    _: &Lua,
    (pos, x, y, z): (Vector, Vector, Vector, Option<Vector>),
) -> mlua::Result<LCFrame> {
    let (x, y) = (g(x), g(y));
    let z = z.map_or_else(|| x.cross(y).normalize(), g);
    Ok(cf(g(pos), Mat3::from_cols(x, y, z)))
}

/// Rx * Ry * Rz.
fn angles(_: &Lua, (x, y, z): (Option<f32>, Option<f32>, Option<f32>)) -> mlua::Result<LCFrame> {
    Ok(LCFrame(CFrame::angles(
        x.unwrap_or(0.0),
        y.unwrap_or(0.0),
        z.unwrap_or(0.0),
    )))
}

/// Ry * Rx * Rz (`fromOrientation`, `fromEulerAnglesYXZ`).
fn angles_yxz(
    _: &Lua,
    (x, y, z): (Option<f32>, Option<f32>, Option<f32>),
) -> mlua::Result<LCFrame> {
    Ok(LCFrame(CFrame::from_orientation(
        x.unwrap_or(0.0),
        y.unwrap_or(0.0),
        z.unwrap_or(0.0),
    )))
}

impl LCFrame {
    fn to_yxz(self) -> (f32, f32, f32) {
        self.0.to_orientation()
    }

    fn to_axis_angle(self) -> (Vector, f32) {
        let (axis, angle) = Quat::from_mat3(&self.0.rotation).to_axis_angle();
        (lv(axis), angle)
    }
}

impl UserData for LCFrame {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "CFrame");
        for name in ["Position", "p"] {
            f.add_field_method_get(name, |_, t| Ok(lv(t.0.position)));
        }
        f.add_field_method_get("Rotation", |_, t| Ok(cf(Vec3::ZERO, t.0.rotation)));
        f.add_field_method_get("X", |_, t| Ok(t.0.position.x));
        f.add_field_method_get("Y", |_, t| Ok(t.0.position.y));
        f.add_field_method_get("Z", |_, t| Ok(t.0.position.z));
        for name in ["LookVector", "lookVector"] {
            f.add_field_method_get(name, |_, t| Ok(lv3(t.0.look_vector())));
        }
        for name in ["RightVector", "rightVector"] {
            f.add_field_method_get(name, |_, t| Ok(lv3(t.0.right_vector())));
        }
        for name in ["UpVector", "upVector"] {
            f.add_field_method_get(name, |_, t| Ok(lv3(t.0.up_vector())));
        }
        f.add_field_method_get("XVector", |_, t| Ok(lv(t.0.rotation.col(0))));
        f.add_field_method_get("YVector", |_, t| Ok(lv(t.0.rotation.col(1))));
        f.add_field_method_get("ZVector", |_, t| Ok(lv(t.0.rotation.col(2))));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        for name in ["Inverse", "inverse"] {
            m.add_method(name, |_, t, ()| Ok(LCFrame(t.0.inverse())));
        }
        for name in ["Lerp", "lerp"] {
            m.add_method(name, |_, t, (goal, a): (UserDataRef<LCFrame>, f32)| {
                Ok(LCFrame(t.0.lerp(goal.0, a)))
            });
        }
        m.add_method("Orthonormalize", |_, t, ()| {
            Ok(LCFrame(t.0.orthonormalize()))
        });
        for name in ["ToWorldSpace", "toWorldSpace"] {
            m.add_method(name, |_, t, others: Variadic<UserDataRef<LCFrame>>| {
                Ok(Variadic::from_iter(
                    others.iter().map(|o| LCFrame(t.0.to_world_space(o.0))),
                ))
            });
        }
        for name in ["ToObjectSpace", "toObjectSpace"] {
            m.add_method(name, |_, t, others: Variadic<UserDataRef<LCFrame>>| {
                Ok(Variadic::from_iter(
                    others.iter().map(|o| LCFrame(t.0.to_object_space(o.0))),
                ))
            });
        }
        for name in ["PointToWorldSpace", "pointToWorldSpace"] {
            m.add_method(name, |_, t, vs: Variadic<Vector>| {
                Ok(Variadic::from_iter(
                    vs.iter().map(|v| lv3(t.0.point_to_world_space(v3(*v)))),
                ))
            });
        }
        for name in ["PointToObjectSpace", "pointToObjectSpace"] {
            m.add_method(name, |_, t, vs: Variadic<Vector>| {
                Ok(Variadic::from_iter(
                    vs.iter().map(|v| lv3(t.0.point_to_object_space(v3(*v)))),
                ))
            });
        }
        for name in ["VectorToWorldSpace", "vectorToWorldSpace"] {
            m.add_method(name, |_, t, vs: Variadic<Vector>| {
                Ok(Variadic::from_iter(
                    vs.iter().map(|v| lv3(t.0.vector_to_world_space(v3(*v)))),
                ))
            });
        }
        for name in ["VectorToObjectSpace", "vectorToObjectSpace"] {
            m.add_method(name, |_, t, vs: Variadic<Vector>| {
                Ok(Variadic::from_iter(
                    vs.iter().map(|v| lv3(t.0.vector_to_object_space(v3(*v)))),
                ))
            });
        }
        for name in ["GetComponents", "components"] {
            m.add_method(name, |_, t, ()| Ok(Variadic::from_iter(t.0.components())));
        }
        for name in ["ToEulerAnglesXYZ", "toEulerAnglesXYZ"] {
            m.add_method(name, |_, t, ()| Ok(t.0.to_euler_angles_xyz()));
        }
        for name in ["ToEulerAnglesYXZ", "toEulerAnglesYXZ", "ToOrientation"] {
            m.add_method(name, |_, t, ()| Ok(t.to_yxz()));
        }
        for name in ["ToAxisAngle", "toAxisAngle"] {
            m.add_method(name, |_, t, ()| Ok(t.to_axis_angle()));
        }
        m.add_method(
            "FuzzyEq",
            |_, t, (o, e): (UserDataRef<LCFrame>, Option<f32>)| {
                Ok(t.0.fuzzy_eq(&o.0, e.unwrap_or(1e-5)))
            },
        );

        m.add_meta_function(
            MetaMethod::Mul,
            |lua, (a, b): (UserDataRef<LCFrame>, LuaValue)| match b {
                LuaValue::Vector(v) => Ok(LuaValue::Vector(lv3(a.0 * v3(v)))),
                LuaValue::UserData(u) => {
                    let o = u.borrow::<LCFrame>()?;
                    mlua::IntoLua::into_lua(LCFrame(a.0 * o.0), lua)
                }
                _ => Err(err("invalid argument #2 (CFrame or Vector3 expected)")),
            },
        );
        m.add_meta_method(MetaMethod::Add, |_, t, v: Vector| {
            Ok(cf(t.0.position + g(v), t.0.rotation))
        });
        m.add_meta_method(MetaMethod::Sub, |_, t, v: Vector| {
            Ok(cf(t.0.position - g(v), t.0.rotation))
        });
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LCFrame>| {
            Ok(t.0 == o.0)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(t.0
                .components()
                .iter()
                .map(|c| fmt_f32(*c))
                .collect::<Vec<_>>()
                .join(", "))
        });
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set("new", lua.create_function(new)?)?;
    t.set("identity", LCFrame(CFrame::IDENTITY))?;
    t.set(
        "lookAt",
        lua.create_function(|_, (at, target, up): (Vector, Vector, Option<Vector>)| {
            Ok(LCFrame(CFrame::look_at(
                g(at),
                g(target),
                up.map_or(Vec3::Y, g),
            )))
        })?,
    )?;
    t.set(
        "lookAlong",
        lua.create_function(|_, (at, dir, up): (Vector, Vector, Option<Vector>)| {
            let at = g(at);
            Ok(LCFrame(CFrame::look_at(
                at,
                at + g(dir),
                up.map_or(Vec3::Y, g),
            )))
        })?,
    )?;
    let angles_fn = lua.create_function(angles)?;
    t.set("Angles", angles_fn.clone())?;
    t.set("fromEulerAnglesXYZ", angles_fn)?;
    let yxz = lua.create_function(angles_yxz)?;
    t.set("fromEulerAnglesYXZ", yxz.clone())?;
    t.set("fromOrientation", yxz)?;
    t.set(
        "fromEulerAngles",
        lua.create_function(
            |lua, (x, y, z, order): (Option<f32>, Option<f32>, Option<f32>, Option<LuaValue>)| {
                // Enum.RotationOrder: XYZ 0, XZY 1, YZX 2, YXZ 3, ZXY 4, ZYX 5.
                let order = order
                    .and_then(|o| super::enums::item_value(lua, &o, "RotationOrder"))
                    .unwrap_or(0);
                let (x, y, z) = (x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0));
                let (rot, a, b, c) = match order {
                    1 => (EulerRot::XZY, x, z, y),
                    2 => (EulerRot::YZX, y, z, x),
                    3 => (EulerRot::YXZ, y, x, z),
                    4 => (EulerRot::ZXY, z, x, y),
                    5 => (EulerRot::ZYX, z, y, x),
                    _ => (EulerRot::XYZ, x, y, z),
                };
                Ok(cf(Vec3::ZERO, Mat3::from_euler(rot, a, b, c)))
            },
        )?,
    )?;
    t.set(
        "fromAxisAngle",
        lua.create_function(|_, (axis, r): (Vector, f32)| {
            Ok(cf(
                Vec3::ZERO,
                Mat3::from_axis_angle(g(axis).normalize(), r),
            ))
        })?,
    )?;
    t.set("fromMatrix", lua.create_function(from_matrix)?)?;
    lua.globals().set("CFrame", t)?;
    Ok(())
}
