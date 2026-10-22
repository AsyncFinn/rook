# GUI legacy styles

Checked against Studio 0.744 on 2026-10-22.

[M8](../../roadmap/m8-gui.md) step 6. A Frame's `Style` (FrameStyle) and a
GuiButton's (ButtonStyle, on TextButton and ImageButton) give the look
older places use for their panels and buttons. Measured in Studio 0.744
on 2026-10-22 on Baseplate, from `screen_capture` shots of styled objects
on SurfaceGuis (`LightInfluence` 0, 50 pixels a stud) at FieldOfView 10,
2 to 3 screen pixels a canvas pixel, next to ImageLabels slicing the same
textures and plain frames of known colors; hover and press through the
world in a playtest client with the MCP's `user_mouse_input`; insets from
children's `AbsolutePosition`. The conformance case `gui/styles` holds the
insets.

Code: the table of styles in `rook-gui`'s `style.rs`, the inset in
`layout/padding.rs`, the drawing in `draw/style.rs`.

## Textures

Each style draws one texture from the install's `content` folder
(Rook loads them from the user's Roblox install like any `rbxasset://`
image and ships none, per the
[content policy](../../design/policies.md#content-and-assets)):

| Style | Texture | Lit (hovered, pressed, not interactable) |
|---|---|---|
| RobloxButtonDefault | `rbxasset://textures/ui/btn_red.png` | `rbxasset://textures/ui/btn_redGlow.png` |
| RobloxButton | `rbxasset://textures/ui/btn_grey.png` | `rbxasset://textures/ui/btn_white.png` |
| RobloxRoundButton | `rbxasset://textures/ui/btn_newGrey.png` | `rbxasset://textures/ui/btn_newGreyGlow.png` |
| RobloxRoundDefaultButton | `rbxasset://textures/ui/btn_newBlue.png` | `rbxasset://textures/ui/btn_newBlueGlow.png` |
| RobloxRoundDropdownButton | `rbxasset://textures/ui/btn_newWhite.png` | `rbxasset://textures/ui/btn_newWhiteGlow.png` |
| ChatBlue, ChatGreen, ChatRed | `rbxasset://textures/ui/dialog_blue.png`, `_green`, `_red` | |
| RobloxSquare | `rbxasset://textures/blackBkg_square.png` (black at 70%) | |
| RobloxRound | `rbxasset://textures/blackBkg_round.png` | |
| DropShadow | `rbxasset://textures/ui/newBkg_square.png` | |

The names sit together in Studio's executable next to each enum's use;
which is which was matched by eye from the captures. The Chat textures
are white bubbles with a colored strip down the left edge, not colored
bubbles. `rbxasset://textures/ui/btn_greyTransp.png`, named with them, showed in no state
tried.

## Drawing

- **The texture replaces the background and border**: it draws untinted
  and opaque whatever `BackgroundColor3`, `BackgroundTransparency` (a
  styled button at 1 still drew), `BorderSizePixel`, `BorderColor3` and
  `AutoButtonColor`. Text keeps its `TextColor3` (dark text on the black
  RobloxButton stays dark), and an ImageButton's image draws over its
  style, over the whole box.
- **Nothing draws outside the box**, DropShadow included: its shadow is
  the bottom of its texture, inside the object.
- **Nine slices**: each texture is cut with square corners of a fixed
  size in texture pixels, drawn one to one: 18 for the 24-pixel square
  button textures, 13 for the 20-pixel round ones, 10.5 for the 24-pixel
  frame textures, 7 for the 40-pixel Chat ones and 27 for DropShadow's
  40-pixel one. Corners may reach past the texture's middle (18 of 24);
  the middle is drawn from between them. These sizes are fitted to
  captures of every texture at heights 4 to 44 (within about half a
  pixel along the edges); for the button and frame textures they are half
  the inset's reach below, which suggests the engine has one size per
  style.
- **Small objects**: when the object's shorter side is under two corners,
  the corners shrink with it (a 60 × 8 RobloxRound is a pill, a 60 × 8
  ChatBlue's strip is about 3 pixels wide). Tall narrow objects shrink
  the same way by their width.
- Rook's screenshot of the same 63 objects differs from Studio's capture
  by 2 to 4 levels on average per object (the JPEG's floor; a plain
  RobloxSquare differs as much), except the RobloxButton and
  RobloxButtonDefault textures under 12 pixels tall: Studio's are blurred
  into a dark bar there, Rook's keep their rim.
- `Rotation` turns the slices with the object. Studio draws thin seams
  between the pieces of a turned style; Rook doesn't.
- UICorner on a styled object rounds each of the nine pieces on its own
  in Studio, leaving notches; UIStroke draws its stroke around the box as
  usual. Rook rounds the object as a whole instead
  ([modern UI objects](modern.md#uicorner)).

## States

- **Lit**: with `AutoButtonColor` on, a hovered or pressed button shows
  its lit texture (the same one for both), as does a non-interactable one
  (`Interactable` false). With `AutoButtonColor` off it keeps its texture.
  No tint is applied on top.
- **Selected**: a selected RobloxButton shows RobloxButtonDefault's
  texture; the other button styles show their lit texture. Inference: a
  selected button that is hovered looks hovered.

## Insets

A style moves its children and its text in from every edge, after which
UIPadding applies inside (a 5-pixel left padding put a child 17 in on a
RobloxButton):

| Style | Inset | Reach |
|---|---|---|
| ChatBlue, ChatGreen, ChatRed | 17 | 60 |
| RobloxSquare, RobloxRound, DropShadow | 8 | 21 |
| RobloxButtonDefault, RobloxButton | 12 | 36 |
| The round button styles | 12 | 26 |

On an object whose shorter side is under the reach, the inset is scaled
by the shorter side over the reach (a 200 × 10 ChatBlue put a child
2.833 in, a 30 × 40 one 8.5). Children's `Size` and `Position` scale with
what's left. An Inset border's part adds to it, though the style draws no
border (a 4-pixel Inset border on a RobloxSquare put a child 12 in).
`TextFits` doesn't count the text's height without wrapping, styled or
not ([text](text.md#lines)).

## Properties

Setting `Style` changes none of the properties scripts read
(`BackgroundColor3`, `BackgroundTransparency`, `BorderSizePixel`,
`AutoButtonColor`, `TextColor3` stay as they were). ScrollingFrame has no
`Style`.
