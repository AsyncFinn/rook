//! MakeJoints and legacy surface joints. Expected values were observed in
//! Studio 0.741.

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, Value};

use super::joints::at;
use super::{block, part, part_of, set};
use crate::joints::{self, surfaces};

fn surfaces(dom: &mut Dom, id: InstanceId, surface: &str) {
    let e = dom.database().enum_by_name("SurfaceType").unwrap();
    let n = e.items.iter().find(|(name, _)| *name == surface).unwrap().1;
    for face in ["Top", "Bottom", "Left", "Right", "Front", "Back"] {
        set(dom, id, &format!("{face}Surface"), Value::Enum(n));
    }
}

/// Two 4×1×2 parts stacked `gap` apart; the upper one calls MakeJoints.
/// Returns the joints' classes and whether the upper part is Part0.
fn stacked(lower: &str, upper: &str, gap: f32) -> Vec<(String, bool)> {
    let mut dom = Dom::new();
    let size = Vec3::new(4.0, 1.0, 2.0);
    let bottom = part(&mut dom, Vec3::ZERO, size, true);
    let top = part(&mut dom, Vec3::new(0.0, 1.0 + gap, 0.0), size, true);
    surfaces(&mut dom, bottom, lower);
    surfaces(&mut dom, top, upper);
    surfaces::make_joints(&mut dom, &[top]).unwrap();
    joints::part_joints(&dom, top)
        .iter()
        .map(|j| (dom.class_name(j.id).to_owned(), j.part0 == top))
        .collect()
}

#[test]
fn surface_types_pick_the_joint() {
    let one = |lower, upper| stacked(lower, upper, 0.0);
    let joint = |class: &str, upper_is_part0| vec![(class.to_owned(), upper_is_part0)];
    assert_eq!(one("Studs", "Inlet"), joint("Snap", true));
    assert_eq!(one("Universal", "Universal"), joint("Snap", true));
    assert_eq!(one("Studs", "Studs"), vec![]);
    assert_eq!(one("Inlet", "Inlet"), vec![]);
    assert_eq!(one("Smooth", "Smooth"), vec![]);
    assert_eq!(one("Smooth", "Weld"), joint("Weld", true));
    assert_eq!(one("Glue", "Studs"), joint("Glue", true));
    assert_eq!(one("Glue", "Weld"), joint("Weld", true));
    assert_eq!(one("Hinge", "Weld"), joint("Rotate", false));
    assert_eq!(one("Motor", "Smooth"), joint("RotateV", false));
    assert_eq!(one("SteppingMotor", "Studs"), joint("RotateP", false));
    assert_eq!(one("Motor", "Hinge"), joint("Rotate", true));
}

#[test]
fn faces_join_only_when_close_and_aligned() {
    assert_eq!(stacked("Weld", "Weld", 0.0099).len(), 1);
    assert_eq!(stacked("Weld", "Weld", 0.011).len(), 0);
    assert_eq!(stacked("Weld", "Weld", -0.008).len(), 1);
    assert_eq!(stacked("Weld", "Weld", -0.011).len(), 0);

    let yawed = |degrees: f32| {
        let mut dom = Dom::new();
        let a = block(&mut dom, Vec3::ZERO, true);
        let b = block(&mut dom, Vec3::ZERO, true);
        let cf = at(0.0, 2.0, 0.0) * CFrame::angles(0.0, degrees.to_radians(), 0.0);
        set(&mut dom, b, "CFrame", Value::CFrame(cf));
        surfaces(&mut dom, a, "Weld");
        surfaces::make_joints(&mut dom, &[a]).unwrap();
        joints::part_joints(&dom, a).len()
    };
    assert_eq!(yawed(0.5), 1);
    assert_eq!(yawed(0.6), 0);
    assert_eq!(yawed(30.0), 0);
    assert_eq!(yawed(90.0), 1);

    // Touching along an edge isn't enough.
    let mut dom = Dom::new();
    let size = Vec3::new(4.0, 1.0, 2.0);
    let a = part(&mut dom, Vec3::ZERO, size, true);
    part(&mut dom, Vec3::new(4.0, 1.0, 0.0), size, true);
    surfaces(&mut dom, a, "Weld");
    surfaces::make_joints(&mut dom, &[a]).unwrap();
    assert!(joints::part_joints(&dom, a).is_empty());
}

