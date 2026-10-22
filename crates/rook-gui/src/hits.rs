//! What the cursor is over: the GUI objects at a screen point, top first,
//! only where they're drawn; and what the mouse's ray meets on the world
//! GUIs that take it (docs/behavior/gui/input.md#what-the-cursor-is-over).

use glam::{Vec2, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_types::Viewer;

use crate::Gui;
use crate::order::{self, Clip, Stacked};

/// The objects on a world GUI the mouse's ray meets.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldHit {
    /// The SurfaceGui or BillboardGui.
    pub gui: InstanceId,
    /// Along the ray, in studs.
    pub distance: f32,
    /// Drawn over the world, so nothing in front hides it.
    pub always_on_top: bool,
    /// Top first.
    pub objects: Vec<InstanceId>,
}

/// Whether `s` is drawn at `at`: inside its turned rectangle and what its
/// ancestors leave of it.
fn covers(s: &Stacked, at: Vec2) -> bool {
    let c = s.clip;
    if at.x < c[0] || at.y < c[1] || at.x > c[2] || at.y > c[3] {
        return false;
    }
    let local = s.placed.local_point(at);
    let size = s.placed.local;
    let (lo, hi) = (size.min(Vec2::ZERO), size.max(Vec2::ZERO));
    local.cmpge(lo).all() && local.cmple(hi).all()
}

/// The objects in `stack` at `at`, top first.
fn top_first(stack: &[Stacked], at: Vec2) -> impl Iterator<Item = InstanceId> + '_ {
    stack
        .iter()
        .rev()
        .filter(move |s| covers(s, at))
        .map(|s| s.id)
}

impl Gui {
    /// The local player's on-screen GUI objects at `at` (GUI
    /// coordinates, below the top bar), top first: what
    /// `GetGuiObjectsAtPosition` lists. Measured: as of the last frame's
    /// layout (a Frame parented just before isn't listed).
    pub fn objects_at(&self, dom: &Dom, at: Vec2) -> Vec<InstanceId> {
        let mut out = Vec::new();
        for screen in order::screens(dom).into_iter().rev() {
            let stack = order::stack(dom, &self.layout, screen, Clip::NONE);
            out.extend(top_first(&stack, at));
        }
        out
    }

    /// The nearest `Active` world GUI the ray from `origin` along `dir`
    /// (a unit vector) meets on an object, seen by `viewer`, as of the
    /// last layout.
    pub fn world_objects_at(
        &self,
        dom: &Dom,
        origin: Vec3,
        dir: Vec3,
        viewer: &Viewer,
    ) -> Option<WorldHit> {
        let mut best: Option<WorldHit> = None;
        for (hung, size) in &self.layout.world {
            let gui = hung.gui;
            if dom.bool_property(gui, "Active") != Some(true) || !hung.shown(dom) {
                continue;
            }
            let Some(frame) = hung.place(dom).and_then(|p| p.frame(*size, viewer)) else {
                continue;
            };
            let outward = frame.down.cross(frame.right);
            let facing = dir.dot(outward);
            if facing.abs() < 1e-6 || (frame.one_sided && facing > 0.0) {
                continue;
            }
            let distance = (frame.corner - origin).dot(outward) / facing;
            let limit = order::max_distance(dom, gui);
            if distance < 0.0 || best.as_ref().is_some_and(|b| b.distance <= distance) {
                continue;
            }
            let rel = origin + dir * distance - frame.corner;
            let at = Vec2::new(
                rel.dot(frame.right) / frame.right.length_squared(),
                rel.dot(frame.down) / frame.down.length_squared(),
            );
            let middle = frame.corner + (frame.right * size.x + frame.down * size.y) * 0.5;
            if middle.distance(viewer.position) > limit {
                continue;
            }
            let stack = order::stack(dom, &self.layout, gui, order::canvas_clip(dom, gui, *size));
            let objects: Vec<InstanceId> = top_first(&stack, at).collect();
            if !objects.is_empty() {
                best = Some(WorldHit {
                    gui,
                    distance,
                    always_on_top: dom.bool_property(gui, "AlwaysOnTop") == Some(true),
                    objects,
                });
            }
        }
        best
    }
}
