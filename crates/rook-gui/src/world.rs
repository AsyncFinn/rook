//! World GUI: SurfaceGuis laid on a part's face, BillboardGuis facing
//! the camera, and the names and health bars over Humanoids' heads. Which are shown, their canvas sizes, and where they are each
//! frame; their contents are laid out and drawn like a ScreenGui's
//! (docs/behavior/gui/world.md).

mod billboard;
pub(crate) mod overhead;
mod surface;

use crate::*;
use glam::{Mat3, Vec2, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, WorldPlace};

use billboard::Billboard;
use surface::Surface;

pub(crate) fn is_world_gui(dom: &Dom, id: InstanceId) -> bool {
    matches!(dom.class_name(id), "SurfaceGui" | "BillboardGui")
}

/// Whether `gui` is somewhere a client draws world GUI from: Workspace or
/// the local player's PlayerGui, however deep (measured: one inside a
/// ScreenGui there draws).
pub(crate) fn held_where_drawn(dom: &Dom, gui: InstanceId) -> bool {
    let in_workspace = dom
        .find_service("Workspace")
        .is_some_and(|ws| dom.is_ancestor_of(ws, gui));
    in_workspace
        || (dom.local_player())
            .and_then(|p| dom.find_first_child_of_class(p, "PlayerGui"))
            .is_some_and(|pg| dom.is_ancestor_of(pg, gui))
}

/// The camera world GUI is seen from: `Workspace.CurrentCamera`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Eye {
    position: Vec3,
    rotation: Mat3,
    /// Pixels a stud covers one stud in front of the camera.
    pixels_per_stud: f32,
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

    fn depth(&self, point: Vec3) -> f32 {
        (point - self.position).dot(-self.rotation.z_axis)
    }
}

/// Where a world GUI's adornee is now: its frame, and its size (zero for
/// an Attachment).
#[derive(Debug, Clone, Copy)]
struct Spot {
    frame: CFrame,
    size: Vec3,
}

/// A world GUI's settings, read once per layout (changing one lays GUI
/// out again), so following its adornee each frame reads only where that
/// is.
#[derive(Debug, Clone)]
pub(crate) struct Hung {
    pub(crate) gui: InstanceId,
    /// Its `Adornee`, or else its parent.
    on: Option<InstanceId>,
    how: How,
}

#[derive(Debug, Clone)]
enum How {
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
    fn spot(&self, dom: &Dom) -> Option<Spot> {
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

/// The world GUIs among `laid` (every world GUI laid out) that draw.
pub(crate) fn drawn<'a>(dom: &Dom, laid: impl Iterator<Item = &'a Hung>) -> Vec<Hung> {
    laid.filter(|h| h.shown(dom)).cloned().collect()
}

impl Gui {
    /// What the shown SurfaceGuis and BillboardGuis draw, as of the last
    /// layout, then the tags over heads; [`Gui::world_places`] says where
    /// each is. The whole list, kept up to date by [`Gui::update_world`].
    pub fn draw_world(&mut self, dom: &Dom) -> Vec<Arc<WorldCanvas>> {
        self.update_world(dom, false);
        (self.world_drawn.iter())
            .map(|h| self.canvases[&h.gui].clone())
            .chain(self.tags_drawn.iter().cloned())
            .collect()
    }

