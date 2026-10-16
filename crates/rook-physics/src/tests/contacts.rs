//! Materials, collision groups and touches. Expected values were observed
//! in Studio 0.742.

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, CustomPhysicalProperties, PhysicalProperties, Value};

use super::{part, position, set};
use crate::contacts::groups::{CollisionGroups, GroupError};
use crate::contacts::materials;
use crate::{Physics, Touch, part_mass};

fn material(dom: &mut Dom, id: InstanceId, name: &str) {
    let e = dom.database().enum_by_name("Material").unwrap();
    let n = e.items.iter().find(|(n, _)| *n == name).unwrap().1;
    set(dom, id, "Material", Value::Enum(n));
}

fn custom(dom: &mut Dom, id: InstanceId, d: f32, f: f32, e: f32, fw: f32, ew: f32) {
    let c = CustomPhysicalProperties::new(d, f, e, fw, ew, 1.0);
    set(
        dom,
        id,
        "CustomPhysicalProperties",
        Value::PhysicalProperties(PhysicalProperties::Custom(c)),
    );
}

/// Step at 60 Hz, collecting touches as (part, other, began).
fn run(physics: &mut Physics, dom: &mut Dom, seconds: f64) -> Vec<Touch> {
    let mut touches = Vec::new();
    for _ in 0..(seconds * 60.0).round() as u32 {
        physics.sync(dom);
        physics.step(1.0 / 60.0);
        physics.write_back(dom);
        touches.extend(physics.take_touches());
    }
    touches
}

#[test]
fn mass_follows_material_shape_and_custom_properties() {
    let mut dom = Dom::new();
    let p = part(&mut dom, "Part", Vec3::ZERO, Vec3::splat(2.0), true);
    let mass = |dom: &Dom| part_mass(dom, p);
    assert!((mass(&dom) - 5.6).abs() < 1e-4);
    material(&mut dom, p, "Wood");
    assert!((mass(&dom) - 2.8).abs() < 1e-4);
    material(&mut dom, p, "Metal");
    assert!((mass(&dom) - 62.8).abs() < 1e-3);
    custom(&mut dom, p, 2.0, 0.1, 0.9, 1.0, 1.0);
    assert!((mass(&dom) - 16.0).abs() < 1e-4);

    let shaped = |shape: u32, size: Vec3| {
        let mut dom = Dom::new();
        let p = part(&mut dom, "Part", Vec3::ZERO, size, true);
        set(&mut dom, p, "Shape", Value::Enum(shape));
        part_mass(&dom, p)
    };
    assert!(
        (shaped(0, Vec3::splat(2.0)) - 2.932_153).abs() < 1e-4,
        "ball"
    );
    assert!(
        (shaped(2, Vec3::new(4.0, 2.0, 2.0)) - 8.796_459).abs() < 1e-4,
        "cylinder"
    );
    let class = |class: &str| {
        let mut dom = Dom::new();
        let p = part(&mut dom, class, Vec3::ZERO, Vec3::splat(2.0), true);
        part_mass(&dom, p)
    };
    assert!((class("WedgePart") - 2.8).abs() < 1e-4);
    assert!((class("CornerWedgePart") - 1.866_666_7).abs() < 1e-4);
}

#[test]
fn materials_mix_by_weight() {
    assert_eq!(materials::of_material(512).density, 0.35);
    assert_eq!(materials::of_material(9999), materials::PLASTIC);
    assert!((materials::mix(0.3, 1.0, 0.02, 3.0) - 0.09).abs() < 1e-6);
    assert!((materials::mix(0.2, 3.0, 0.8, 1.0) - 0.35).abs() < 1e-6);
    let p = materials::Properties {
        density: 1e6,
        friction: -1.0,
        elasticity: 5.0,
        friction_weight: 1000.0,
        elasticity_weight: -1.0,
        acoustic_absorption: 5.0,
    }
    .clamped();
    assert_eq!(
        (
            p.density,
            p.friction,
            p.elasticity,
            p.friction_weight,
            p.elasticity_weight,
            p.acoustic_absorption
        ),
        (100.0, 0.0, 1.0, 100.0, 0.0, 1.0)
    );
}

/// The friction a block with `block`'s friction and weight shows sliding
/// at 20 studs/s over a floor with `floor`'s: it slows by mu·g.
fn sliding_friction(floor: (f32, f32), block: (f32, f32)) -> f32 {
    let (floor_f, floor_w) = floor;
    let (block_f, block_w) = block;
    let mut dom = Dom::new();
    let floor = part(
        &mut dom,
        "Part",
        Vec3::ZERO,
        Vec3::new(200.0, 1.0, 10.0),
        true,
    );
    let block = part(
        &mut dom,
        "Part",
        Vec3::new(-80.0, 1.5, 0.0),
        Vec3::splat(2.0),
        false,
    );
    custom(&mut dom, floor, 1.0, floor_f, 0.0, floor_w, 1.0);
    custom(&mut dom, block, 1.0, block_f, 0.0, block_w, 1.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 0.5);
    set(
        &mut dom,
        block,
        "AssemblyLinearVelocity",
        Value::Vector3(rook_types::Vector3::new(20.0, 0.0, 0.0)),
    );
    run(&mut physics, &mut dom, 0.05);
    let speed = |dom: &Dom| match dom.property_by_name(block, "AssemblyLinearVelocity") {
        Some(Value::Vector3(v)) => v.x(),
        other => panic!("{other:?}"),
    };
    let v1 = speed(&dom);
    run(&mut physics, &mut dom, 0.25);
    (v1 - speed(&dom)) / 0.25 / 196.2
}