#[test]
fn make_joints_frames_match_studio() {
    let check = |dir: Vec3, c0: [f32; 12], c1: [f32; 12]| {
        let mut dom = Dom::new();
        let a = block(&mut dom, Vec3::ZERO, true);
        block(&mut dom, dir * 2.0, true);
        surfaces(&mut dom, a, "Weld");
        surfaces::make_joints(&mut dom, &[a]).unwrap();
        let j = joints::part_joints(&dom, a)[0];
        assert_eq!(j.part0, a);
        assert_eq!(dom.parent(j.id), Some(a));
        assert_eq!(dom.name(j.id), "Weld");
        let get = |name| dom.cframe_property(j.id, name).unwrap();
        let (got0, got1) = (get("C0"), get("C1"));
        assert!(
            got0.fuzzy_eq(&CFrame::from_components(c0), 1e-5),
            "{dir} C0 {got0:?}"
        );
        assert!(
            got1.fuzzy_eq(&CFrame::from_components(c1), 1e-5),
            "{dir} C1 {got1:?}"
        );
    };
    // Recorded with :MakeJoints() on 2×2×2 parts, as CFrame components.
    #[rustfmt::skip]
    let cases = [
        (Vec3::Y, [0., 1., 0., -1., 0., 0., 0., 0., 1., 0., 1., 0.], [0., -1., 0., -1., 0., 0., 0., 0., 1., 0., 1., 0.]),
        (-Vec3::Y, [0., -1., 0., 1., 0., 0., 0., 0., -1., 0., 1., 0.], [0., 1., 0., 1., 0., 0., 0., 0., -1., 0., 1., 0.]),
        (Vec3::X, [1., 0., 0., 0., 0., 1., 0., 1., 0., -1., 0., 0.], [-1., 0., 0., 0., 0., 1., 0., 1., 0., -1., 0., 0.]),
        (-Vec3::X, [-1., 0., 0., 0., 0., -1., 0., 1., 0., 1., 0., 0.], [1., 0., 0., 0., 0., -1., 0., 1., 0., 1., 0., 0.]),
        (Vec3::Z, [0., 0., 1., 1., 0., 0., 0., 1., 0., 0., 0., 1.], [0., 0., -1., 1., 0., 0., 0., 1., 0., 0., 0., 1.]),
        (-Vec3::Z, [0., 0., -1., -1., 0., 0., 0., 1., 0., 0., 0., -1.], [0., 0., 1., -1., 0., 0., 0., 1., 0., 0., 0., -1.]),
    ];
    for (dir, c0, c1) in cases {
        check(dir, c0, c1);
    }
}

#[test]
fn model_parts_join_parts_outside_and_only_once() {
    // Model:MakeJoints on A and B joins A-B and A-C (C is outside the
    // model); B doesn't join A again, and a second call adds nothing.
    let mut dom = Dom::new();
    let a = block(&mut dom, Vec3::ZERO, true);
    let b = block(&mut dom, Vec3::new(0.0, 2.0, 0.0), true);
    let c = block(&mut dom, Vec3::new(2.0, 0.0, 0.0), true);
    for p in [a, b, c] {
        surfaces(&mut dom, p, "Weld");
    }
    surfaces::make_joints(&mut dom, &[a, b]).unwrap();
    surfaces::make_joints(&mut dom, &[a, b]).unwrap();
    assert_eq!(joints::part_joints(&dom, a).len(), 2);
    assert_eq!(joints::part_joints(&dom, b).len(), 1);
    assert_eq!(joints::part_joints(&dom, c).len(), 1);
}

#[test]
fn many_callers_sweep_the_world_with_the_same_result() {
    // A column of 20 parts: each neighbouring pair joins once, Part0 being
    // the one earlier in the list.
    let mut dom = Dom::new();
    let column: Vec<_> = (0..20)
        .map(|i| block(&mut dom, Vec3::new(0.0, 2.0 * i as f32, 0.0), true))
        .collect();
    for &p in &column {
        surfaces(&mut dom, p, "Weld");
    }
    let mut callers = column.clone();
    callers.reverse();
    surfaces::make_joints(&mut dom, &callers).unwrap();
    for pair in column.windows(2) {
        let js = joints::part_joints(&dom, pair[0]);
        let j = js.iter().find(|j| j.other(pair[0]) == pair[1]).unwrap();
        assert_eq!(j.part0, pair[1]);
    }
    let total: usize = column
        .iter()
        .map(|p| joints::part_joints(&dom, *p).len())
        .sum();
    assert_eq!(total, 2 * 19);
}

