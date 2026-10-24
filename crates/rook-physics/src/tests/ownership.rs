//! Assemblies another machine simulates ([`Authority`]).

use super::*;

/// A Model in Workspace holding one unanchored block at `at`.
fn owned_block(dom: &mut Dom, at: Vec3) -> (InstanceId, InstanceId) {
    let ws = dom.get_service("Workspace").unwrap();
    let model = child(dom, "Model", ws, &[]);
    let p = block(dom, at, false);
    dom.set_parent(p, Some(model)).unwrap();
    (model, p)
}

#[test]
fn a_server_leaves_client_owned_assemblies_where_the_client_puts_them() {
    let mut dom = Dom::new();
    let (model, p) = owned_block(&mut dom, Vec3::new(0.0, 20.0, 0.0));
    let free = block(&mut dom, Vec3::new(10.0, 20.0, 0.0), false);
    let mut physics = Physics::with_authority(Authority::Except(vec![model]));
    run(&mut physics, &mut dom, 0.5);
    assert_eq!(position(&dom, p).y, 20.0);
    assert!(
        position(&dom, free).y < 10.0,
        "the server simulates the rest"
    );
    assert!(!physics.simulates(p) && physics.simulates(free));

    // The client's position arrives, and the server's touches see it there.
    let pad = part(
        &mut dom,
        Vec3::new(0.0, 5.0, 0.0),
        Vec3::new(6.0, 1.0, 6.0),
        true,
    );
    run(&mut physics, &mut dom, 0.1);
    set(
        &mut dom,
        p,
        "CFrame",
        Value::CFrame(CFrame::from_position(Vec3::new(0.0, 6.0, 0.0))),
    );
    physics.sync(&mut dom);
    physics.step(1.0 / 60.0);
    physics.write_back(&mut dom);
    let touches = physics.take_touches();
    assert!(
        touches
            .iter()
            .any(|t| t.began && [t.part, t.other].contains(&p) && [t.part, t.other].contains(&pad)),
        "{touches:?}"
    );
    assert_eq!(position(&dom, p).y, 6.0);
}

#[test]
fn a_client_simulates_only_what_it_owns() {
    let mut dom = Dom::new();
    let (model, p) = owned_block(&mut dom, Vec3::new(0.0, 20.0, 0.0));
    let other = block(&mut dom, Vec3::new(10.0, 20.0, 0.0), false);
    let mut physics = Physics::with_authority(Authority::Only(Vec::new()));
    run(&mut physics, &mut dom, 0.25);
    assert_eq!(position(&dom, p).y, 20.0, "nothing is owned yet");

    // The character becomes the client's: it falls, the server's part doesn't.
    physics.set_authority(&dom, Authority::Only(vec![model]));
    run(&mut physics, &mut dom, 0.5);
    assert!(position(&dom, p).y < 10.0);
    assert_eq!(position(&dom, other).y, 20.0);
}
