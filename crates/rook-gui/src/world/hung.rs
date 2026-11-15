//! Where a world canvas hangs: the part or attachment it's on, the eye it's
//! seen from, and the spot that puts it.

use super::*;

/// The camera world GUI is seen from: `Workspace.CurrentCamera`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Eye {
    pub(super) position: Vec3,
    pub(super) rotation: Mat3,
    /// Pixels a stud covers one stud in front of the camera.
    pub(super) pixels_per_stud: f32,
}

impl Eye {
    pub(crate) fn of(dom: &Dom) -> Option<Self> {
        let camera = dom
            .find_service("Workspace")
            .and_then(|ws| dom.ref_property(ws, "CurrentCamera"))?;
        let frame = dom.drawn_cframe(camera)?;
        let fov = dom.float_property(camera, "FieldOfView").unwrap_or(70.0);
        let height = dom.vector2_property(camera, "ViewportSize")?.0.y;
        Some(Self {
            position: frame.position,
            rotation: frame.rotation,
            pixels_per_stud: height / (2.0 * (fov.to_radians() * 0.5).tan()),
        })
    }

    pub(super) fn depth(&self, point: Vec3) -> f32 {
        (point - self.position).dot(-self.rotation.z_axis)
    }
}

/// Where a world GUI's adornee is now: its frame, and its size (zero for
/// an Attachment).
#[derive(Debug, Clone, Copy)]
pub(super) struct Spot {
    pub(super) frame: CFrame,
    pub(super) size: Vec3,
}

/// A world GUI's settings, read once per layout (changing one lays GUI
/// out again), so following its adornee each frame reads only where that
/// is.
#[derive(Debug, Clone)]
pub(crate) struct Hung {
    pub(crate) gui: InstanceId,
    /// Its `Adornee`, or else its parent.
    pub(super) on: Option<InstanceId>,
    pub(super) how: How,
}

#[derive(Debug, Clone)]
pub(super) enum How {
    Surface(Surface),
    Billboard(Billboard),
}

impl Hung {
    pub(crate) fn of(dom: &Dom, gui: InstanceId) -> Self {
        let how = match dom.class_name(gui) {
            "SurfaceGui" => How::Surface(Surface::of(dom, gui)),
            _ => How::Billboard(Billboard::of(dom, gui)),
        };
        Self {
            gui,
            on: dom.ref_property(gui, "Adornee").or_else(|| dom.parent(gui)),
            how,
        }
    }

    /// Where its adornee is: a BasePart, or for a BillboardGui an
    /// Attachment in one.
    pub(super) fn spot(&self, dom: &Dom) -> Option<Spot> {
        let on = self.on?;
        if dom.is_a(on, "BasePart") {
            return Some(Spot {
                frame: dom.drawn_cframe(on)?,
                size: dom.vector3_property(on, "Size").unwrap_or(Vec3::ZERO),
            });
        }
        if !matches!(self.how, How::Billboard(_)) || dom.class_name(on) != "Attachment" {
            return None;
        }
        let part = dom.parent(on).filter(|&p| dom.is_a(p, "BasePart"))?;
        let local = dom.cframe_property(on, "CFrame").unwrap_or_default();
        Some(Spot {
            frame: dom.drawn_cframe(part)? * local,
            size: Vec3::ZERO,
        })
    }

    /// Its canvas size in pixels: its `AbsoluteSize`.
    pub(crate) fn canvas_size(&self, dom: &Dom, eye: Option<&Eye>) -> Vec2 {
        let spot = self.spot(dom);
        match &self.how {
            How::Surface(s) => s.canvas_size(spot.map(|s| s.size)),
            How::Billboard(b) => b.pixel_size(spot.as_ref(), eye),
        }
    }

    /// Whether its canvas size can change with nothing in the GUI
    /// changing: as the camera or its part moves, or its part resizes.
    pub(crate) fn follows(&self) -> bool {
        match &self.how {
            How::Surface(s) => s.per_stud(),
            How::Billboard(b) => b.in_studs(),
        }
    }

    /// Whether it hangs on what `other` does.
    pub(crate) fn same_hosts(&self, other: &Hung) -> bool {
        self.on == other.on
    }

    /// What it moves with: its adornee, and an Attachment's part.
    pub(crate) fn hosts(&self, dom: &Dom) -> impl Iterator<Item = InstanceId> {
        let part = self.on.filter(|&on| dom.class_name(on) == "Attachment");
        self.on.into_iter().chain(part.and_then(|a| dom.parent(a)))
    }

    pub(crate) fn place(&self, dom: &Dom) -> Option<WorldPlace> {
        let spot = self.spot(dom)?;
        match &self.how {
            How::Surface(s) => s.place(&spot),
            How::Billboard(b) => Some(b.place(&spot)),
        }
    }

    /// Whether it draws in the local player's world: Enabled, in the
    /// Workspace or the local player's PlayerGui, on something in the
    /// Workspace it can be drawn on (Inference: Studio draws neither kind
    /// from StarterGui or storage).
    pub(crate) fn shown(&self, dom: &Dom) -> bool {
        let gui = self.gui;
        if dom.get(gui).is_none() || dom.bool_property(gui, "Enabled") == Some(false) {
            return false;
        }
        let player = dom.local_player();
        if player.is_some() && dom.ref_property(gui, "PlayerToHideFrom") == player {
            return false;
        }
        let workspace = dom.find_service("Workspace");
        let in_workspace = |id| workspace.is_some_and(|ws| dom.is_ancestor_of(ws, id));
        held_where_drawn(dom, gui) && self.on.is_some_and(in_workspace) && self.place(dom).is_some()
    }
}
