//! WorldModels, each a world of its own. Expected values were observed in
//! Studio 0.747 (docs/behavior/physics/queries.md#worldmodels).

use super::*;

use crate::query::{CastShape, Filter, Region};

const DOWN: Vec3 = Vec3::new(0.0, -20.0, 0.0);

fn all() -> Filter {
    Filter::legacy_ray(Vec::new(), false)
}

/// A 4-stud cube at `at` under `parent`.
fn cube(dom: &mut Dom, parent: InstanceId, at: Vec3, anchored: bool) -> InstanceId {
    child(
        dom,
        "Part",
        parent,
        &[
            ("CFrame", Value::CFrame(CFrame::from_position(at))),
            ("Size", v3(4.0, 4.0, 4.0)),
            ("Anchored", Value::Bool(anchored)),
        ],
    )
}

/// The part a ray straight down onto `at` hits in `model`'s world.
fn hit_in(physics: &mut Physics, dom: &mut Dom, model: InstanceId, at: Vec3) -> Option<InstanceId> {
    let from = at + Vec3::Y * 10.0;
    (physics.model_raycast(dom, model, from, DOWN, &all())).map(|h| h.part)
}

/// The part a ray straight down onto `at` hits in Workspace's world.
fn hit_in_workspace(physics: &mut Physics, dom: &mut Dom, at: Vec3) -> Option<InstanceId> {
    (physics.raycast(dom, at + Vec3::Y * 10.0, DOWN, &all())).map(|h| h.part)
}

#[test]
fn a_world_models_queries_find_only_its_own_parts() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let storage = dom.get_service("ReplicatedStorage").unwrap();
    let wm = child(&mut dom, "WorldModel", storage, &[]);
    let own = cube(&mut dom, wm, Vec3::new(0.0, 500.0, 0.0), true);
    let model = child(&mut dom, "Model", wm, &[]);
    let nested = cube(&mut dom, model, Vec3::new(100.0, 500.0, 0.0), true);
    let inner = child(&mut dom, "WorldModel", wm, &[]);
    let inner_part = cube(&mut dom, inner, Vec3::new(300.0, 500.0, 0.0), true);
    let in_workspace = child(&mut dom, "WorldModel", ws, &[]);
    let walled = cube(&mut dom, in_workspace, Vec3::new(0.0, 600.0, 0.0), true);
    let mut physics = Physics::new();
    let p = &mut physics;
    assert_eq!(
        hit_in(p, &mut dom, wm, Vec3::new(0.0, 500.0, 0.0)),
        Some(own)
    );
    assert_eq!(
        hit_in(p, &mut dom, wm, Vec3::new(100.0, 500.0, 0.0)),
        Some(nested)
    );
    // A WorldModel inside is a world of its own.
    assert_eq!(hit_in(p, &mut dom, wm, Vec3::new(300.0, 500.0, 0.0)), None);
    let inner_at = Vec3::new(300.0, 500.0, 0.0);
    assert_eq!(hit_in(p, &mut dom, inner, inner_at), Some(inner_part));
    // Neither world sees the other's parts, even a WorldModel in
    // Workspace.
    assert_eq!(hit_in(p, &mut dom, wm, Vec3::new(0.0, 600.0, 0.0)), None);
    assert_eq!(
        hit_in_workspace(p, &mut dom, Vec3::new(0.0, 500.0, 0.0)),
        None
    );
    assert_eq!(
        hit_in_workspace(p, &mut dom, Vec3::new(0.0, 600.0, 0.0)),
        None
    );
    let at = Vec3::new(0.0, 600.0, 0.0);
    assert_eq!(hit_in(p, &mut dom, in_workspace, at), Some(walled));
    // A part moved is found where it is now.
    let moved = CFrame::from_position(Vec3::new(50.0, 500.0, 0.0));
    set(&mut dom, own, "CFrame", Value::CFrame(moved));
    assert_eq!(hit_in(p, &mut dom, wm, Vec3::new(0.0, 500.0, 0.0)), None);
    assert_eq!(
        hit_in(p, &mut dom, wm, Vec3::new(50.0, 500.0, 0.0)),
        Some(own)
    );
    // A part added or taken out.
    let added = cube(&mut dom, wm, Vec3::new(200.0, 500.0, 0.0), true);
    assert_eq!(
        hit_in(p, &mut dom, wm, Vec3::new(200.0, 500.0, 0.0)),
        Some(added)
    );
    dom.set_parent(added, Some(ws)).unwrap();
    assert_eq!(hit_in(p, &mut dom, wm, Vec3::new(200.0, 500.0, 0.0)), None);
    let back = Vec3::new(200.0, 500.0, 0.0);
    assert_eq!(hit_in_workspace(p, &mut dom, back), Some(added));
    // Out of the DataModel it finds nothing.
    dom.set_parent(wm, None).unwrap();
    assert_eq!(hit_in(p, &mut dom, wm, Vec3::new(50.0, 500.0, 0.0)), None);
}

#[test]
fn a_world_models_casts_and_overlaps_stay_in_it() {
    let mut dom = Dom::new();
    let storage = dom.get_service("ReplicatedStorage").unwrap();
    let wm = child(&mut dom, "WorldModel", storage, &[]);
    let at = Vec3::new(0.0, 500.0, 0.0);
    let own = cube(&mut dom, wm, at, true);
    // The same place in Workspace.
    let ws = dom.get_service("Workspace").unwrap();
    cube(&mut dom, ws, at, true);
    let mut physics = Physics::new();
    let start = CFrame::from_position(at + Vec3::Y * 10.0);
    let filter = all();
    let hit = (physics.model_shapecast(&mut dom, wm, CastShape::Sphere(1.0), start, DOWN, &filter))
        .unwrap();
    assert_eq!(hit.part, own);
    assert!((hit.distance - 7.0).abs() < 1e-3, "{hit:?}");
    assert!((hit.position.y - 502.0).abs() < 1e-3, "{hit:?}");
    assert!((hit.normal - Vec3::Y).length() < 1e-4, "{hit:?}");
    let region = Region::Box {
        cframe: CFrame::from_position(at),
        size: Vec3::splat(10.0),
    };
    let overlap = Filter::legacy_region(vec![], false);
    let found = physics.model_parts_in_bounds(&mut dom, wm, region, &overlap, 0);
    assert_eq!(found, [own]);
    let probe = cube(&mut dom, storage, at, true);
    let found = physics.model_parts_in_part(&mut dom, wm, probe, &overlap, 0);
    assert_eq!(found, [own]);
}

#[test]
fn nothing_simulates_a_world_models_parts() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let wm = child(&mut dom, "WorldModel", ws, &[]);
    let held = cube(&mut dom, wm, Vec3::new(0.0, 100.0, 0.0), false);
    let falling = cube(&mut dom, ws, Vec3::new(20.0, 100.0, 0.0), false);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0);
    assert_eq!(position(&dom, held), Vec3::new(0.0, 100.0, 0.0));
    assert!(position(&dom, falling).y < 99.0);
}
