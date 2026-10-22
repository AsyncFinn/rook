//! The legacy styles: a Frame's `Style` (FrameStyle) and a GuiButton's
//! (ButtonStyle). Each draws one of the install's textures sliced over the
//! object in place of its background and border, and moves what the
//! object holds in from its edges (docs/behavior/gui/styles.md).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

/// A style's look: its textures and how they're cut, and how far it moves
/// children and text in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Style {
    /// The texture shown at rest.
    pub(crate) idle: &'static str,
    /// The texture shown while a button is hovered, pressed or not
    /// interactable (a frame has one look)...
    pub(crate) lit: &'static str,
    /// ...and while it's selected.
    pub(crate) chosen: &'static str,
    /// Each corner's size in texture pixels, drawn at that many object
    /// pixels until the object's shorter side is less than two corners.
    pub(crate) corner: f32,
    /// How far children and text are moved in from each edge...
    inset: f32,
    /// ...on an object at least this many pixels on its shorter side;
    /// smaller ones move them in proportionally less.
    reach: f32,
}

const fn texture(path: &'static str, corner: f32, inset: f32, reach: f32) -> Style {
    Style {
        idle: path,
        lit: path,
        chosen: path,
        corner,
        inset,
        reach,
    }
}

const fn button(idle: &'static str, lit: &'static str, corner: f32, reach: f32) -> Style {
    Style {
        idle,
        lit,
        chosen: lit,
        corner,
        inset: 12.0,
        reach,
    }
}

// Measured: the textures are the ones named in Studio's executable next to
// these enums; the insets come from children's `AbsolutePosition`, the
// corners from fitting captures of each style at heights 4 to 44. For
// the button and square textures the corner is half the inset's reach.
const RED: Style = button(
    "rbxasset://textures/ui/btn_red.png",
    "rbxasset://textures/ui/btn_redGlow.png",
    18.0,
    36.0,
);
// Measured: a selected RobloxButton looks like a RobloxButtonDefault.
const GREY: Style = Style {
    chosen: RED.idle,
    ..button(
        "rbxasset://textures/ui/btn_grey.png",
        "rbxasset://textures/ui/btn_white.png",
        18.0,
        36.0,
    )
};
const ROUND_GREY: Style = button(
    "rbxasset://textures/ui/btn_newGrey.png",
    "rbxasset://textures/ui/btn_newGreyGlow.png",
    13.0,
    26.0,
);
const ROUND_BLUE: Style = button(
    "rbxasset://textures/ui/btn_newBlue.png",
    "rbxasset://textures/ui/btn_newBlueGlow.png",
    13.0,
    26.0,
);
const ROUND_WHITE: Style = button(
    "rbxasset://textures/ui/btn_newWhite.png",
    "rbxasset://textures/ui/btn_newWhiteGlow.png",
    13.0,
    26.0,
);

/// `id`'s style, or `None` for Custom (and for classes without one).
pub(crate) fn of(dom: &Dom, id: InstanceId) -> Option<Style> {
    let name = dom.enum_item_property(id, "Style")?;
    let style = if dom.is_a(id, "GuiButton") {
        match name {
            "RobloxButtonDefault" => RED,
            "RobloxButton" => GREY,
            "RobloxRoundButton" => ROUND_GREY,
            "RobloxRoundDefaultButton" => ROUND_BLUE,
            "RobloxRoundDropdownButton" => ROUND_WHITE,
            _ => return None,
        }
    } else {
        match name {
            "ChatBlue" => texture("rbxasset://textures/ui/dialog_blue.png", 7.0, 17.0, 60.0),
            "ChatGreen" => texture("rbxasset://textures/ui/dialog_green.png", 7.0, 17.0, 60.0),
            "ChatRed" => texture("rbxasset://textures/ui/dialog_red.png", 7.0, 17.0, 60.0),
            "RobloxSquare" => texture("rbxasset://textures/blackBkg_square.png", 10.5, 8.0, 21.0),
            "RobloxRound" => texture("rbxasset://textures/blackBkg_round.png", 10.5, 8.0, 21.0),
            "DropShadow" => texture("rbxasset://textures/ui/newBkg_square.png", 27.0, 8.0, 21.0),
            _ => return None,
        }
    };
    Some(style)
}

impl Style {
    /// How far children and text are moved in from each edge of an object
    /// `size` big. Measured: 17 for the Chat styles, 8 for the other frame
    /// styles and 12 for the buttons, scaled by the shorter side over the
    /// reach when that's less (a 200 × 10 ChatBlue put a child 2.833 in).
    pub(crate) fn inset(&self, size: Vec2) -> f32 {
        self.inset * (size.abs().min_element() / self.reach).min(1.0)
    }

    /// The texture `id` shows now. Measured: with `AutoButtonColor` on, a
    /// hovered, pressed or non-interactable button shows its lit texture
    /// (with it off, its idle one), and a selected idle one its chosen
    /// texture. Inference: a selected button that's hovered looks hovered.
    pub(crate) fn shown(&self, dom: &Dom, id: InstanceId) -> &'static str {
        let tints = dom.bool_property(id, "AutoButtonColor") != Some(false);
        let busy = !matches!(dom.enum_item_property(id, "GuiState"), None | Some("Idle"));
        if tints && busy {
            self.lit
        } else if dom.bool_property(id, "Selected") == Some(true) {
            self.chosen
        } else {
            self.idle
        }
    }
}
