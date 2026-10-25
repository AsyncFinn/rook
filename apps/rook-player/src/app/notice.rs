//! What the window says while no place is open: how to open one, or why
//! the last one didn't. Rook's own GUI, a ScreenGui built in code and laid
//! out by rook-gui, so it looks like the rest of Rook's drawing.

use std::collections::HashMap;

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_gui::Gui;
use rook_types::{Color3, UDim2, Vector2};

use crate::*;

/// Any camera does: the backdrop covers the whole frame.
pub(crate) const CAMERA: Camera = Camera {
    position: Vec3::ZERO,
    rotation: glam::Mat3::IDENTITY,
    fov_y: 70.0,
};

pub(crate) struct Notice {
    dom: Dom,
    gui: Gui,
    camera: InstanceId,
    /// The GUI generation the renderer has.
    drawn: Option<u64>,
}

impl Notice {
    /// The window's message before any place is open.
    pub(crate) fn drop_here(content: &Arc<Resolver>) -> Self {
        Self::new(content, "Drop a place file here")
    }

    /// Why a place didn't open: the first line of the error says what
    /// failed, the rest why, then where the log is.
    pub(crate) fn failed(content: &Arc<Resolver>, e: &anyhow::Error) -> Self {
        let mut text = e.to_string();
        for cause in e.chain().skip(1) {
            text.push_str(&format!("\n{cause}"));
        }
        if let Some(log) = rook_engine::logging::log_path() {
            text.push_str(&format!("\n\nThe log has more: {}", log.display()));
        }
        text.push_str("\n\nDrop a place file here to try another.");
        Self::new(content, &text)
    }

    fn new(content: &Arc<Resolver>, text: &str) -> Self {
        let mut dom = Dom::new();
        let workspace = service(&mut dom, "Workspace");
        let camera = child(&mut dom, "Camera", workspace, &[]);
        set(
            &mut dom,
            workspace,
            "CurrentCamera",
            Value::Ref(Some(camera.to_ref())),
        );
        // rook-gui draws the local player's screens, as a client's.
        let players = service(&mut dom, "Players");
        let player = child(&mut dom, "Player", players, &[]);
        set(
            &mut dom,
            players,
            "LocalPlayer",
            Value::Ref(Some(player.to_ref())),
        );
        let player_gui = child(&mut dom, "PlayerGui", player, &[]);
        let screen = child(&mut dom, "ScreenGui", player_gui, &[]);
        // Over the top bar's strip too, as IgnoreGuiInset does.
        set_enum(&mut dom, screen, "ScreenInsets", "DeviceSafeInsets");
        let backdrop = child(
            &mut dom,
            "Frame",
            screen,
            &[
                ("Size", udim2(1.0, 0, 1.0, 0)),
                // The loading screen's gray.
                ("BackgroundColor3", color(0.11, 0.115, 0.13)),
                ("BorderSizePixel", Value::Int32(0)),
            ],
        );
        let mut label = vec![
            ("Position", udim2(0.1, 0, 0.1, 0)),
            ("Size", udim2(0.8, 0, 0.8, 0)),
            ("BackgroundTransparency", Value::Float32(1.0)),
            ("Text", Value::String(text.to_owned())),
            ("TextColor3", color(0.95, 0.95, 0.95)),
            ("TextSize", Value::Float32(24.0)),
            ("TextWrapped", Value::Bool(true)),
        ];
        if let Some(font) = rook_types::font_from_enum("BuilderSans") {
            label.push(("FontFace", Value::Font(font)));
        }
        child(&mut dom, "TextLabel", backdrop, &label);
        let mut gui = Gui::default();
        gui.set_content(content.clone());
        Self {
            dom,
            gui,
            camera,
            drawn: None,
        }
    }

    /// Give the renderer the message, laid out to fill a `width` x `height`
    /// frame.
    pub(crate) fn show(&mut self, renderer: &mut Renderer, width: u32, height: u32) {
        let size = Vec2::new(width as f32, height as f32);
        if self
            .dom
            .vector2_property(self.camera, "ViewportSize")
            .map(|v| v.0)
            != Some(size)
        {
            let size = Value::Vector2(Vector2(size));
            set(&mut self.dom, self.camera, "ViewportSize", size);
        }
        self.gui.sync(&mut self.dom);
        if self.drawn != Some(self.gui.generation()) {
            self.drawn = Some(self.gui.generation());
            renderer.set_gui(&self.gui.draw(&self.dom), &[], &HashMap::new());
        }
    }
}

fn service(dom: &mut Dom, class: &str) -> InstanceId {
    dom.get_service(class).expect("a built-in service")
}

fn child(dom: &mut Dom, class: &str, parent: InstanceId, props: &[(&str, Value)]) -> InstanceId {
    let id = dom.create_by_name(class).expect("a built-in class");
    for (name, value) in props {
        set(dom, id, name, value.clone());
    }
    dom.set_parent(id, Some(parent)).expect("a fresh instance");
    id
}

fn set(dom: &mut Dom, id: InstanceId, name: &str, value: Value) {
    dom.set_property_by_name(id, name, value)
        .expect("a property of the class");
}

fn udim2(xs: f32, xo: i32, ys: f32, yo: i32) -> Value {
    Value::UDim2(UDim2::new(xs, xo, ys, yo))
}

fn color(r: f32, g: f32, b: f32) -> Value {
    Value::Color3(Color3::new(r, g, b))
}

fn set_enum(dom: &mut Dom, id: InstanceId, name: &str, item: &str) {
    let class = dom.get(id).expect("a live instance").class;
    let property = dom
        .database()
        .find_canonical_property(class, name)
        .expect("a property of the class");
    let rook_reflection::PropertyType::Enum(e) = property.ty else {
        unreachable!("{name} is an enum");
    };
    let value = rook_reflection::enum_item_value(e, item).expect("an item of the enum");
    set(dom, id, name, Value::Enum(value));
}
