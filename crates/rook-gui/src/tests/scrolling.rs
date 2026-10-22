//! ScrollingFrames, checked against values read in Studio 0.743
//! (docs/behavior/gui/scrolling.md): a 200 × 100 frame at (10, 10) in the
//! 400 × 300 box, with no border unless a case gives one.

use super::*;

/// A ScrollingFrame in the box with `props` set, and a child at (10, 20)
/// half the canvas's size.
fn scroller(props: &[(&str, Value)]) -> (Dom, InstanceId, InstanceId) {
    let (mut dom, frame) = boxed();
    let mut all = vec![
        ("Size", udim2(0.0, 200, 0.0, 100)),
        ("Position", udim2(0.0, 10, 0.0, 10)),
        ("BorderSizePixel", Value::Int32(0)),
    ];
    all.extend(props.iter().cloned());
    let f = child(&mut dom, "ScrollingFrame", frame, &all);
    let c = child(
        &mut dom,
        "Frame",
        f,
        &[
            ("Position", udim2(0.0, 10, 0.0, 20)),
            ("Size", udim2(0.5, 0, 0.5, 0)),
        ],
    );
    (dom, f, c)
}

fn canvas(x: i32, y: i32) -> (&'static str, Value) {
    ("CanvasSize", udim2(0.0, x, 0.0, y))
}

/// `AbsoluteWindowSize` and `AbsoluteCanvasSize`.
fn sizes(dom: &Dom, f: InstanceId) -> (Vec2, Vec2) {
    (
        vec2_of(dom, f, "AbsoluteWindowSize"),
        vec2_of(dom, f, "AbsoluteCanvasSize"),
    )
}

#[track_caller]
fn check(props: &[(&str, Value)], window: [f32; 2], canvas: [f32; 2]) {
    let (mut dom, f, _) = scroller(props);
    laid_out(&mut dom);
    near(sizes(&dom, f), (window, canvas));
}

#[test]
fn bars_take_their_thickness_from_the_window() {
    check(&[canvas(300, 400)], [188.0, 88.0], [300.0, 400.0]);
    check(&[canvas(100, 400)], [188.0, 100.0], [200.0, 400.0]);
    check(&[canvas(300, 50)], [200.0, 88.0], [300.0, 100.0]);
    check(&[canvas(50, 50)], [200.0, 100.0], [200.0, 100.0]);
    // The vertical bar's room makes the horizontal one show.
    check(&[canvas(195, 400)], [188.0, 88.0], [200.0, 400.0]);
    check(
        &[canvas(300, 400), ("ScrollBarThickness", Value::Int32(30))],
        [170.0, 70.0],
        [300.0, 400.0],
    );
}

#[test]
fn insets_and_directions() {
    let (mut dom, f, c) = scroller(&[canvas(50, 50)]);
    set_enum(&mut dom, f, "VerticalScrollBarInset", "Always");
    set_enum(&mut dom, f, "HorizontalScrollBarInset", "Always");
    laid_out(&mut dom);
    near(sizes(&dom, f), ([200.0, 100.0], [188.0, 88.0]));
    near(rect(&dom, c, f), ([10.0, 20.0], [94.0, 44.0]));

    let (mut dom, f, _) = scroller(&[canvas(100, 400)]);
    set_enum(&mut dom, f, "VerticalScrollBarInset", "ScrollBar");
    laid_out(&mut dom);
    near(sizes(&dom, f), ([188.0, 100.0], [188.0, 400.0]));

    let (mut dom, f, _) = scroller(&[canvas(300, 400)]);
    set_enum(&mut dom, f, "ScrollingDirection", "X");
    laid_out(&mut dom);
    near(sizes(&dom, f), ([200.0, 88.0], [300.0, 400.0]));
}

#[test]
fn canvas_size_scales_with_the_parent() {
    // The default {0, 0}, {2, 0}: twice the 300-high box.
    check(&[], [188.0, 100.0], [200.0, 600.0]);
}

