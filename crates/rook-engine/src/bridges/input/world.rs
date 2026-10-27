//! The mouse in the 3D world: the ray through the cursor, the Mouse's
//! `Hit`, `Target` and the rest, and the ClickDetector under it.

use glam::Vec3;
use rook_dom::InstanceId;
use rook_types::{CFrame, Value, Vector3};

use super::{ClickEvent, ClickKind, gui_point};
use crate::DataModel;

/// How far the mouse looks for a target (Inference).
pub(super) const MOUSE_RANGE: f32 = 1000.0;

/// Where the mouse looked from: everything its properties follow.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Look {
    camera: CFrame,
    dir: Vec3,
    cursor: (f32, f32),
    viewport: (u32, u32),
    target_filter: Option<InstanceId>,
    /// [`rook_physics::Physics::generation`].
    world: u64,
}

/// What the mouse last saw, and from where.
#[derive(Debug, Clone, Copy)]
pub(super) struct Under {
    look: Look,
    hit: Option<rook_physics::Hit>,
    click_hit: Option<rook_physics::Hit>,
}

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
    ///
    /// The mouse looks again, and writes what changed, only when the
    /// camera, the cursor, the window, its TargetFilter or the world
    /// queries see changed. Measured: Mouse
    /// fires no Changed for any of these (Studio 0.744, moving the cursor
    /// and the camera), so they're written without signals, as physics
    /// writes parts.
    pub(crate) fn refresh_mouse(&mut self, clicks: &mut Vec<ClickEvent>) -> Option<InstanceId> {
        let mouse = self.mouse_object()?;
        // The camera may have been replaced since the window last resized.
        self.sync_viewport_size();
        let (origin, dir, camera) = self.mouse_ray()?;
        let (hit, click_hit, looked) = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            let target_filter = w.dom.ref_property(mouse, "TargetFilter");
            w.physics.sync(&mut w.dom);
            let look = Look {
                camera,
                dir,
                cursor: w.input.mouse,
                viewport: self.input.viewport,
                target_filter,
                world: w.physics.generation(),
            };
            match self.input.under {
                Some(under) if under.look == look => (under.hit, under.click_hit, false),
                _ => {
                    #[cfg(test)]
                    {
                        self.input.looks += 1;
                    }
                    // TargetFilter and what it contains are ignored;
                    // otherwise the mouse sees what legacy rays see
                    // (Inference).
                    let filter = rook_physics::Filter::legacy_ray(
                        target_filter.into_iter().collect(),
                        false,
                    );
                    let hit = w
                        .physics
                        .raycast(&mut w.dom, origin, dir * MOUSE_RANGE, &filter);
                    // Clicks pass through invisible parts that don't
                    // collide; the same TargetFilter applying is Inference.
                    let clicks = rook_physics::Filter {
                        skip_invisible_ghosts: true,
                        ..filter
                    };
                    let click_hit =
                        w.physics
                            .raycast(&mut w.dom, origin, dir * MOUSE_RANGE, &clicks);
                    self.input.under = Some(Under {
                        look,
                        hit,
                        click_hit,
                    });
                    (hit, click_hit, true)
                }
            }
        };
        if looked {
            let hit = hit.map(|h| (h.part, h.position));
            self.write_mouse(mouse, (origin, dir, camera), hit);
        }

        // Measured: a detector's hover goes on under an Active GUI object.
        let from = self.character_position().unwrap_or(origin);
        let detector = click_hit.and_then(|h| self.detector_for(h.part, from.distance(h.position)));
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

    /// Write the Mouse's properties for the ray `(origin, dir)` from
    /// `camera` and what it hit; the DOM keeps values that didn't change.
    fn write_mouse(
        &self,
        mouse: InstanceId,
        (origin, dir, camera): (Vec3, Vec3, CFrame),
        hit: Option<(InstanceId, Vec3)>,
    ) {
        let point = hit.map_or(origin + dir * MOUSE_RANGE, |(_, at)| at);
        let mut w = self.runtime.world_mut();
        let (mx, my) = gui_point(w.input.mouse);
        let (vw, vh) = self.input.viewport;
        let vh = (vh as f32 - rook_gui::GUI_INSET).max(0.0);
        let hit_cf = CFrame {
            position: point,
            rotation: camera.rotation,
        };
        let ray = rook_types::Ray::new(Vector3(origin).into(), Vector3(dir).into());
        let values = [
            ("Hit", Value::CFrame(hit_cf)),
            ("Origin", Value::CFrame(camera)),
            ("Target", Value::Ref(hit.map(|(part, _)| part.to_ref()))),
            ("X", Value::Int32(mx as i32)),
            ("Y", Value::Int32(my as i32)),
            ("ViewSizeX", Value::Int32(vw as i32)),
            ("ViewSizeY", Value::Int32(vh as i32)),
            ("UnitRay", Value::Ray(ray)),
        ];
        for (name, value) in values {
            if let Err(e) = w.dom.set_property_by_name(mouse, name, value) {
                tracing::warn!("setting Mouse.{name} failed: {e}");
            }
        }
    }

    /// Where the local player's character is: its root part.
    fn character_position(&self) -> Option<glam::Vec3> {
        let w = self.runtime.world();
        let dom = &w.dom;
        let root = rook_lua::character_root(dom, dom.local_player()?)?;
        Some(dom.cframe_property(root, "CFrame")?.position)
    }

    /// The ClickDetector that handles clicks on `part`: one inside the part,
    /// or inside the nearest Model containing it, if within its
    /// MaxActivationDistance, from the character (creator-docs
    /// ClickDetector.MaxActivationDistance), or the camera without one.
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