/// A 2×4×4 wedge with a 2×1×√32 slab lying on its slope, all Smooth.
fn slab_on_wedge(dom: &mut Dom) -> (InstanceId, InstanceId) {
    let wedge = part_of(dom, "WedgePart", Vec3::ZERO, Vec3::new(2.0, 4.0, 4.0), true);
    let slab_size = Vec3::new(2.0, 1.0, 32f32.sqrt());
    let slab = part(dom, Vec3::ZERO, slab_size, true);
    let on_slope = Vec3::new(0.0, 1.0, -1.0).normalize() * 0.5;
    let cf = at(on_slope.x, on_slope.y, on_slope.z) * CFrame::angles(-45f32.to_radians(), 0.0, 0.0);
    set(dom, slab, "CFrame", Value::CFrame(cf));
    surfaces(dom, wedge, "Smooth");
    surfaces(dom, slab, "Smooth");
    (wedge, slab)
}

#[test]
fn wedge_slopes_join_like_studio() {
    // A 2×4×4 wedge with a 2×1×√32 slab lying on its slope, everything
    // Smooth except `on_wedge` and the slab's bottom. Returns the joint's
    // class, whether the wedge is Part0, and C0's position.
    let trial = |on_wedge: &[(&str, &str)],
                 slab_bottom: &str,
                 wedge_calls: bool|
     -> Option<(String, bool, Vec3)> {
        let mut dom = Dom::new();
        let (wedge, slab) = slab_on_wedge(&mut dom);
        let e = dom.database().enum_by_name("SurfaceType").unwrap();
        let n = |s: &str| Value::Enum(e.items.iter().find(|(name, _)| *name == s).unwrap().1);
        let wedge_surfaces: Vec<_> = on_wedge.iter().map(|&(f, s)| (f, n(s))).collect();
        let bottom = n(slab_bottom);
        for (face, v) in wedge_surfaces {
            set(&mut dom, wedge, &format!("{face}Surface"), v);
        }
        set(&mut dom, slab, "BottomSurface", bottom);
        let caller = if wedge_calls { wedge } else { slab };
        surfaces::make_joints(&mut dom, &[caller]).unwrap();
        let js = joints::part_joints(&dom, wedge);
        assert!(js.len() <= 1);
        let j = js.first()?;
        let c0 = dom.cframe_property(j.id, "C0").expect("C0");
        Some((
            dom.class_name(j.id).to_owned(),
            j.part0 == wedge,
            c0.position,
        ))
    };
    let joined = |class: &str, wedge_is_part0, c0: [f32; 3]| {
        Some((class.to_owned(), wedge_is_part0, Vec3::from(c0)))
    };
    let slab_face = [0.0, -0.5, 0.0];

    // The wedge calling: only its own Weld, Top before Front.
    let top = [0.0, 2.0, 0.0];
    let front = [0.0, 0.0, -2.0];
    assert_eq!(
        trial(&[("Top", "Weld")], "Smooth", true),
        joined("Weld", true, top)
    );
    assert_eq!(
        trial(&[("Front", "Weld")], "Smooth", true),
        joined("Weld", true, front)
    );
    let both = [("Top", "Weld"), ("Front", "Weld")];
    assert_eq!(trial(&both, "Smooth", true), joined("Weld", true, top));
    let glue_top = [("Top", "Glue"), ("Front", "Weld")];
    assert_eq!(
        trial(&glue_top, "Smooth", true),
        joined("Weld", true, front)
    );
    assert_eq!(trial(&[], "Weld", true), None);
    assert_eq!(trial(&[("Front", "Universal")], "Universal", true), None);
    assert_eq!(trial(&[("Front", "Hinge")], "Weld", true), None);

    // The slab calling: the slope is the wedge's Front.
    assert_eq!(trial(&[], "Weld", false), joined("Weld", false, slab_face));
    assert_eq!(
        trial(&[("Front", "Weld")], "Smooth", false),
        joined("Weld", false, slab_face)
    );
    assert_eq!(trial(&[("Top", "Weld")], "Smooth", false), None);
    assert_eq!(
        trial(&[("Front", "Studs")], "Inlet", false),
        joined("Snap", false, slab_face)
    );
    assert_eq!(trial(&[("Top", "Studs")], "Inlet", false), None);
    assert_eq!(trial(&[], "Hinge", false), None);

    // The box's top and front aren't faces.
    let mut dom = Dom::new();
    let wedge = part_of(
        &mut dom,
        "WedgePart",
        Vec3::ZERO,
        Vec3::new(2.0, 4.0, 4.0),
        true,
    );
    part(
        &mut dom,
        Vec3::new(0.0, 2.5, 0.0),
        Vec3::new(2.0, 1.0, 4.0),
        true,
    );
    part(
        &mut dom,
        Vec3::new(0.0, 0.0, -2.5),
        Vec3::new(2.0, 4.0, 1.0),
        true,
    );
    surfaces(&mut dom, wedge, "Weld");
    surfaces::make_joints(&mut dom, &[wedge]).unwrap();
    assert!(joints::part_joints(&dom, wedge).is_empty());
}

