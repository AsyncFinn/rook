//! Names and health bars over Humanoids' heads: which the local player
//! sees, how big and how faded, and where. Studio's engine draws them, with
//! no instances; the rules were read from Studio 0.744 screenshots
//! (docs/behavior/characters/overhead.md).

use glam::{Vec2, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_types::WorldPlace;
use rustc_hash::FxHashSet;

use super::Eye;
use crate::draw::overhead::BAR_HEIGHT;

/// Whether a part outside the ignored instances is between two points.
pub(crate) type Blocked<'a> = dyn FnMut(&mut Dom, Vec3, Vec3, &[InstanceId]) -> bool + 'a;

/// What a tag shows; a tag draws again only when this changes.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Look {
    pub(crate) text: String,
    /// Its size step: 1, 0.75 or 0.5.
    pub(crate) scale: f32,
    pub(crate) name_alpha: f32,
    pub(crate) bar: Option<Bar>,
    pub(crate) bar_alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Bar {
    /// `Health / MaxHealth`, 0 to 1.
    pub(crate) health: f32,
}

/// A tag shown this frame.
#[derive(Debug, Clone)]
pub(crate) struct Shown {
    pub(crate) humanoid: InstanceId,
    pub(crate) look: Look,
    /// Measured: the bar's top is 1.5 studs above the head's center, at
    /// any head size and distance.
    anchor: Vec3,
}

impl Shown {
    pub(crate) fn place(&self, size: Vec2) -> WorldPlace {
        // Measured: the bar's top 1.4 pixels above the anchor.
        const LIFT: f32 = 1.4;
        WorldPlace::Billboard {
            anchor: self.anchor,
            studs: Vec3::ZERO,
            size_studs: Vec2::ZERO,
            size_pixels: size,
            shift: Vec2::new(0.0, (size.y * 0.5 - (BAR_HEIGHT - LIFT)) / size.y),
        }
    }
}

/// Measured: tags shrink in steps with the head's distance from the
/// camera: full size at 18 studs, three quarters from 22 to 34, half
/// from 52. Inference: the steps are at 20 and 40.
fn scale(distance: f32) -> f32 {
    if distance < 20.0 {
        1.0
    } else if distance < 40.0 {
        0.75
    } else {
        0.5
    }
}

/// Measured: opaque to 90 studs of a 100-stud display distance, half at
/// 95, gone past 100. Inference: a fade over the last 10 studs, kept to
/// steps of 1/32 so a slowly walking player doesn't draw it every frame.
fn fade(limit: f32, distance: f32) -> f32 {
    (((limit - distance) / 10.0).clamp(0.0, 1.0) * 32.0).round() / 32.0
}

/// Every Humanoid that has been in the Workspace; ones that left are
/// dropped each update.
#[derive(Debug, Default)]
pub(crate) struct Overheads {
    humanoids: FxHashSet<InstanceId>,
    pub(crate) shown: Vec<Shown>,
}

impl Overheads {
    pub(crate) fn note(&mut self, dom: &Dom, ids: impl Iterator<Item = InstanceId>) {
        self.humanoids
            .extend(ids.filter(|&id| dom.class_name(id) == "Humanoid"));
    }

    /// Work out this frame's tags. `blocked(dom, from, to, ignore)` says
    /// whether a part (outside `ignore`) is between two points. Returns
    /// whether any tag's look changed, or tags came or went.
    pub(crate) fn update(&mut self, dom: &mut Dom, blocked: &mut Blocked<'_>) -> bool {
        let shown = self.tags(dom, blocked);
        let changed = shown.len() != self.shown.len()
            || (shown.iter().zip(&self.shown))
                .any(|(a, b)| a.humanoid != b.humanoid || a.look != b.look);
        self.shown = shown;
        changed
    }

    fn tags(&mut self, dom: &mut Dom, blocked: &mut Blocked<'_>) -> Vec<Shown> {
        let (Some(workspace), Some(player), Some(eye)) = (
            dom.find_service("Workspace"),
            dom.local_player(),
            Eye::of(dom),
        ) else {
            return Vec::new();
        };
        self.humanoids
            .retain(|&h| dom.get(h).is_some() && dom.is_ancestor_of(workspace, h));
        let viewer = Viewer::of(dom, player, eye.position);
        let mut humanoids: Vec<InstanceId> = self.humanoids.iter().copied().collect();
        humanoids.sort_unstable();
        // Measured: other characters don't hide a head.
        let characters: Vec<InstanceId> = humanoids.iter().filter_map(|&h| dom.parent(h)).collect();
        let mut shown = Vec::new();
        for humanoid in humanoids {
            let Some((look, head)) = look(dom, humanoid, &viewer, &eye) else {
                continue;
            };
            if occluded(dom, humanoid, &viewer) && blocked(dom, eye.position, head, &characters) {
                continue;
            }
            shown.push(Shown {
                humanoid,
                look,
                anchor: head + Vec3::Y * 1.5,
            });
        }
        shown
    }
}

/// The local player, and where distances to tags are measured from.
struct Viewer {
    player: InstanceId,
    character: Option<InstanceId>,
    humanoid: Option<InstanceId>,
    /// Measured: from the player's character (its head), not the camera.
    /// Inference: from the camera with no character.
    from: Vec3,
}

impl Viewer {
    fn of(dom: &Dom, player: InstanceId, camera: Vec3) -> Self {
        let character = dom.ref_property(player, "Character");
        let humanoid = character.and_then(|c| dom.find_first_child_of_class(c, "Humanoid"));
        let from = character
            .and_then(|c| dom.find_first_child(c, "Head", false))
            .and_then(|h| dom.cframe_property(h, "CFrame"))
            .map_or(camera, |f| f.position);
        Self {
            player,
            character,
            humanoid,
            from,
        }
    }
}

/// What `humanoid`'s tag shows to `viewer`, and where its head is; `None`
/// when it shows nothing.
fn look(dom: &Dom, humanoid: InstanceId, viewer: &Viewer, eye: &Eye) -> Option<(Look, Vec3)> {
    let model = dom.parent(humanoid)?;
    // Inference: the local player never sees its own.
    if Some(model) == viewer.character || !dom.is_a(model, "Model") {
        return None;
    }
    let head = dom
        .find_first_child(model, "Head", false)
        .filter(|&h| dom.is_a(h, "BasePart"))?;
    let head = dom.drawn_cframe(head)?.position;
    // creator-docs HumanoidDisplayDistanceType: None hides it; Subject
    // uses its own distances, Viewer the viewer's (measured: raising the
    // viewer's showed tags 300 studs off).
    let distances_from = match dom.enum_item_property(humanoid, "DisplayDistanceType") {
        Some("None") => return None,
        Some("Subject") => Some(humanoid),
        _ => viewer.humanoid,
    };
    let limit = |name| {
        distances_from
            .and_then(|h| dom.float_property(h, name))
            .unwrap_or(100.0)
    };
    let distance = (head - viewer.from).length();
    let name_alpha = fade(limit("NameDisplayDistance"), distance);
    let bar_alpha = fade(limit("HealthDisplayDistance"), distance);
    let bar = health_bar(dom, humanoid).filter(|_| bar_alpha > 0.0);
    if name_alpha <= 0.0 && bar.is_none() {
        return None;
    }
    let text = dom
        .string_property(humanoid, "DisplayName")
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| dom.name(model).to_owned());
    let look = Look {
        text,
        scale: scale((head - eye.position).length()),
        name_alpha,
        bar,
        bar_alpha,
    };
    Some((look, head))
}

