use rook_types::{Color3, GuiQuad};

use super::*;

/// A borderless 200 × 200 Frame at (600, 300) with this ZIndex, coloured
/// by its index so draws can be told apart.
fn square(dom: &mut Dom, parent: InstanceId, z: i32, tag: f32) -> InstanceId {
    child(
        dom,
        "Frame",
        parent,
        &[
            ("Size", udim2(0.0, 200, 0.0, 200)),
            ("Position", udim2(0.0, 600, 0.0, 300)),
            ("ZIndex", Value::Int32(z)),
            ("BorderSizePixel", Value::Int32(0)),
            (
                "BackgroundColor3",
                Value::Color3(Color3::new(tag, 0.0, 0.0)),
            ),
        ],
    )
}

fn order(dom: &mut Dom) -> Vec<f32> {
    let gui = laid_out(dom);
    gui.draw(dom).iter().map(|q| q.color[0]).collect()
}

#[test]
fn z_index_behaviours_and_display_order() {
    let (mut dom, player_gui) = client();
    let s1 = child(&mut dom, "ScreenGui", player_gui, &[]);
    let p = square(&mut dom, s1, 5, 0.1);
    let q = square(&mut dom, s1, 3, 0.2);
    let pc = square(&mut dom, p, 1, 0.3);
    set(&mut dom, pc, "Position", udim2(0.0, 0, 0.0, 0));
    let qc = square(&mut dom, q, 10, 0.4);
    set(&mut dom, qc, "Position", udim2(0.0, 0, 0.0, 0));
    // Bottom first: the reverse of GetGuiObjectsAtPosition's lists.
    set_enum(&mut dom, s1, "ZIndexBehavior", "Global");
    assert_eq!(order(&mut dom), [0.3, 0.2, 0.1, 0.4]);
    set_enum(&mut dom, s1, "ZIndexBehavior", "Sibling");
    assert_eq!(order(&mut dom), [0.2, 0.4, 0.1, 0.3]);
    set(&mut dom, q, "ZIndex", Value::Int32(5));
    assert_eq!(order(&mut dom), [0.1, 0.3, 0.2, 0.4]);

    let s2 = child(&mut dom, "ScreenGui", player_gui, &[]);
    square(&mut dom, s2, 0, 0.5);
    assert_eq!(order(&mut dom).last(), Some(&0.5));
    set(&mut dom, s1, "DisplayOrder", Value::Int32(1));
    assert_eq!(order(&mut dom).first(), Some(&0.5));
}

#[test]
fn clipping_and_hidden_objects() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let p = square(&mut dom, screen, 1, 0.1);
    set(&mut dom, p, "ClipsDescendants", Value::Bool(true));
    let c = square(&mut dom, p, 1, 0.2);
    set(&mut dom, c, "Position", udim2(0.0, 150, 0.0, 0));
    let hidden = square(&mut dom, screen, 1, 0.3);
    set(&mut dom, hidden, "Visible", Value::Bool(false));
    square(&mut dom, hidden, 1, 0.4);
    let gui = laid_out(&mut dom);
    let quads = gui.draw(&dom);
    assert_eq!(quads.len(), 2);
    assert_eq!(quads[0].clip, GuiQuad::NO_CLIP);
    assert_eq!(quads[1].clip, [600.0, 358.0, 800.0, 558.0]);

    // A turned clipper clips nothing (creator-docs ClipsDescendants).
    set(&mut dom, p, "Rotation", Value::Float32(10.0));
    let gui = laid_out(&mut dom);
    assert_eq!(gui.draw(&dom)[1].clip, GuiQuad::NO_CLIP);
}

#[test]
fn borders_go_around_the_background() {
    let (mut dom, player_gui) = client();
    let screen = child(
        &mut dom,
        "ScreenGui",
        player_gui,
        &[("IgnoreGuiInset", Value::Bool(true))],
    );
    let f = square(&mut dom, screen, 1, 0.1);
    set(&mut dom, f, "BorderSizePixel", Value::Int32(2));
    set(&mut dom, f, "BackgroundTransparency", Value::Float32(0.25));
    let gui = laid_out(&mut dom);
    let quads = gui.draw(&dom);
    // Four strips outside a 200 × 200 background, all 75% opaque.
    assert_eq!(quads.len(), 5);
    assert_eq!(quads[0].center, Vec2::new(700.0, 299.0));
    assert_eq!(quads[0].size, Vec2::new(204.0, 2.0));
    assert_eq!(quads[2].size, Vec2::new(2.0, 200.0));
    assert_eq!(quads[4].size, Vec2::new(200.0, 200.0));
    assert!(quads.iter().all(|q| q.color[3] == 0.75));
    set_enum(&mut dom, f, "BorderMode", "Inset");
    let gui = laid_out(&mut dom);
    assert_eq!(gui.draw(&dom)[4].size, Vec2::new(196.0, 196.0));
}