#[test]
fn a_slope_joins_from_either_caller() {
    // Observed in Studio with Model:MakeJoints: a slab whose Weld bottom
    // lies on a wedge's slope joins whichever part comes first, the slab
    // being Part0 either way.
    for wedge_first in [true, false] {
        let mut dom = Dom::new();
        let (wedge, slab) = slab_on_wedge(&mut dom);
        let e = dom.database().enum_by_name("SurfaceType").unwrap();
        let weld = e.items.iter().find(|(name, _)| *name == "Weld").unwrap().1;
        set(&mut dom, slab, "BottomSurface", Value::Enum(weld));
        let callers = if wedge_first {
            [wedge, slab]
        } else {
            [slab, wedge]
        };
        surfaces::make_joints(&mut dom, &callers).unwrap();
        let js = joints::part_joints(&dom, wedge);
        assert_eq!(js.len(), 1, "wedge first: {wedge_first}");
        assert_eq!(dom.class_name(js[0].id), "Weld");
        assert_eq!(js[0].part0, slab);
    }
}

#[test]
fn balls_and_cylinders_join_as_boxes() {
    // A ball or cylinder beside a 10-stud Smooth cube (or one whose facing
    // surface is `base_face`) calls MakeJoints. Returns the joint's class,
    // whether the round part is Part0, and C0's position.
    let trial = |shape: u32,
                 size: Vec3,
                 offset: Vec3,
                 own: (&str, &str),
                 base_face: Option<(&str, &str)>|
     -> Option<(String, bool, Vec3)> {
        let mut dom = Dom::new();
        let base = part(&mut dom, Vec3::ZERO, Vec3::splat(10.0), true);
        let round = part(&mut dom, offset, size, true);
        set(&mut dom, round, "Shape", Value::Enum(shape));
        surfaces(&mut dom, base, "Smooth");
        surfaces(&mut dom, round, "Smooth");
        let e = dom.database().enum_by_name("SurfaceType").unwrap();
        let n = |s: &str| Value::Enum(e.items.iter().find(|(name, _)| *name == s).unwrap().1);
        set(&mut dom, round, &format!("{}Surface", own.0), n(own.1));
        if let Some((face, s)) = base_face {
            set(&mut dom, base, &format!("{face}Surface"), n(s));
        }
        surfaces::make_joints(&mut dom, &[round]).unwrap();
        let j = *joints::part_joints(&dom, round).first()?;
        let c0 = dom.cframe_property(j.id, "C0").expect("C0");
        Some((
            dom.class_name(j.id).to_owned(),
            j.part0 == round,
            c0.position,
        ))
    };
    let joined = |class: &str, c0: Vec3| Some((class.to_owned(), true, c0));
    const BALL: u32 = 0;
    const CYLINDER: u32 = 2;
    let ball = Vec3::splat(3.0);
    let wheel = Vec3::new(2.0, 6.0, 6.0);
    // Observed in Studio: everything framed at the round part's box face.
    assert_eq!(
        trial(BALL, ball, Vec3::Y * 6.5, ("Bottom", "Hinge"), None),
        joined("Rotate", Vec3::new(0.0, -1.5, 0.0))
    );
    assert_eq!(
        trial(
            BALL,
            ball,
            Vec3::Y * 6.5,
            ("Bottom", "Studs"),
            Some(("Top", "Inlet"))
        ),
        joined("Snap", Vec3::new(0.0, -1.5, 0.0))
    );
    assert_eq!(
        trial(CYLINDER, wheel, -Vec3::X * 6.0, ("Right", "Hinge"), None),
        joined("Rotate", Vec3::new(1.0, 0.0, 0.0))
    );
    assert_eq!(
        trial(CYLINDER, wheel, Vec3::Y * 8.0, ("Bottom", "Weld"), None),
        joined("Weld", Vec3::new(0.0, -3.0, 0.0))
    );
}

#[test]
fn surface_type_numbers_match_the_database() {
    let e = rook_reflection::database()
        .enum_by_name("SurfaceType")
        .unwrap();
    let value = |name: &str| e.items.iter().find(|(n, _)| *n == name).unwrap().1;
    let names = [
        "Glue",
        "Weld",
        "Studs",
        "Inlet",
        "Universal",
        "Hinge",
        "Motor",
        "SteppingMotor",
    ];
    for (i, name) in names.iter().enumerate() {
        assert_eq!(value(name), i as u32 + 1, "{name}");
    }
}
