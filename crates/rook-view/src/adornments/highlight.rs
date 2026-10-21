//! Highlight: which one covers a part, and its look
//! (docs/behavior/highlight.md). The renderer outlines and fills the
//! parts over the finished frame.

use std::collections::HashMap;

use rook_render::Highlight;

use super::*;

/// The Highlight covering part `id`, if any: one whose `Adornee` is the
/// part or a Model it's in, or one inside the part or such a Model
/// without an `Adornee`; the nearest wins (Inference).
///
/// Observed in Studio 0.742 (2026-10-20): a Highlight with an `Adornee`
/// draws wherever it is (in ReplicatedStorage or Lighting too); a part's
/// covers only that part, not parts inside it; a Model's covers all its
/// parts, outlined together; one in a Folder, or adorning a Folder,
/// draws nothing; neither do invisible parts (the renderer leaves them
/// out).
pub(crate) fn highlight(
    dom: &Dom,
    id: InstanceId,
    models: &mut ModelHighlights,
) -> Option<Highlight> {
    if let Some(h) = covering(dom, id) {
        return Some(h);
    }
    // Workspace is a Model too, but holds everything (Inference).
    let mut held = dom
        .ancestors(id)
        .filter(|&a| dom.is_a(a, "Model") && !dom.is_a(a, "WorldRoot"));
    held.find_map(|model| *models.entry(model).or_insert_with(|| covering(dom, model)))
}

/// Each Model's own Highlight, looked up once per scene update (a big
/// Model's parts would each look through its children).
pub(crate) type ModelHighlights = HashMap<InstanceId, Option<Highlight>>;

/// The Highlight on `target` itself: adorning it, or inside it without
/// an `Adornee`.
fn covering(dom: &Dom, target: InstanceId) -> Option<Highlight> {
    let adorning = dom.referrers(target).iter().map(|&(h, _)| h).find(|&h| {
        dom.class_name(h) == "Highlight"
            && dom.ref_property(h, "Adornee") == Some(target)
            && enabled(dom, h)
    });
    let inside = || {
        dom.children(target).iter().copied().find(|&h| {
            dom.class_name(h) == "Highlight"
                && dom.ref_property(h, "Adornee").is_none()
                && enabled(dom, h)
        })
    };
    adorning.or_else(inside).map(|h| look(dom, h))
}

fn enabled(dom: &Dom, id: InstanceId) -> bool {
    dom.bool_property(id, "Enabled") != Some(false) && dom.parent(id).is_some()
}

/// Highlight `id`'s look; the defaults are the API reference's (fill red
/// at 0.5, outline white, AlwaysOnTop).
fn look(dom: &Dom, id: InstanceId) -> Highlight {
    let colour = |name, default: [f32; 3], transparency, at: f32| {
        let [r, g, b] = dom
            .color3_property(id, name)
            .map_or(default, Color3::to_array);
        let opacity = 1.0 - dom.float_property(id, transparency).unwrap_or(at);
        [r, g, b, opacity.clamp(0.0, 1.0)]
    };
    Highlight {
        key: id.to_ref().0,
        fill: colour("FillColor", [1.0, 0.0, 0.0], "FillTransparency", 0.5),
        outline: colour("OutlineColor", [1.0; 3], "OutlineTransparency", 0.0),
        on_top: dom.enum_item_property(id, "DepthMode") != Some("Occluded"),
    }
}
