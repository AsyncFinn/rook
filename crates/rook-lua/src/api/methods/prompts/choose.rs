//! Which prompts a client shows, and which show their indicator
//! (measured in Studio 0.744, docs/behavior/gui/proximity-prompts.md#which-show).
//!
//! A prompt can show when it's `Enabled`, in Workspace under a part, an
//! Attachment or a Model with a `PrimaryPart`, within its
//! `MaxActivationDistance` of the local character's root part, on screen
//! and, with `RequiresLineOfSight`, seen from the camera. Among those,
//! the ones nearest a point `GuiInset` below the viewport's center win:
//! `AlwaysShow` ones up to `MaxPromptsVisible`, one `OneGlobally` prompt,
//! and one `OnePerButton` prompt per key.

use glam::{Vec2, Vec3};
use rook_dom::{Dom, InstanceId};
use rustc_hash::FxHashSet;

use crate::api::methods::camera::{Lens, project};
use crate::world::World;

/// A prompt near enough to show it or its indicator.
#[derive(Debug, Clone, Copy)]
pub(super) struct Candidate {
    id: InstanceId,
    /// Pixels from the point prompts are nearest to.
    score: f32,
    /// Studs from the character's root part: the nearer of two at the
    /// same point wins (measured).
    distance: f32,
    at: Vec3,
    /// The part the prompt is placed by, which can't hide it.
    part: Option<InstanceId>,
    in_reach: bool,
    indicator_reach: bool,
    exclusivity: Exclusivity,
    /// `KeyboardKeyCode`, which `OnePerButton` prompts share.
    key: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exclusivity {
    OnePerButton,
    OneGlobally,
    AlwaysShow,
}

impl Exclusivity {
    fn of(dom: &Dom, prompt: InstanceId) -> Self {
        match dom.enum_item_property(prompt, "Exclusivity") {
            Some("OneGlobally") => Self::OneGlobally,
            Some("AlwaysShow") => Self::AlwaysShow,
            _ => Self::OnePerButton,
        }
    }
}

/// The service's limits and switch, at Roblox's defaults without one.
struct Limits {
    enabled: bool,
    prompts: usize,
    indicators: usize,
}

fn limits(dom: &Dom) -> Limits {
    let Some(s) = super::service(dom) else {
        return Limits {
            enabled: true,
            prompts: 16,
            indicators: 16,
        };
    };
    let count = |name| {
        dom.number_property(s, name)
            .map_or(16, |n| n.max(0.0) as usize)
    };
    Limits {
        enabled: dom.bool_property(s, "Enabled") != Some(false),
        prompts: count("MaxPromptsVisible"),
        indicators: count("MaxIndicatorsVisible"),
    }
}

pub(super) fn in_workspace(dom: &Dom, prompt: InstanceId) -> bool {
    dom.find_service("Workspace")
        .is_some_and(|ws| dom.is_ancestor_of(ws, prompt))
}

/// Where a prompt is, and the part it's placed by: its parent part's
/// position, its Attachment's world position, or its Model's
/// `PrimaryPart`'s (measured: not a part's pivot, and a Model without a
/// `PrimaryPart`, or any other parent, shows nothing).
fn placement(dom: &Dom, prompt: InstanceId) -> Option<(Vec3, Option<InstanceId>)> {
    let parent = dom.parent(prompt)?;
    let at = |part: InstanceId| Some(dom.cframe_property(part, "CFrame")?.position);
    if dom.is_a(parent, "BasePart") {
        return Some((at(parent)?, Some(parent)));
    }
    if dom.is_a(parent, "Attachment") {
        let local = dom.cframe_property(parent, "CFrame")?;
        let part = dom.parent(parent).filter(|p| dom.is_a(*p, "BasePart"));
        let frame = part.and_then(|p| dom.cframe_property(p, "CFrame"));
        let world = frame.map_or(local, |f| f * local);
        return Some((world.position, part));
    }
    if dom.is_a(parent, "Model") {
        let primary = dom.ref_property(parent, "PrimaryPart")?;
        return Some((at(primary)?, Some(primary)));
    }
    None
}

/// The prompts and indicators that show now, each list in the order they
/// would be shown.
pub(super) fn visible(w: &mut World) -> (Vec<InstanceId>, Vec<InstanceId>) {
    let mut candidates = std::mem::take(&mut w.prompts.scratch);
    candidates.clear();
    let chosen = collect(w, &mut candidates).map(|limits| pick(w, &candidates, &limits));
    w.prompts.scratch = candidates;
    chosen.unwrap_or_default()
}

/// Gather the prompts near enough, best first. `None` when none can show
/// (the service is off, or there's no character or camera).
fn collect(w: &World, out: &mut Vec<Candidate>) -> Option<Limits> {
    let dom = &w.dom;
    let limits = limits(dom);
    if !limits.enabled {
        return None;
    }
    let root = crate::character_root(dom, dom.local_player()?)?;
    let from = dom.cframe_property(root, "CFrame")?.position;
    let workspace = dom.find_service("Workspace")?;
    let lens = Lens::of(dom, dom.ref_property(workspace, "CurrentCamera")?);
    // Measured: screen points (the viewport's less the inset) must be on
    // the viewport, and the best is nearest its center: a point the inset
    // below the viewport's.
    let inset = Vec2::new(0.0, rook_gui::GUI_INSET);
    let center = lens.viewport * 0.5;
    for id in dom.each_of(super::prompt_class()) {
        if dom.bool_property(id, "Enabled") == Some(false) || !in_workspace(dom, id) {
            continue;
        }
        let Some((at, part)) = placement(dom, id) else {
            continue;
        };
        let distance = at.distance(from);
        let reach = |name| distance <= dom.float_property(id, name).unwrap_or(0.0);
        let (in_reach, indicator_reach) = (
            reach("MaxActivationDistance"),
            reach("MaxIndicatorDistance"),
        );
        if !in_reach && !indicator_reach {
            continue;
        }
        let (p, _) = project(&lens, at);
        let screen = Vec2::new(p.x, p.y) - inset;
        let on_screen =
            p.z > 0.0 && screen.cmpge(Vec2::ZERO).all() && screen.cmple(lens.viewport).all();
        if !on_screen {
            continue;
        }
        out.push(Candidate {
            id,
            score: screen.distance(center),
            distance,
            at,
            part,
            in_reach,
            indicator_reach,
            exclusivity: Exclusivity::of(dom, id),
            key: dom.enum_property(id, "KeyboardKeyCode"),
        });
    }
    out.sort_by(|a, b| {
        (a.score.total_cmp(&b.score))
            .then(a.distance.total_cmp(&b.distance))
            .then(a.id.cmp(&b.id))
    });
    Some(limits)
}

/// Choose from `candidates` (best first): `AlwaysShow` prompts, then the
/// `OneGlobally` one, then one per key, the order Roblox shows them in
/// (measured); then indicators for the rest, which need no line of sight
/// (measured).
fn pick(
    w: &mut World,
    candidates: &[Candidate],
    limits: &Limits,
) -> (Vec<InstanceId>, Vec<InstanceId>) {
    let sight = Sight::new(w);
    let reachable = |kind| (candidates.iter()).filter(move |c| c.in_reach && c.exclusivity == kind);
    let mut shown = Vec::new();
    for c in reachable(Exclusivity::AlwaysShow) {
        if shown.len() == limits.prompts {
            break;
        }
        if sight.sees(w, c) {
            shown.push(c.id);
        }
    }
    if let Some(c) = reachable(Exclusivity::OneGlobally).find(|c| sight.sees(w, c)) {
        shown.push(c.id);
    }
    let mut keys = FxHashSet::default();
    for c in reachable(Exclusivity::OnePerButton) {
        if !keys.contains(&c.key) && sight.sees(w, c) {
            keys.insert(c.key);
            shown.push(c.id);
        }
    }
    let indicated = (candidates.iter())
        .filter(|c| c.indicator_reach && !shown.contains(&c.id))
        .take(limits.indicators)
        .map(|c| c.id)
        .collect();
    (shown, indicated)
}

/// Line of sight from the camera, through the local character and the
/// prompt's own part, which can't hide it (measured; transparent and
/// non-colliding parts do, ones without `CanQuery` don't).
struct Sight {
    camera: Option<Vec3>,
    character: Option<InstanceId>,
}

impl Sight {
    fn new(w: &World) -> Self {
        let dom = &w.dom;
        let camera = (dom.find_service("Workspace"))
            .and_then(|ws| dom.ref_property(ws, "CurrentCamera"))
            .and_then(|c| dom.cframe_property(c, "CFrame"))
            .map(|c| c.position);
        let character = (dom.local_player()).and_then(|p| dom.ref_property(p, "Character"));
        Self { camera, character }
    }

    fn sees(&self, w: &mut World, c: &Candidate) -> bool {
        if w.dom.bool_property(c.id, "RequiresLineOfSight") == Some(false) {
            return true;
        }
        let Some(camera) = self.camera else {
            return true;
        };
        let ignore = self.character.into_iter().chain(c.part);
        // Inference: legacy rays' rules for groups and water.
        let filter = rook_physics::Filter::legacy_ray(ignore, false);
        let w = &mut *w;
        w.physics
            .raycast(&mut w.dom, camera, c.at - camera, &filter)
            .is_none()
    }
}