#[test]
fn friction_is_mixed_in_the_simulation() {
    // Studio measured mu ≈ 0.09 for 0.3 (weight 1) against 0.02 (weight 3).
    let mu = sliding_friction((0.3, 1.0), (0.02, 3.0));
    assert!((mu - 0.09).abs() < 0.015, "{mu}");
    // Equal weights average plainly, with no contact hook on either part.
    let mu = sliding_friction((0.3, 1.0), (0.02, 1.0));
    assert!((mu - 0.16).abs() < 0.015, "{mu}");
}

#[test]
fn collision_group_data_round_trips() {
    // a large modern place's: Default, NoClip, FlyNoClip.
    let bytes: Vec<u8> = vec![
        1, 3, 0, 4, 249, 255, 255, 255, 7, 68, 101, 102, 97, 117, 108, 116, 1, 4, 250, 255, 255,
        255, 6, 78, 111, 67, 108, 105, 112, 2, 4, 248, 255, 255, 255,
        9, 70, 108, 121, 78, 111, 67, 108, 105, 112,
    ];
    let groups = CollisionGroups::decode(&bytes).unwrap();
    assert_eq!(groups.0.len(), 3);
    assert_eq!(groups.0[1].name, "NoClip");
    assert_eq!(groups.0[0].mask, -7);
    assert!(!groups.collidable("Default", "NoClip"));
    assert_eq!(groups.encode(), bytes);
}

#[test]
fn collision_groups_follow_studio() {
    let mut g = CollisionGroups::default();
    g.register("A").unwrap();
    g.register("B").unwrap();
    g.register("A").unwrap();
    g.set_collidable("A", "B", false).unwrap();
    assert_eq!(
        (g.get("A").unwrap().mask, g.get("B").unwrap().mask),
        (-5, -3)
    );
    assert!(!g.collidable("B", "A") && g.collidable("A", "A") && g.collidable("A", "Z"));
    assert_eq!(
        g.set_collidable("A", "Z", false),
        Err(GroupError::NotRegistered)
    );
    assert_eq!(g.unregister("Default"), Err(GroupError::Default));
    assert_eq!(g.rename("A", "Default"), Err(GroupError::Default));
    assert_eq!(g.register(""), Err(GroupError::EmptyName));
    assert_eq!(g.register(&"x".repeat(101)), Err(GroupError::LongName));
    g.rename("B", "C").unwrap();
    g.unregister("C").unwrap();
    assert_eq!(
        g.get("A").unwrap().mask,
        -1,
        "unregistering restores the bit"
    );
    // New groups take the lowest free id; 32 at most.
    g.register("D").unwrap();
    assert_eq!(g.id_of("D"), 2);
    for i in 0..29 {
        g.register(&format!("G{i}")).unwrap();
    }
    assert_eq!(g.register("H"), Err(GroupError::Full));

    // They live on Workspace.
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    g.write(&mut dom).unwrap();
    assert_eq!(CollisionGroups::read(&dom), g);
    assert!(matches!(
        dom.property_by_name(ws, "CollisionGroupData"),
        Some(Value::BinaryString(_))
    ));
}

fn touches_of(touches: &[Touch], began: bool) -> usize {
    touches.iter().filter(|t| t.began == began).count()
}

