//! Unions collide and are queried as their saved geometry, on the server
//! and on the client whose world the mouse queries.

use super::*;

use rook_types::{CFrame, Hull, SharedString, Vector3};

/// Two 2-stud blocks with a 2-stud gap between them along X, as a 6 × 2 ×
/// 2 union's saved `PhysicalConfigData`.
fn gap_blob() -> Vec<u8> {
    let cuboid = |x0: f32, x1: f32| Hull {
        positions: (0..8)
            .map(|i| {
                [
                    if i & 1 == 0 { x0 } else { x1 },
                    if i & 2 == 0 { -1.0 } else { 1.0 },
                    if i & 4 == 0 { -1.0 } else { 1.0 },
                ]
            })
            .collect(),
        indices: Vec::new(),
    };
    rook_content::mesh::union::physics::encode(&[cuboid(-3.0, -1.0), cuboid(1.0, 3.0)])
}

fn add_union(server: &Server, fidelity: u32) {
    let ws = service(server, "Workspace");
    let mut w = server.runtime.world_mut();
    let dom = &mut w.dom;
    let id = dom.create_by_name("UnionOperation").unwrap();
    let size = Value::Vector3(Vector3(glam::Vec3::new(6.0, 2.0, 2.0)));
    for (name, value) in [
        (
            "CFrame",
            Value::CFrame(CFrame::from_position(glam::Vec3::new(0.0, 50.0, 0.0))),
        ),
        ("Size", size.clone()),
        ("InitialSize", size),
        ("Anchored", Value::Bool(true)),
        ("CollisionFidelity", Value::Enum(fidelity)),
        (
            "PhysicalConfigData",
            Value::SharedString(SharedString::new(gap_blob())),
        ),
    ] {
        dom.set_property_by_name(id, name, value).unwrap();
    }
    dom.set_parent(id, Some(ws)).unwrap();
}

/// Whether a ray down the middle of the union, through its gap, hits it.
fn hits_gap(dm: &DataModel) -> bool {
    let mut w = dm.runtime.world_mut();
    let w = &mut *w;
    let filter = rook_physics::Filter::legacy_ray(Vec::new(), false);
    let origin = glam::Vec3::new(0.0, 50.0, 10.0);
    w.physics
        .raycast(
            &mut w.dom,
            origin,
            glam::Vec3::new(0.0, 0.0, -20.0),
            &filter,
        )
        .is_some()
}

#[test]
fn rays_pass_through_saved_union_gaps_on_server_and_client() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_union(&server, 0);
    server.start();
    assert!(!hits_gap(&server), "server, before the first frame");
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player1", 1).unwrap();
    session.run_for(0.2);
    assert!(!hits_gap(session.server()), "server");
    assert!(!hits_gap(session.client(index).unwrap()), "client");
}

/// `CollisionFidelity` Box: the union is its box, whatever it saved.
#[test]
fn box_fidelity_unions_are_boxes() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_union(&server, 2);
    server.start();
    assert!(hits_gap(&server));
}
