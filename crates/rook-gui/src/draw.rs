//! What the local player's ScreenGuis draw, in order: each visible GUI
//! object's background and border, then its text (images come with
//! docs/roadmap/m8-gui.md, step 4), and any Hint or Message over it all; and what each
//! world GUI's canvas draws, the same way. Order and clipping rules:
//! docs/behavior/gui/layout.md#drawing.

mod frame;
mod messages;
mod text;

use rook_dom::{Dom, InstanceId};
use rook_types::{GuiQuad, WorldCanvas};

use crate::layout::{Layout, Placed, gui_children, through_folders, visible};

/// The fonts and glyph masks text is drawn with.
pub(crate) struct Text<'a> {
    pub(crate) fonts: &'a mut crate::text::Fonts,
    pub(crate) glyphs: &'a mut crate::text::Glyphs,
}

/// Bottom first. Measured: ScreenGuis stack by `DisplayOrder`, later ones
/// on top on a tie.
pub(crate) fn quads(dom: &Dom, layout: &Layout, text: &mut Text<'_>) -> Vec<GuiQuad> {
    let mut out = Vec::new();
    let Some(player_gui) = dom
        .local_player()
        .and_then(|p| dom.find_first_child_of_class(p, "PlayerGui"))
    else {
        return out;
    };
    let mut draw = Draw {
        dom,
        layout,
        text,
        out: &mut out,
        fields: false,
    };
    let mut screens = through_folders(dom, player_gui, |id| dom.class_name(id) == "ScreenGui");
    screens.retain(|&s| dom.bool_property(s, "Enabled") != Some(false));
    screens.sort_by_key(|&s| dom.number_property(s, "DisplayOrder").unwrap_or(0.0) as i64);
    for screen in screens {
        draw.collector(screen, Clip::NONE);
    }
    messages::emit(dom, draw.text, draw.out);
    out
}

/// What each of `guis` (world GUIs, laid out) draws on its canvas. Its
/// text is drawn from distance fields, so it stays sharp however near it's
/// seen.
pub(crate) fn world(
    dom: &Dom,
    layout: &Layout,
    guis: &[InstanceId],
    text: &mut Text<'_>,
) -> Vec<WorldCanvas> {
    let mut canvases = Vec::new();
    for &gui in guis {
        let Some(placed) = layout.placed.get(&gui) else {
            continue;
        };
        let size = placed.local;
        let clip = Clip {
            rect: if dom.bool_property(gui, "ClipsDescendants") == Some(true) {
                [0.0, 0.0, size.x, size.y]
            } else {
                GuiQuad::NO_CLIP
            },
            turned: false,
        };
        let mut quads = Vec::new();
        Draw {
            dom,
            layout,
            text,
            out: &mut quads,
            fields: true,
        }
        .collector(gui, clip);
        let max_distance = match dom.float_property(gui, "MaxDistance") {
            // Inference: a SurfaceGui's default MaxDistance 0 means no
            // limit, as creator-docs say.
            Some(d) if d > 0.0 => d,
            _ => f32::INFINITY,
        };
        canvases.push(WorldCanvas {
            quads,
            size,
            light_influence: dom.float_property(gui, "LightInfluence").unwrap_or(0.0),
            brightness: dom.float_property(gui, "Brightness").unwrap_or(1.0),
            always_on_top: dom.bool_property(gui, "AlwaysOnTop") == Some(true),
            max_distance,
        });
    }
    canvases
}

struct Draw<'a, 'b> {
    dom: &'a Dom,
    layout: &'a Layout,
    text: &'a mut Text<'b>,
    out: &'a mut Vec<GuiQuad>,
    /// Text is drawn from distance fields, not masks at its size.
    fields: bool,
}

impl Draw<'_, '_> {
    /// What a LayerCollector holds, in its `ZIndexBehavior`'s order.
    fn collector(&mut self, root: InstanceId, clip: Clip) {
        let mut drawn = Vec::new();
        collect(self.dom, self.layout, root, clip, &mut drawn);
        if self.dom.enum_item_property(root, "ZIndexBehavior") == Some("Global") {
            // Measured: every object by ZIndex alone, later in the tree on
            // top on a tie.
            let mut flat = Vec::new();
            flatten(drawn, &mut flat);
            flat.sort_by_key(|d| d.z);
            for d in flat {
                self.emit(&d);
            }
        } else {
            sibling_order(self, drawn);
        }
    }

    fn emit(&mut self, d: &Drawn) {
        frame::emit(self.dom, d.id, d.placed, d.clip, self.out);
        if let Some(laid) = self.layout.texts.get(&d.id) {
            let (text, out) = (&mut *self.text, &mut *self.out);
            text::emit(&laid.block, d.placed, d.clip, self.fields, text, out);
        }
    }
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
fn sibling_order(draw: &mut Draw<'_, '_>, mut drawn: Vec<Drawn>) {
    drawn.sort_by_key(|d| d.z);
    for mut d in drawn {
        let kids = std::mem::take(&mut d.kids);
        draw.emit(&d);
        sibling_order(draw, kids);
    }
}