    /// What changed in [`Gui::draw_world`]'s list since last time (the
    /// whole list when `all`): `None` when nothing did. Only canvases laid
    /// out or restyled since they were last drawn, and tags whose look
    /// changed, are drawn again, and which GUIs draw is worked out again
    /// only when that can have changed; otherwise only the GUIs that
    /// changed are given, by index.
    pub fn update_world(&mut self, dom: &Dom, all: bool) -> Option<WorldCanvases> {
        let mut relisted = self.world_list_drawn != Some(self.world_listed);
        let mut rehung: Vec<InstanceId> = self.world_rehung.drain().collect();
        rehung.sort_unstable();
        if !relisted {
            // Read again for the GUIs that changed alone; any that starts or
            // stops drawing, or hangs on something else, lists them all.
            for gui in rehung {
                let laid = self.layout.world.get(&gui).map(|(h, _)| h);
                match (self.world_index.get(&gui), laid) {
                    (Some(&i), Some(h)) if h.shown(dom) && h.same_hosts(&self.world_drawn[i]) => {
                        self.world_drawn[i] = h.clone();
                    }
                    (None, Some(h)) if !h.shown(dom) => {}
                    (None, None) => {}
                    _ => {
                        relisted = true;
                        break;
                    }
                }
            }
        }
        if relisted {
            self.world_list_drawn = Some(self.world_listed);
            let laid = self.layout.world.values().map(|(h, _)| h);
            self.world_drawn = world::drawn(dom, laid);
            self.world_hosts.clear();
            self.world_index.clear();
            for (i, hung) in self.world_drawn.iter().enumerate() {
                self.world_index.insert(hung.gui, i);
                for host in hung.hosts(dom) {
                    self.world_hosts.entry(host).or_default().push(i);
                }
            }
            self.world_fresh = None;
        }
        rook_types::gauge!(GuiCanvases, self.world_drawn.len());
        let stale: Vec<InstanceId> = self.world_stale.drain().collect();
        let mut fresh: Vec<usize> = if relisted {
            (self.world_drawn.iter().enumerate())
                .filter(|(_, h)| !self.canvases.contains_key(&h.gui))
                .map(|(i, _)| i)
                .collect()
        } else {
            (stale.iter())
                .filter_map(|g| self.world_index.get(g).copied())
                .filter(|&i| !self.canvases.contains_key(&self.world_drawn[i].gui))
                .collect()
        };
        fresh.sort_unstable();
        let missing: Vec<InstanceId> = fresh.iter().map(|&i| self.world_drawn[i].gui).collect();
        let mut text = draw::Text {
            fonts: &mut self.fonts,
            glyphs: &mut self.glyphs,
        };
        let drawn = draw::world(
            dom,
            &self.layout,
            &missing,
            &mut text,
            &mut self.images,
            self.focus,
        );
        let drawn: Vec<Arc<WorldCanvas>> = drawn.into_iter().map(Arc::new).collect();
        self.canvases
            .extend(missing.iter().copied().zip(drawn.iter().cloned()));
        if let Some(waiting) = &mut self.world_fresh {
            waiting.extend(&fresh);
        }
        // The tags, drawn again when one's look changed.
        let world_len = self.world_drawn.len();
        let mut changed: Vec<(usize, Arc<WorldCanvas>)> = Vec::new();
        let old_tags = self.tags_drawn.len();
        if self.tags_drawn_at != Some(self.tag_changes) {
            self.tags_drawn_at = Some(self.tag_changes);
            let mut kept = std::mem::take(&mut self.tags);
            let mut tags = Vec::with_capacity(self.overheads.shown.len());
            for tag in &self.overheads.shown {
                let canvas = match kept.remove(&tag.humanoid) {
                    Some((look, canvas)) if look == tag.look => canvas,
                    _ => draw::overhead::canvas(&tag.look, &mut text),
                };
                self.tags
                    .insert(tag.humanoid, (tag.look.clone(), canvas.clone()));
                tags.push(canvas);
            }
            for (j, canvas) in tags.iter().enumerate() {
                if self
                    .tags_drawn
                    .get(j)
                    .is_none_or(|c| !Arc::ptr_eq(c, canvas))
                {
                    changed.push((world_len + j, canvas.clone()));
                }
            }
            self.tags_drawn = tags;
        }
        if relisted || all {
            let list = (self.world_drawn.iter())
                .map(|h| self.canvases[&h.gui].clone())
                .chain(self.tags_drawn.iter().cloned())
                .collect();
            return Some(WorldCanvases::List(list));
        }
        let len = world_len + self.tags_drawn.len();
        let mut canvases: Vec<(usize, Arc<WorldCanvas>)> = fresh.into_iter().zip(drawn).collect();
        canvases.extend(changed);
        if canvases.is_empty() && self.tags_drawn.len() == old_tags {
            return None;
        }
        Some(WorldCanvases::Changed { len, canvases })
    }

    /// Where each canvas the last [`Gui::draw_world`] gave is now (`None`:
    /// it can't be drawn, its part gone).
    pub fn world_places(&self, dom: &Dom) -> Vec<Option<WorldPlace>> {
        rook_types::count!(GuiCanvasesPlaced, self.world_drawn.len());
        (self.world_drawn.iter().map(|h| h.place(dom)))
            .chain(self.tag_places())
            .collect()
    }

    /// Where the canvases the last [`Gui::draw_world`] gave whose adornee
    /// is among `moved` are now, by index in its list, then every tag over
    /// a head (each follows its Humanoid every frame). `moved` names what
    /// may be somewhere new: parts drawn somewhere new, resized or gone,
    /// and Attachments changed or moved in the tree.
    pub fn world_places_moved(
        &mut self,
        dom: &Dom,
        moved: impl IntoIterator<Item = InstanceId>,
    ) -> Vec<(usize, Option<WorldPlace>)> {
        self.places_at(dom, moved, Vec::new())
    }

    /// The places a renderer needs after [`Gui::draw_world`], by index in
    /// its list: every canvas when the list changed since they were last
    /// asked for (or with `all`, for a renderer that has none of them),
    /// else those drawn anew since and those [`Gui::world_places_moved`]
    /// gives. A canvas kept from the list before keeps its place.
    pub fn world_places_drawn(
        &mut self,
        dom: &Dom,
        moved: impl IntoIterator<Item = InstanceId>,
        all: bool,
    ) -> Vec<(usize, Option<WorldPlace>)> {
        match self.world_fresh.replace(Vec::new()).filter(|_| !all) {
            Some(fresh) => self.places_at(dom, moved, fresh),
            None => self.world_places(dom).into_iter().enumerate().collect(),
        }
    }

    /// [`Gui::world_places_moved`], and the canvases at `hit` too.
    pub(crate) fn places_at(
        &mut self,
        dom: &Dom,
        moved: impl IntoIterator<Item = InstanceId>,
        mut hit: Vec<usize>,
    ) -> Vec<(usize, Option<WorldPlace>)> {
        for id in moved {
            let Some(on) = self.world_hosts.get(&id) else {
                continue;
            };
            hit.extend(on);
            // An Attachment put in another part: its GUIs move with that
            // one from now on (the old part's entry only places them
            // again for nothing until the next draw).
            if dom.class_name(id) == "Attachment"
                && let Some(part) = dom.parent(id)
            {
                let on = on.clone();
                let list = self.world_hosts.entry(part).or_default();
                for i in on {
                    if !list.contains(&i) {
                        list.push(i);
                    }
                }
            }
        }
        hit.sort_unstable();
        hit.dedup();
        rook_types::count!(GuiCanvasesPlaced, hit.len());
        let tags = self.world_drawn.len();
        let world = hit.into_iter().map(|i| (i, self.world_drawn[i].place(dom)));
        world
            .chain((self.tag_places()).enumerate().map(|(i, p)| (tags + i, p)))
            .collect()
    }

    pub(crate) fn tag_places(&self) -> impl Iterator<Item = Option<WorldPlace>> + '_ {
        (self.overheads.shown.iter()).map(|tag| Some(tag.place(draw::overhead::size(&tag.look))))
    }
}