#[test]
fn borders_part_the_bars_from_the_window() {
    check(
        &[canvas(400, 600), ("BorderSizePixel", Value::Int32(3))],
        [185.0, 85.0],
        [400.0, 600.0],
    );
    let (mut dom, f, c) = scroller(&[canvas(400, 600), ("BorderSizePixel", Value::Int32(4))]);
    set_enum(&mut dom, f, "BorderMode", "Inset");
    laid_out(&mut dom);
    near(sizes(&dom, f), ([184.0, 84.0], [400.0, 600.0]));
    near(rect(&dom, c, f), ([14.0, 24.0], [196.0, 296.0]));
}

#[test]
fn canvas_position_is_kept_within_the_canvas() {
    let at = |x, y| ("CanvasPosition", v2(x, y));
    let (mut dom, f, c) = scroller(&[canvas(300, 400), at(1000.0, 1000.0)]);
    laid_out(&mut dom);
    assert_eq!(vec2_of(&dom, f, "CanvasPosition"), Vec2::new(112.0, 312.0));
    near(rect(&dom, c, f), ([-102.0, -292.0], [150.0, 200.0]));
    // Children land on whole pixels.
    let (mut dom, f, c) = scroller(&[canvas(300, 400), at(10.6, 20.25)]);
    laid_out(&mut dom);
    near(rect(&dom, c, f), ([-1.0, 0.0], [150.0, 200.0]));
    // Scripts' writes are kept within the canvas at once.
    let (mut dom, f, _) = scroller(&[canvas(300, 400)]);
    let mut gui = laid_out(&mut dom);
    gui.set_canvas_position(&mut dom, f, Vec2::new(-50.0, 500.0))
        .unwrap();
    assert_eq!(vec2_of(&dom, f, "CanvasPosition"), Vec2::new(0.0, 312.0));
}

#[test]
fn a_left_bar_moves_an_inset_canvas() {
    let (mut dom, f, c) = scroller(&[canvas(100, 400)]);
    set_enum(&mut dom, f, "VerticalScrollBarPosition", "Left");
    set_enum(&mut dom, f, "VerticalScrollBarInset", "ScrollBar");
    laid_out(&mut dom);
    near(rect(&dom, c, f), ([22.0, 20.0], [94.0, 200.0]));
}

#[test]
fn automatic_canvas_size_holds_what_is_inside() {
    let (mut dom, f, c) = scroller(&[canvas(0, 0)]);
    set_enum(&mut dom, f, "AutomaticCanvasSize", "Y");
    set(&mut dom, c, "Size", udim2(0.0, 50, 0.0, 50));
    set(&mut dom, c, "Position", udim2(0.0, 0, 0.0, 300));
    let deep = frame(&mut dom, c, udim2(0.0, 50, 0.0, 50));
    set(&mut dom, deep, "Position", udim2(0.0, 0, 0.0, 400));
    let padding = child(
        &mut dom,
        "UIPadding",
        f,
        &[
            ("PaddingTop", udim(0.0, 10)),
            ("PaddingBottom", udim(0.0, 20)),
        ],
    );
    laid_out(&mut dom);
    // A grandchild counts, with the padding: 10 + 300 + 400 + 50 + 20.
    near(sizes(&dom, f), ([188.0, 100.0], [200.0, 780.0]));
    dom.destroy(padding).unwrap();
    dom.destroy(deep).unwrap();
    set(&mut dom, f, "CanvasSize", udim2(0.0, 0, 0.0, 500));
    laid_out(&mut dom);
    near(sizes(&dom, f), ([188.0, 100.0], [200.0, 500.0]));
}

#[test]
fn automatic_canvas_size_with_a_list() {
    let (mut dom, f, c) = scroller(&[canvas(0, 0)]);
    dom.destroy(c).unwrap();
    set_enum(&mut dom, f, "AutomaticCanvasSize", "Y");
    child(&mut dom, "UIListLayout", f, &[("Padding", udim(0.0, 5))]);
    for _ in 0..5 {
        frame(&mut dom, f, udim2(0.0, 150, 0.0, 60));
    }
    laid_out(&mut dom);
    near(sizes(&dom, f), ([188.0, 100.0], [200.0, 320.0]));
}
