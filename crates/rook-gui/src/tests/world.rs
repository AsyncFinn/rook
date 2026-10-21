use glam::Vec3;
use rook_types::{CFrame, Color3, WorldPlace};

use super::*;

/// [`client`] with an anchored part of `size` at `at` in the Workspace;
/// the camera sits at the origin looking down −Z.
fn world_part(size: Vec3, at: Vec3) -> (Dom, InstanceId) {
    let (mut dom, _) = client();
    let ws = dom.find_service("Workspace").unwrap();
    let part = child(
        &mut dom,
        "Part",
        ws,
        &[
            ("Size", Value::Vector3(rook_types::Vector3(size))),
            (
                "CFrame",
                Value::CFrame(CFrame {
                    position: at,
                    ..CFrame::default()
                }),
            ),
        ],
    );
    (dom, part)
}

fn surface(dom: &mut Dom, part: InstanceId, face: &str) -> InstanceId {
    let gui = child(dom, "SurfaceGui", part, &[]);
    set_enum(dom, gui, "Face", face);
    gui
}

#[track_caller]
fn near3(got: Vec3, want: [f32; 3]) {
    assert!(
        got.abs_diff_eq(want.into(), 1e-4),
        "got {got}, want {want:?}"
    );
}

#[test]
fn surface_canvas_sizes() {
    let (mut dom, part) = world_part(Vec3::new(4.0, 2.0, 1.0), Vec3::ZERO);
    let fixed = surface(&mut dom, part, "Front");
    let per_stud = surface(&mut dom, part, "Right");
    set_enum(&mut dom, per_stud, "SizingMode", "PixelsPerStud");
    set(&mut dom, per_stud, "PixelsPerStud", Value::Float32(25.0));
    laid_out(&mut dom);
    assert_eq!(
        vec2_of(&dom, fixed, "AbsoluteSize"),
        Vec2::new(800.0, 600.0)
    );
    // Measured: the Right face is 1 stud wide and 2 tall.
    assert_eq!(
        vec2_of(&dom, per_stud, "AbsoluteSize"),
        Vec2::new(25.0, 50.0)
    );
}

/// Measured on a cube with a SurfaceGui on every face: the canvas's top
/// left on the Front face is at +X +Y; on the Top it's at −X +Z, x running
/// along −Z.
#[test]
fn surface_canvases_on_faces() {
    let (mut dom, part) = world_part(Vec3::new(8.0, 6.0, 4.0), Vec3::new(0.0, 10.0, 0.0));
    let front = surface(&mut dom, part, "Front");
    let top = surface(&mut dom, part, "Top");
    let mut gui = laid_out(&mut dom);
    gui.draw_world(&dom);
    let drawn: Vec<InstanceId> = gui.world_drawn.iter().map(|h| h.gui).collect();
    assert_eq!(drawn, [front, top]);
    let places = gui.world_places(&dom);
    let [
        Some(WorldPlace::Surface {
            origin,
            right,
            down,
        }),
        Some(WorldPlace::Surface {
            origin: top_origin,
            right: top_right,
            down: top_down,
        }),
    ] = places[..]
    else {
        panic!("{places:?}");
    };
    // 800 × 600 over 8 × 6 studs.
    near3(origin, [4.0, 13.0, -2.0]);
    near3(right * 800.0, [-8.0, 0.0, 0.0]);
    near3(down * 600.0, [0.0, -6.0, 0.0]);
    near3(top_origin, [-4.0, 13.0, 2.0]);
    near3(top_right * 800.0, [0.0, 0.0, -4.0]);
    near3(top_down * 600.0, [8.0, 0.0, 0.0]);
}

/// Measured: `Size` scales are studs at the billboard's depth, offsets are
/// pixels; it lays out again as the camera moves.
#[test]
fn billboard_size_follows_depth() {
    let (mut dom, part) = world_part(Vec3::ONE, Vec3::new(0.0, 0.0, -40.0));
    let gui = child(
        &mut dom,
        "BillboardGui",
        part,
        &[("Size", udim2(4.0, 100, 2.0, 50))],
    );
    let camera = dom
        .ref_property(dom.find_service("Workspace").unwrap(), "CurrentCamera")
        .unwrap();
    let look_from = |dom: &mut Dom, z: f32| {
        let at = CFrame {
            position: Vec3::new(0.0, 0.0, z),
            ..CFrame::default()
        };
        set(dom, camera, "CFrame", Value::CFrame(at));
    };
    look_from(&mut dom, 0.0);
    set(&mut dom, camera, "FieldOfView", Value::Float32(70.0));
    let mut gui_layout = laid_out(&mut dom);
    // 793 pixels high at FieldOfView 70: 566.3 pixels per stud at 1.
    assert_eq!(vec2_of(&dom, gui, "AbsoluteSize"), Vec2::new(157.0, 78.0));
    look_from(&mut dom, -20.0);
    gui_layout.sync(&mut dom);
    assert_eq!(vec2_of(&dom, gui, "AbsoluteSize"), Vec2::new(213.0, 107.0));
}

/// Measured: with ClipsDescendants off what spills past the canvas is
/// drawn; hidden or disabled world GUIs and ones off the Workspace draw
/// nothing.
#[test]
fn what_world_gui_draws() {
    let (mut dom, part) = world_part(Vec3::new(8.0, 6.0, 1.0), Vec3::ZERO);
    let gui = surface(&mut dom, part, "Front");
    let spill = frame(&mut dom, gui, udim2(0.0, 300, 0.0, 300));
    set(&mut dom, spill, "Position", udim2(0.0, 700, 0.0, 500));
    set(&mut dom, spill, "BorderSizePixel", Value::Int32(0));
    set(
        &mut dom,
        spill,
        "BackgroundColor3",
        Value::Color3(Color3::new(0.0, 1.0, 0.0)),
    );
    let mut layout = laid_out(&mut dom);
    let canvases = layout.draw_world(&dom);
    assert_eq!(canvases.len(), 1);
    let spilled = &canvases[0].quads[0];
    assert_eq!(spilled.center, Vec2::new(850.0, 650.0));
    assert_eq!(spilled.clip, rook_types::GuiQuad::NO_CLIP);
    assert_eq!(canvases[0].max_distance, f32::INFINITY);

    set(&mut dom, gui, "ClipsDescendants", Value::Bool(true));
    layout.sync(&mut dom);
    let clipped = &layout.draw_world(&dom)[0].quads[0];
    assert_eq!(clipped.clip, [0.0, 0.0, 800.0, 600.0]);

    set(&mut dom, gui, "Enabled", Value::Bool(false));
    layout.sync(&mut dom);
    assert!(layout.draw_world(&dom).is_empty());
    set(&mut dom, gui, "Enabled", Value::Bool(true));
    let storage = dom.get_service("ReplicatedStorage").unwrap();
    dom.set_parent(part, Some(storage)).unwrap();
    layout.sync(&mut dom);
    assert!(layout.draw_world(&dom).is_empty());
}
