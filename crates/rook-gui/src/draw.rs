//! What the local player's ScreenGuis draw, in order: each visible GUI
//! object's background and border (text and images come with ROADMAP M8
//! steps 2 and 3). Order and clipping rules: docs/behavior/gui.md#drawing.

use glam::Vec2;
use rook_dom::{Dom, InstanceId};
use rook_types::GuiQuad;

use crate::layout::{Layout, Placed, gui_children, through_folders, turn, visible};

/// Bottom first. Measured: ScreenGuis stack by `DisplayOrder`, later ones
/// on top on a tie.
pub(crate) fn quads(dom: &Dom, layout: &Layout) -> Vec<GuiQuad> {
    let mut out = Vec::new();
    let Some(player_gui) = dom
        .local_player()
        .and_then(|p| dom.find_first_child_of_class(p, "PlayerGui"))
    else {
        return out;
    };
    let mut screens = through_folders(dom, player_gui, |id| dom.class_name(id) == "ScreenGui");
    screens.retain(|&s| dom.bool_property(s, "Enabled") != Some(false));
    screens.sort_by_key(|&s| dom.number_property(s, "DisplayOrder").unwrap_or(0.0) as i64);
    for screen in screens {
        let mut drawn = Vec::new();
        collect(dom, layout, screen, Clip::NONE, &mut drawn);
        if dom.enum_item_property(screen, "ZIndexBehavior") == Some("Global") {
            // Measured: every object by ZIndex alone, later in the tree on
            // top on a tie.
            let mut flat = Vec::new();
            flatten(drawn, &mut flat);
            flat.sort_by_key(|d| d.z);
            for d in flat {
                d.emit(dom, &mut out);
            }
        } else {
            sibling_order(dom, drawn, &mut out);
        }
    }
    out
}

/// What clips an object's descendants.
#[derive(Debug, Clone, Copy)]
struct Clip {
    rect: [f32; 4],
    /// The object or an ancestor is turned, which turns clipping off
    /// below it (Confirmed: creator-docs GuiObject.ClipsDescendants).
    turned: bool,
}

impl Clip {
    const NONE: Self = Self {
        rect: GuiQuad::NO_CLIP,
        turned: false,
    };
}

/// A visible object to draw, with what it holds.
struct Drawn {
    id: InstanceId,
    z: i64,
    placed: Placed,
    clip: [f32; 4],
    kids: Vec<Drawn>,
}

fn collect(dom: &Dom, layout: &Layout, parent: InstanceId, clip: Clip, out: &mut Vec<Drawn>) {
    for kid in gui_children(dom, parent) {
        let Some(&placed) = layout.placed.get(&kid) else {
            continue;
        };
        if !visible(dom, kid) {
            continue;
        }
        let turned = clip.turned || placed.rotation != 0.0;
        let mut inside = Clip {
            rect: clip.rect,
            turned,
        };
        if !turned && dom.bool_property(kid, "ClipsDescendants") == Some(true) {
            let (lo, hi) = (placed.top_left(), placed.top_left() + placed.size());
            let r = clip.rect;
            inside.rect = [
                r[0].max(lo.x),
                r[1].max(lo.y),
                r[2].min(hi.x),
                r[3].min(hi.y),
            ];
        }
        let mut kids = Vec::new();
        collect(dom, layout, kid, inside, &mut kids);
        out.push(Drawn {
            id: kid,
            z: dom.number_property(kid, "ZIndex").unwrap_or(1.0) as i64,
            placed,
            clip: clip.rect,
            kids,
        });
    }
}

fn flatten(drawn: Vec<Drawn>, out: &mut Vec<Drawn>) {
    for mut d in drawn {
        let kids = std::mem::take(&mut d.kids);
        out.push(d);
        flatten(kids, out);
    }
}

/// Measured: siblings by ZIndex (later on top on a tie), each with all it
/// holds over it.
fn sibling_order(dom: &Dom, mut drawn: Vec<Drawn>, out: &mut Vec<GuiQuad>) {
    drawn.sort_by_key(|d| d.z);
    for mut d in drawn {
        let kids = std::mem::take(&mut d.kids);
        d.emit(dom, out);
        sibling_order(dom, kids, out);
    }
}

impl Drawn {
    /// The background, and the border around it. Inference: the border
    /// takes the background's transparency and doesn't overlap it, and
    /// UIScales thicken it.
    fn emit(&self, dom: &Dom, out: &mut Vec<GuiQuad>) {
        let alpha = 1.0
            - dom
                .float_property(self.id, "BackgroundTransparency")
                .unwrap_or(0.0);
        if alpha <= 0.0 {
            return;
        }
        let p = self.placed;
        let size = p.size();
        let border = dom
            .number_property(self.id, "BorderSizePixel")
            .unwrap_or(0.0) as f32
            * p.scale;
        // Confirmed (creator-docs BorderMode): Outline outside the
        // object, Middle across its edge, Inset inside.
        let (inset, outset) = match dom.enum_item_property(self.id, "BorderMode") {
            Some("Middle") => (border * 0.5, border * 0.5),
            Some("Inset") => (border, 0.0),
            _ => (0.0, border),
        };
        let quad = |offset: Vec2, extent: Vec2, color: [f32; 3]| GuiQuad {
            center: p.center + turn(p.rotation, offset),
            size: extent,
            rotation: p.rotation.to_radians(),
            color: [color[0], color[1], color[2], alpha],
            clip: self.clip,
        };
        if border > 0.0 {
            let color = dom
                .color3_property(self.id, "BorderColor3")
                .map_or([0.0; 3], |c| [c.r, c.g, c.b]);
            let outer = size.abs() + Vec2::splat(2.0 * outset);
            let thick = border;
            let half = outer * 0.5 - thick * 0.5;
            out.push(quad(
                Vec2::new(0.0, -half.y),
                Vec2::new(outer.x, thick),
                color,
            ));
            out.push(quad(
                Vec2::new(0.0, half.y),
                Vec2::new(outer.x, thick),
                color,
            ));
            let side = Vec2::new(thick, outer.y - 2.0 * thick);
            if side.y > 0.0 {
                out.push(quad(Vec2::new(-half.x, 0.0), side, color));
                out.push(quad(Vec2::new(half.x, 0.0), side, color));
            }
        }
        let inner = size.abs() - Vec2::splat(2.0 * inset);
        if inner.x > 0.0 && inner.y > 0.0 {
            let color = dom
                .color3_property(self.id, "BackgroundColor3")
                .map_or([1.0; 3], |c| [c.r, c.g, c.b]);
            out.push(quad(Vec2::ZERO, inner, color));
        }
    }
}