#[test]
fn dropped_parts_touch_the_floor() {
    let mut dom = Dom::new();
    let floor = part(
        &mut dom,
        "Part",
        Vec3::ZERO,
        Vec3::new(20.0, 1.0, 20.0),
        true,
    );
    let drop = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let ghost = part(
        &mut dom,
        "Part",
        Vec3::new(5.0, 5.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    set(&mut dom, ghost, "CanCollide", Value::Bool(false));
    let shy = part(
        &mut dom,
        "Part",
        Vec3::new(-5.0, 5.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    set(&mut dom, shy, "CanTouch", Value::Bool(false));
    let mut physics = Physics::new();
    let touches = run(&mut physics, &mut dom, 1.5);

    let with = |p| {
        touches
            .iter()
            .filter(|t| t.part == p || t.other == p)
            .copied()
            .collect::<Vec<_>>()
    };
    let first = with(drop)[0];
    assert_eq!((first.part, first.other, first.began), (floor, drop, true));
    // The non-colliding part falls through: in, then out.
    let g = with(ghost);
    assert_eq!((touches_of(&g, true), touches_of(&g, false)), (1, 1));
    assert!(position(&dom, ghost).y < -5.0);
    // CanTouch off: nothing.
    assert!(with(shy).is_empty());
    assert!(position(&dom, drop).y < 2.5);
}

#[test]
fn rebuilding_a_resting_body_fires_nothing() {
    let mut dom = Dom::new();
    part(
        &mut dom,
        "Part",
        Vec3::ZERO,
        Vec3::new(20.0, 1.0, 20.0),
        true,
    );
    let block = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 1.5, 0.0),
        Vec3::splat(2.0),
        false,
    );
    custom(&mut dom, block, 0.7, 0.3, 0.0, 1.0, 1.0);
    let mut physics = Physics::new();
    run(&mut physics, &mut dom, 1.0);
    // Changing the material rebuilds the body under the same contact.
    material(&mut dom, block, "Wood");
    let touches = run(&mut physics, &mut dom, 0.2);
    assert!(touches.is_empty(), "{touches:?}");
}

#[test]
fn groups_that_dont_collide_pass_through_but_still_touch() {
    let mut dom = Dom::new();
    let floor = part(
        &mut dom,
        "Part",
        Vec3::ZERO,
        Vec3::new(20.0, 1.0, 20.0),
        true,
    );
    let drop = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    let mut g = CollisionGroups::default();
    g.register("Ghosts").unwrap();
    g.set_collidable("Ghosts", "Default", false).unwrap();
    g.write(&mut dom).unwrap();
    set(
        &mut dom,
        drop,
        "CollisionGroup",
        Value::String("Ghosts".into()),
    );
    let mut physics = Physics::new();
    let touches = run(&mut physics, &mut dom, 1.0);
    assert!(position(&dom, drop).y < -5.0, "fell through");
    assert_eq!(touches_of(&touches, true), 1, "{touches:?}");
    let _ = floor;

    // With TouchesUseCollisionGroups they don't touch either.
    let mut dom = Dom::new();
    part(
        &mut dom,
        "Part",
        Vec3::ZERO,
        Vec3::new(20.0, 1.0, 20.0),
        true,
    );
    let drop = part(
        &mut dom,
        "Part",
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::splat(2.0),
        false,
    );
    g.write(&mut dom).unwrap();
    let ws = dom.find_service("Workspace").unwrap();
    set(&mut dom, ws, "TouchesUseCollisionGroups", Value::Bool(true));
    set(
        &mut dom,
        drop,
        "CollisionGroup",
        Value::String("Ghosts".into()),
    );
    let mut physics = Physics::new();
    let touches = run(&mut physics, &mut dom, 1.0);
    assert!(touches.is_empty(), "{touches:?}");
}

/// A rubber block at rest on a rubber ramp at 30°, which holds it.
fn block_on_ramp(dom: &mut Dom) -> (InstanceId, InstanceId) {
    let tilt = CFrame::from_position(Vec3::new(0.0, 30.0, 0.0))
        * CFrame::angles(0.0, 0.0, 30f32.to_radians());
    let ramp = part(dom, "Part", Vec3::ZERO, Vec3::new(20.0, 1.0, 20.0), true);
    set(dom, ramp, "CFrame", Value::CFrame(tilt));
    let block = part(dom, "Part", Vec3::ZERO, Vec3::splat(2.0), false);
    let on_top = tilt * CFrame::from_position(Vec3::new(0.0, 1.5, 0.0));
    set(dom, block, "CFrame", Value::CFrame(on_top));
    material(dom, ramp, "Rubber");
    material(dom, block, "Rubber");
    (ramp, block)
}

#[test]
fn friction_changes_act_at_once_on_resting_parts() {
    // Observed in Studio 0.742: a part at rest on a 30° ramp slid as soon
    // as its friction (or, here, the ramp's) was set to 0. The density
    // stays the same, so only the friction changes.
    for change_ramp in [false, true] {
        let mut dom = Dom::new();
        let (ramp, block) = block_on_ramp(&mut dom);
        let mut physics = Physics::new();
        run(&mut physics, &mut dom, 3.0);
        let rested = position(&dom, block);
        run(&mut physics, &mut dom, 0.5);
        assert!(
            (position(&dom, block) - rested).length() < 0.01,
            "rubber holds"
        );
        let root = physics.root_of(block);
        let density = materials::of_part(&dom, block).density;
        custom(
            &mut dom,
            if change_ramp { ramp } else { block },
            density,
            0.0,
            0.5,
            100.0,
            1.0,
        );
        run(&mut physics, &mut dom, 0.5);
        assert!(
            (position(&dom, block) - rested).length() > 1.0,
            "slides once friction is 0 (ramp changed: {change_ramp})"
        );
        assert_eq!(physics.root_of(block), root);
    }
}