/// creator-docs HumanoidHealthDisplayType; measured: a dead Humanoid keeps
/// its name and an empty bar.
fn health_bar(dom: &Dom, humanoid: InstanceId) -> Option<Bar> {
    let max = dom.number_property(humanoid, "MaxHealth").unwrap_or(100.0);
    let health = dom.number_property(humanoid, "Health_XML").unwrap_or(max);
    let shown = match dom.enum_item_property(humanoid, "HealthDisplayType") {
        Some("AlwaysOff") => false,
        Some("AlwaysOn") => true,
        _ => health < max,
    };
    (shown && max > 0.0).then(|| Bar {
        health: (health / max).clamp(0.0, 1.0) as f32,
    })
}

/// Whether `humanoid`'s tag hides while a part is between the camera and
/// its head (measured: OccludeAll, the default, hides it; NoOcclusion
/// doesn't). Inference: EnemyOcclusion hides only other teams' players.
fn occluded(dom: &Dom, humanoid: InstanceId, viewer: &Viewer) -> bool {
    match dom.enum_item_property(humanoid, "NameOcclusion") {
        Some("NoOcclusion") => false,
        Some("EnemyOcclusion") => {
            let Some(other) = dom.parent(humanoid).and_then(|m| player_of(dom, m)) else {
                return false;
            };
            let team = |p| {
                (dom.bool_property(p, "Neutral") == Some(false))
                    .then(|| dom.ref_property(p, "Team"))
                    .flatten()
            };
            team(other).is_none() || team(other) != team(viewer.player)
        }
        _ => true,
    }
}

fn player_of(dom: &Dom, character: InstanceId) -> Option<InstanceId> {
    let players = dom.find_service("Players")?;
    (dom.children(players).iter().copied())
        .find(|&p| dom.ref_property(p, "Character") == Some(character))
}
