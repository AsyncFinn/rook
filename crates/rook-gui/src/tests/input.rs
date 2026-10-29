//! What input sees of the GUI: hit-testing, AutoButtonColor's tints, the
//! wheel and scroll bars (docs/behavior/gui/input.md).

use rook_types::Color3;

use super::*;

fn button(
    dom: &mut Dom,
    parent: InstanceId,
    at: (i32, i32),
    props: &[(&str, Value)],
) -> InstanceId {
    let id = child(
        dom,
        "TextButton",
        parent,
        &[
            ("Position", udim2(0.0, at.0, 0.0, at.1)),
            ("Size", udim2(0.0, 100, 0.0, 50)),
        ],
    );
    for (name, value) in props {
        set(dom, id, name, value.clone());
    }
    id
}

#[test]
fn hits_follow_the_drawing() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let low = button(&mut dom, screen, (50, 200), &[]);
    let top = button(&mut dom, screen, (50, 200), &[("ZIndex", Value::Int32(2))]);
    // Measured: a button turned 45° about (350, 425) takes a click inside
    // its turned shape, not one in its unturned corner.
    let turned = button(
        &mut dom,
        screen,
        (300, 400),
        &[("Rotation", Value::Float32(45.0))],
    );
    let clip = child(
        &mut dom,
        "Frame",
        screen,
        &[
            ("Position", udim2(0.0, 50, 0.0, 600)),
            ("Size", udim2(0.0, 100, 0.0, 50)),
            ("ClipsDescendants", Value::Bool(true)),
        ],
    );
    let clipped = button(&mut dom, clip, (50, 0), &[]);
    let off = child(
        &mut dom,
        "ScreenGui",
        player_gui,
        &[("Enabled", Value::Bool(false))],
    );
    button(&mut dom, off, (700, 200), &[]);
    let gui = laid_out(&mut dom);
    let at = |x, y| gui.objects_at(&dom, Vec2::new(x, y));
    assert_eq!(at(100.0, 225.0), [top, low]);
    assert_eq!(at(395.0, 405.0), []);
    assert_eq!(at(380.0, 455.0), [turned]);
    assert_eq!(at(125.0, 625.0), [clipped, clip]);
    assert_eq!(at(175.0, 625.0), []);
    assert_eq!(at(750.0, 225.0), []);
}

#[test]
fn auto_button_color_tints_the_background() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let color = Value::Color3(Color3::new(200.0 / 255.0, 100.0 / 255.0, 50.0 / 255.0));
    let b = button(
        &mut dom,
        screen,
        (0, 0),
        &[
            ("BackgroundColor3", color),
            ("BorderSizePixel", Value::Int32(0)),
        ],
    );
    let mut gui = laid_out(&mut dom);
    let mut drawn = |dom: &mut Dom, state: &str| {
        set_enum(dom, b, "GuiState", state);
        gui.sync(dom);
        let c = drawn(&mut gui, dom)[0].color;
        [c[0], c[1], c[2]].map(|v| (v * 255.0).round())
    };
    // Measured: (140, 70, 35) hovered and (216, 147, 110) pressed.
    assert_eq!(drawn(&mut dom, "Idle"), [200.0, 100.0, 50.0]);
    assert_eq!(drawn(&mut dom, "Hover"), [140.0, 70.0, 35.0]);
    assert_eq!(drawn(&mut dom, "Press"), [217.0, 147.0, 112.0]);
}

/// Ease `gui`'s wheel scrolls until they land, as the engine does.
fn settle(gui: &mut Gui, dom: &mut Dom) {
    for _ in 0..60 {
        for (id, at) in gui.ease_scrolls(dom) {
            set(dom, id, "CanvasPosition", v2(at.x, at.y));
        }
        gui.sync(dom);
    }
}

#[test]
fn the_wheel_scrolls_the_innermost_frame_that_can_move() {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let outer = child(
        &mut dom,
        "ScrollingFrame",
        screen,
        &[
            ("Position", udim2(0.0, 100, 0.0, 100)),
            ("Size", udim2(0.0, 200, 0.0, 200)),
            ("CanvasSize", udim2(0.0, 0, 0.0, 1000)),
        ],
    );
    let inner = child(
        &mut dom,
        "ScrollingFrame",
        outer,
        &[
            ("Position", udim2(0.0, 20, 0.0, 20)),
            ("Size", udim2(0.0, 100, 0.0, 100)),
            ("CanvasSize", udim2(0.0, 0, 0.0, 150)),
        ],
    );
    let mut gui = laid_out(&mut dom);
    let position = |dom: &Dom, id| vec2_of(dom, id, "CanvasPosition");
    let hits = gui.objects_at(&dom, Vec2::new(150.0, 150.0));
    assert_eq!(gui.wheel(&dom, &hits, -1.0), Some(inner));
    settle(&mut gui, &mut dom);
    assert_eq!(position(&dom, inner), Vec2::new(0.0, 50.0));
    // At its end, the next notch goes to the frame around it.
    assert_eq!(gui.wheel(&dom, &hits, -1.0), Some(outer));
    settle(&mut gui, &mut dom);
    assert_eq!(position(&dom, outer), Vec2::new(0.0, 100.0));

    // Measured: dragging the thumb 50 pixels down a 200-pixel window
    // over a 1000-pixel canvas moves it 250 more.
    let drag = gui
        .grab_bar(outer, Vec2::new(294.0, 125.0))
        .expect("on the thumb");
    let to = gui.drag_bar(&drag, Vec2::new(294.0, 175.0)).unwrap();
    assert_eq!(to, Vec2::new(0.0, 350.0));
}
