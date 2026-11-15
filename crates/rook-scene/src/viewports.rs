//! ViewportFrames' worlds (docs/behavior/gui/viewport-frames.md): which
//! world a part is drawn in, and how each frame sees and lights its own.

use glam::{Mat3, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_render::{Camera, Space, Viewport};

use crate::SceneCache;

/// Which world part `id` is drawn in: that of the nearest ViewportFrame
/// it's under, else Workspace's if it's in Workspace; `None` if neither,
/// or out of the tree.
pub(crate) fn space(dom: &Dom, id: InstanceId) -> Option<Space> {
    let mut space = None;
    let mut top = id;
    for a in dom.ancestors(id) {
        if space.is_none() {
            if is_frame(dom, a) {
                space = Some(Space::Viewport(key(a)));
            } else if dom.class_name(a) == "Workspace" {
                space = Some(Space::Workspace);
            }
        }
        top = a;
    }
    space.filter(|_| top == dom.root())
}

pub(crate) fn is_frame(dom: &Dom, id: InstanceId) -> bool {
    dom.class_name(id) == "ViewportFrame"
}

/// The key a frame's world and picture go by.
pub(crate) fn key(frame: InstanceId) -> u64 {
    frame.to_ref().0
}

/// How frame `id` sees and lights its world.
fn look(dom: &Dom, id: InstanceId) -> Viewport {
    let color = |name, default: [f32; 3]| {
        dom.color3_property(id, name)
            .map_or(default, rook_types::Color3::to_array)
    };
    let camera = dom.ref_property(id, "CurrentCamera").and_then(|c| {
        let cf = dom.cframe_property(c, "CFrame")?;
        Some(Camera {
            position: cf.position,
            rotation: Mat3::from_cols(
                cf.rotation.x_axis.normalize_or_zero(),
                cf.rotation.y_axis.normalize_or_zero(),
                cf.rotation.z_axis.normalize_or_zero(),
            ),
            fov_y: dom.float_property(c, "FieldOfView").unwrap_or(70.0),
            focus: None,
        })
    });
    // The property keeps its value when set to zero (`rook-lua`), so a
    // zero one is only a broken file's.
    let direction = dom
        .vector3_property(id, "LightDirection")
        .unwrap_or(Vec3::new(-1.0, -1.0, -1.0));
    let opacity = 1.0
        - dom
            .float_property(id, "BackgroundTransparency")
            .unwrap_or(0.0);
    let [r, g, b] = color("BackgroundColor3", [1.0; 3]);
    Viewport {
        camera,
        ambient: color("Ambient", [200.0 / 255.0; 3]),
        light_color: color("LightColor", [140.0 / 255.0; 3]),
        towards_light: (-direction).normalize_or(Vec3::ONE.normalize()),
        background: [r, g, b, opacity.clamp(0.0, 1.0)],
    }
}

impl SceneCache {
    /// Read every ViewportFrame in the tree again: the frames.
    pub(crate) fn build_viewports(&mut self, dom: &Dom) -> Vec<InstanceId> {
        self.scene.viewports.clear();
        let mut frames = dom.descendants(dom.root());
        frames.retain(|&f| is_frame(dom, f));
        for &frame in &frames {
            self.scene.viewports.insert(key(frame), look(dom, frame));
        }
        self.viewports_changed.clear();
        frames
    }

    /// Read frames `ids` again (each a ViewportFrame or gone from the
    /// tree), noting those that changed for the renderer.
    pub(crate) fn update_viewports(
        &mut self,
        dom: &Dom,
        ids: impl IntoIterator<Item = InstanceId>,
    ) {
        for id in ids {
            let key = key(id);
            let look =
                (dom.get(id).is_some() && is_frame(dom, id) && dom.is_ancestor_of(dom.root(), id))
                    .then(|| look(dom, id));
            let was = match look {
                Some(look) => self.scene.viewports.insert(key, look),
                None => self.scene.viewports.remove(&key),
            };
            if was != look && !self.viewports_changed.contains(&key) {
                self.viewports_changed.push(key);
            }
        }
    }

    /// Forget freed frames `ids`, without reading them.
    pub(crate) fn drop_viewports(&mut self, ids: impl IntoIterator<Item = InstanceId>) {
        for id in ids {
            let key = key(id);
            if self.scene.viewports.remove(&key).is_some() && !self.viewports_changed.contains(&key)
            {
                self.viewports_changed.push(key);
            }
        }
    }

    /// The frames whose looks changed since last asked.
    pub(crate) fn take_viewport_changes(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.viewports_changed)
    }
}
