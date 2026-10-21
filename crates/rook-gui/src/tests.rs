//! Layouts checked against values measured in Studio 0.742's client, in
//! a 1561 × 793 window (docs/behavior/gui/layout.md).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::{UDim, UDim2, Value, Vector2};

use super::*;

mod draw;
mod layouts;
mod world;

fn set(dom: &mut Dom, id: InstanceId, name: &str, value: Value) {
    dom.set_property_by_name(id, name, value).unwrap();
}

fn v2(x: f32, y: f32) -> Value {
    Value::Vector2(Vector2(Vec2::new(x, y)))
}

fn udim2(xs: f32, xo: i32, ys: f32, yo: i32) -> Value {
    Value::UDim2(UDim2::new(xs, xo, ys, yo))
}

fn udim(scale: f32, offset: i32) -> Value {
    Value::UDim(UDim::new(scale, offset))
}

fn set_enum(dom: &mut Dom, id: InstanceId, property: &str, item: &str) {
    let class = dom.get(id).unwrap().class;
    let p = dom
        .database()
        .find_canonical_property(class, property)
        .unwrap();
    let rook_reflection::PropertyType::Enum(e) = p.ty else {
        panic!("{property} isn't an enum");
    };
    let value = rook_reflection::enum_item_value(e, item).unwrap();
    set(dom, id, property, Value::Enum(value));
}

/// A new `class` under `parent`, with `props` set first.
fn child(dom: &mut Dom, class: &str, parent: InstanceId, props: &[(&str, Value)]) -> InstanceId {
    let id = dom.create_by_name(class).unwrap();
    for (name, value) in props {
        set(dom, id, name, value.clone());
    }
    dom.set_parent(id, Some(parent)).unwrap();
    id
}

/// A client's DataModel with a 1561 × 793 window and the local player's
/// PlayerGui.
fn client() -> (Dom, InstanceId) {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let camera = child(
        &mut dom,
        "Camera",
        ws,
        &[("ViewportSize", v2(1561.0, 793.0))],
    );
    set(
        &mut dom,
        ws,
        "CurrentCamera",
        Value::Ref(Some(camera.to_ref())),
    );
    let players = dom.get_service("Players").unwrap();
    let player = child(&mut dom, "Player", players, &[]);
    set(
        &mut dom,
        players,
        "LocalPlayer",
        Value::Ref(Some(player.to_ref())),
    );
    let player_gui = child(&mut dom, "PlayerGui", player, &[]);
    (dom, player_gui)
}

/// [`client`] with a ScreenGui holding a 400 × 300 Frame at (10, 20), the
/// box most cases lay out in.
fn boxed() -> (Dom, InstanceId) {
    let (mut dom, player_gui) = client();
    let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
    let frame = frame(&mut dom, screen, udim2(0.0, 400, 0.0, 300));
    set(&mut dom, frame, "Position", udim2(0.0, 10, 0.0, 20));
    (dom, frame)
}

fn frame(dom: &mut Dom, parent: InstanceId, size: Value) -> InstanceId {
    child(dom, "Frame", parent, &[("Size", size)])
}

fn laid_out(dom: &mut Dom) -> Gui {
    let mut gui = Gui::default();
    gui.sync(dom);
    gui
}

fn vec2_of(dom: &Dom, id: InstanceId, name: &str) -> Vec2 {
    dom.vector2_property(id, name).unwrap().0
}

/// `id`'s `AbsolutePosition` from `origin`'s and its `AbsoluteSize`.
fn rect(dom: &Dom, id: InstanceId, origin: InstanceId) -> (Vec2, Vec2) {
    let at = vec2_of(dom, id, "AbsolutePosition") - vec2_of(dom, origin, "AbsolutePosition");
    (at, vec2_of(dom, id, "AbsoluteSize"))
}

#[track_caller]
fn near(got: (Vec2, Vec2), want: ([f32; 2], [f32; 2])) {
    let want = (Vec2::from(want.0), Vec2::from(want.1));
    assert!(
        got.0.abs_diff_eq(want.0, 1e-3) && got.1.abs_diff_eq(want.1, 1e-3),
        "got {got:?}, want {want:?}"
    );
}
