//! The mouse in the 3D world: the ray through the cursor, the Mouse's
//! `Hit`, `Target` and the rest, and the ClickDetector under it.

use glam::Vec3;
use rook_dom::InstanceId;
use rook_types::{CFrame, Value, Vector3};

use super::{ClickEvent, ClickKind, gui_point};
use crate::DataModel;

/// How far the mouse looks for a target (Inference).
pub(super) const MOUSE_RANGE: f32 = 1000.0;

impl DataModel {
    /// The ray from the camera through the cursor (unit direction), the
    /// same as `Camera:ViewportPointToRay` but starting at the camera.
    pub(super) fn mouse_ray(&self) -> Option<(Vec3, Vec3, CFrame)> {
        let camera = self.current_camera()?;
        let w = self.runtime.world();
        let (vw, vh) = self.input.viewport;
        let lens = rook_lua::Lens {
            viewport: glam::Vec2::new(vw as f32, vh as f32),
            ..rook_lua::Lens::of(&w.dom, camera)
        };
        let (mx, my) = w.input.mouse;
        let (_, dir) = rook_lua::viewport_ray(&lens, mx, my);
        Some((lens.cframe.position, dir, lens.cframe))
    }

    /// Update Mouse.Hit, Target and the rest from the cursor and camera,
    /// and track which ClickDetector is under the cursor. Returns it.
    pub(crate) fn refresh_mouse(&mut self, clicks: &mut Vec<ClickEvent>) -> Option<InstanceId> {
        let mouse = self.mouse_object()?;
        // The camera may have been replaced since the window last resized.
        self.sync_viewport_size();
        let (origin, dir, camera) = self.mouse_ray()?;
        let (hit, click_hit) = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            // TargetFilter and what it contains are ignored; otherwise the
            // mouse sees what legacy rays see (Inference).
            let ignore = w
                .dom
                .ref_property(mouse, "TargetFilter")
                .into_iter()
                .collect();
            let filter = rook_physics::Filter::legacy_ray(ignore, false);
            let hit = w
                .physics
                .raycast(&mut w.dom, origin, dir * MOUSE_RANGE, &filter);
            // Clicks pass through invisible parts that don't collide; the
            // same TargetFilter applying is Inference.
            let clicks = rook_physics::Filter {
                skip_invisible_ghosts: true,
                ..filter
            };
            let click_hit = w
                .physics
                .raycast(&mut w.dom, origin, dir * MOUSE_RANGE, &clicks);
            (hit, click_hit)
        };
        let point = hit.map_or(origin + dir * MOUSE_RANGE, |h| h.position);
        let (mx, my) = gui_point(self.runtime.world().input.mouse);
        let (vw, vh) = self.input.viewport;
        let vh = (vh as f32 - rook_gui::GUI_INSET).max(0.0);
        let hit_cf = CFrame {
            position: point,
            rotation: camera.rotation,
        };
        self.set_property(mouse, "Hit", Value::CFrame(hit_cf));
        self.set_property(mouse, "Origin", Value::CFrame(camera));
        self.set_property(mouse, "Target", Value::Ref(hit.map(|h| h.part.to_ref())));
        self.set_property(mouse, "X", Value::Int32(mx as i32));
        self.set_property(mouse, "Y", Value::Int32(my as i32));
        self.set_property(mouse, "ViewSizeX", Value::Int32(vw as i32));
        self.set_property(mouse, "ViewSizeY", Value::Int32(vh as i32));
        self.set_property(
            mouse,
            "UnitRay",
            Value::Ray(rook_types::Ray::new(
                Vector3(origin).into(),
                Vector3(dir).into(),
            )),
        );

        // Measured: a detector's hover goes on under an Active GUI object.
        let detector =
            click_hit.and_then(|h| self.detector_for(h.part, origin.distance(h.position)));
        if detector != self.input.hover {
            if let Some(old) = self.input.hover {
                clicks.push(ClickEvent {
                    detector: old,
                    kind: ClickKind::HoverLeave,
                });
            }
            if let Some(new) = detector {
                clicks.push(ClickEvent {
                    detector: new,
                    kind: ClickKind::HoverEnter,
                });
            }
            self.input.hover = detector;
        }
        detector
    }

    /// The ClickDetector that handles clicks on `part`: one inside the part,
    /// or inside the nearest Model containing it, if within its
    /// MaxActivationDistance. Distance is measured from the camera until
    /// characters exist (Inference; Roblox measures from the character).
    fn detector_for(&self, part: InstanceId, distance: f32) -> Option<InstanceId> {
        let w = self.runtime.world();
        let dom = &w.dom;
        let find = |owner: InstanceId| dom.find_first_child_of_class(owner, "ClickDetector");
        let detector = find(part).or_else(|| {
            dom.ancestors(part)
                .filter(|a| dom.is_a(*a, "Model"))
                .find_map(find)
        })?;
        let max = dom
            .float_property(detector, "MaxActivationDistance")
            .unwrap_or(32.0);
        (distance <= max).then_some(detector)
    }
}
