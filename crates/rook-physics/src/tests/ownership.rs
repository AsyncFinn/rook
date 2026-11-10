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

/// A server frame's updates write back only the assemblies they moved:
/// the others' parts already show where they are.
#[test]
fn an_update_writes_back_only_what_it_moved() {
    use rook_types::counters::{Counter, take_local};
    let mut dom = Dom::new();
    let a = block(&mut dom, Vec3::new(0.0, 20.0, 0.0), false);
    let b = block(&mut dom, Vec3::new(10.0, 20.0, 0.0), false);
    let mut physics = Physics::with_authority(Authority::Only(Vec::new()));
    physics.sync(&mut dom);
    let put = |dom: &mut Dom, id, y: f32| {
        let at = CFrame::from_position(Vec3::new(position(dom, id).x, y, 0.0));
        set(dom, id, "CFrame", Value::CFrame(at));
    };
    put(&mut dom, a, 19.0);
    put(&mut dom, b, 19.0);
    physics.receive_updates(&mut dom, 1.0);
    put(&mut dom, a, 18.0);
    take_local();
    physics.receive_updates(&mut dom, 1.1);
    let touched = take_local()[Counter::PhysicsBodiesTouched];
    assert_eq!(touched, 1, "a alone");
}

/// A played assembly nothing simulated is near sleeps in Rapier and is
/// put in place once a frame, one beside the client's own moves at every
/// step, and the far one ends each frame where the near one does: the
/// same pose, velocity and drawn pose, 64 studs away (far enough, near
/// enough for the numbers to round alike).
#[test]
fn far_played_assemblies_end_each_frame_where_near_ones_do() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    set(&mut dom, ws, "Gravity", Value::Float32(0.0));
    let (model, mine) = owned_block(&mut dom, Vec3::new(0.0, 20.0, 0.0));
    set(&mut dom, mine, "AssemblyLinearVelocity", v3(1.0, 0.0, 0.0));
    let away = Vec3::new(0.0, 0.0, 64.0);
    let near = block(&mut dom, Vec3::new(0.0, 20.0, 4.0), false);
    let far = block(&mut dom, Vec3::new(0.0, 20.0, 4.0) + away, false);
    let mut physics = Physics::with_authority(Authority::Only(vec![model]));
    physics.sync(&mut dom);
    let (mut client, mut server) = (0.0, 0.0);
    let mut i = 0;
    while client < 2.0 {
        let dt = if i % 2 == 0 { 1.0 / 144.0 } else { 1.0 / 45.0 };
        i += 1;
        client += dt;
        while server < client {
            server += 1.0 / 30.0;
            // Along a curve, turning as it goes.
            let s = server as f32;
            let at = CFrame::from_position(Vec3::new(3.0 * s, 20.0 + s.sin(), 4.0))
                * CFrame::angles(0.0, s, 0.0);
            let v = v3(3.0, s.cos(), 0.0);
            for (id, shift) in [(near, Vec3::ZERO), (far, away)] {
                let cf = CFrame::from_position(shift) * at;
                set(&mut dom, id, "CFrame", Value::CFrame(cf));
                set(&mut dom, id, "AssemblyLinearVelocity", v.clone());
            }
            physics.receive_updates(&mut dom, server);
        }
        physics.sync(&mut dom);
        rook_types::counters::take_local();
        physics.step(dt);
        let moves =
            rook_types::counters::take_local()[rook_types::counters::Counter::PhysicsPlayedMoves];
        if i % 2 == 0 && i > 2 {
            // 5 or 6 steps: each for the near one, one move for the far one.
            assert!((6..=7).contains(&moves), "frame {i}: {moves}");
            let active: Vec<_> = physics.world.islands.active_bodies().collect();
            let body = |id| physics.body_of(id).unwrap();
            assert!(!active.contains(&body(far)), "frame {i}: far one stepped");
            assert!(active.contains(&body(near)), "frame {i}: near one asleep");
        }
        physics.write_back(&mut dom);
        let drawn: Vec<(InstanceId, CFrame)> = physics.drawn_poses().collect();
        let drawn_at = |id| {
            drawn
                .iter()
                .find(|(p, _)| *p == id)
                .map(|(_, cf)| cf.position)
        };
        let close = |a: Vec3, b: Vec3| (a - b).length() < 1e-3;
        assert!(
            close(position(&dom, far) - away, position(&dom, near)),
            "frame {i}"
        );
        let velocity = |id| dom.vector3_property(id, "AssemblyLinearVelocity").unwrap();
        let (vf, vn) = (velocity(far), velocity(near));
        assert!((vf - vn).length() < 5e-3, "frame {i}: {vf} {vn}");
        match (drawn_at(far), drawn_at(near)) {
            (Some(f), Some(n)) => assert!(close(f - away, n), "frame {i}: {f} {n}"),
            (f, n) => assert_eq!(f.is_some(), n.is_some(), "frame {i}"),
        }
    }
    assert!(position(&dom, near).x > 3.0, "played");
    assert!(position(&dom, mine).x > 1.5, "simulated");
}
