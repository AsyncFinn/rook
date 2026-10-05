//! Ray, Rect and Region3.

use mlua::{
    Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue, Vector,
};
use rbx_types::{Ray, Rect, Region3};
use rook_types::{CFrame, Vector2, Vector3};

use super::{LCFrame, LVector2, fmt_f32};
use crate::args::{borrow_clone, number_of};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LRay(pub Ray);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LRect(pub Rect);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LRegion3(pub Region3);

fn r3(v: rbx_types::Vector3) -> Vector3 {
    v.into()
}

fn lv(v: Vector3) -> Vector {
    Vector::new(v.x(), v.y(), v.z())
}

fn from_lv(v: Vector) -> Vector3 {
    Vector3::new(v.x(), v.y(), v.z())
}

fn fmt_v3(v: Vector3) -> String {
    format!("{}, {}, {}", fmt_f32(v.x()), fmt_f32(v.y()), fmt_f32(v.z()))
}

impl LRay {
    fn closest_point(&self, p: Vector3) -> Vector3 {
        let o = r3(self.0.origin);
        let d = r3(self.0.direction).unit();
        let t = (p - o).dot(d).max(0.0);
        o + d * t
    }
}

impl UserData for LRay {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Ray");
        f.add_field_method_get("Origin", |_, t| Ok(lv(r3(t.0.origin))));
        f.add_field_method_get("Direction", |_, t| Ok(lv(r3(t.0.direction))));
        f.add_field_method_get("Unit", |_, t| {
            Ok(LRay(Ray::new(t.0.origin, r3(t.0.direction).unit().into())))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("ClosestPoint", |_, t, p: Vector| {
            Ok(lv(t.closest_point(from_lv(p))))
        });
        m.add_method("Distance", |_, t, p: Vector| {
            let p = from_lv(p);
            Ok((t.closest_point(p) - p).magnitude())
        });
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LRay>| Ok(t.0 == o.0));
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!(
                "{{{}}}, {{{}}}",
                fmt_v3(r3(t.0.origin)),
                fmt_v3(r3(t.0.direction))
            ))
        });
    }
}

impl UserData for LRect {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Rect");
        f.add_field_method_get("Min", |_, t| Ok(LVector2(t.0.min.into())));
        f.add_field_method_get("Max", |_, t| Ok(LVector2(t.0.max.into())));
        f.add_field_method_get("Width", |_, t| Ok(t.0.max.x - t.0.min.x));
        f.add_field_method_get("Height", |_, t| Ok(t.0.max.y - t.0.min.y));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LRect>| Ok(t.0 == o.0));
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!(
                "{}, {}, {}, {}",
                fmt_f32(t.0.min.x),
                fmt_f32(t.0.min.y),
                fmt_f32(t.0.max.x),
                fmt_f32(t.0.max.y)
            ))
        });
    }
}

impl UserData for LRegion3 {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Region3");
        f.add_field_method_get("CFrame", |_, t| {
            let center = (r3(t.0.min) + r3(t.0.max)) * 0.5;
            Ok(LCFrame(CFrame::from_position(center.0)))
        });
        f.add_field_method_get("Size", |_, t| Ok(lv(r3(t.0.max) - r3(t.0.min))));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("ExpandToGrid", |_, t, res: f32| {
            let snap = |v: Vector3, f: fn(f32) -> f32| {
                Vector3::new(
                    f(v.x() / res) * res,
                    f(v.y() / res) * res,
                    f(v.z() / res) * res,
                )
            };
            Ok(LRegion3(Region3::new(
                snap(r3(t.0.min), f32::floor).into(),
                snap(r3(t.0.max), f32::ceil).into(),
            )))
        });
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LRegion3>| {
            Ok(t.0 == o.0)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            let center = (r3(t.0.min) + r3(t.0.max)) * 0.5;
            let size = r3(t.0.max) - r3(t.0.min);
            Ok(format!("{}; {}", fmt_v3(center), fmt_v3(size)))
        });
    }
}

fn rect_new(_: &Lua, args: MultiValue) -> mlua::Result<LRect> {
    let args = args.into_vec();
    let v2 = |v: &LuaValue| match v {
        LuaValue::UserData(u) => borrow_clone::<LVector2>(u).map(|v| v.0),
        _ => None,
    };
    let (min, max) = match args.as_slice() {
        [a, b] => (
            v2(a).unwrap_or(Vector2::ZERO),
            v2(b).unwrap_or(Vector2::ZERO),
        ),
        _ => {
            let n = |i: usize| args.get(i).and_then(number_of).unwrap_or(0.0) as f32;
            (Vector2::new(n(0), n(1)), Vector2::new(n(2), n(3)))
        }
    };
    Ok(LRect(Rect::new(min.into(), max.into())))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    let ray = lua.create_table()?;
    ray.set(
        "new",
        lua.create_function(|_, (o, d): (Option<Vector>, Option<Vector>)| {
            let z = Vector::new(0.0, 0.0, 0.0);
            Ok(LRay(Ray::new(
                from_lv(o.unwrap_or(z)).into(),
                from_lv(d.unwrap_or(z)).into(),
            )))
        })?,
    )?;
    g.set("Ray", ray)?;

    let rect = lua.create_table()?;
    rect.set("new", lua.create_function(rect_new)?)?;
    g.set("Rect", rect)?;

    let region = lua.create_table()?;
    region.set(
        "new",
        lua.create_function(|_, (a, b): (Option<Vector>, Option<Vector>)| {
            let z = Vector::new(0.0, 0.0, 0.0);
            Ok(LRegion3(Region3::new(
                from_lv(a.unwrap_or(z)).into(),
                from_lv(b.unwrap_or(z)).into(),
            )))
        })?,
    )?;
    g.set("Region3", region)?;
    Ok(())
}
