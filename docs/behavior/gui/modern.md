# GUI modern UI objects

Checked against Studio 0.744 on 2026-10-22.

[M8](../../roadmap/m8-gui.md) step 7: UICorner, UIStroke, UIGradient,
UIListLayout's flex and `Wraps` with UIFlexItem, UITableLayout,
UIPageLayout and CanvasGroup. ViewportFrame and WorldModel are a later
step.

Measured in Studio 0.744 on 2026-10-22 on Baseplate. Layouts and property
reads are conformance cases (`gui/flex`, `gui/table_layout`,
`gui/page_layout`, `gui/corner_radius`), probed first in a 400 × 300 box.
Looks are from `screen_capture` shots of objects on SurfaceGuis
(`LightInfluence` 0, 50 pixels a stud) at FieldOfView 10, 1.92 screen
pixels a canvas pixel (8 for one close-up), read with System.Drawing next
to grey frames of known colors; Rook's screenshots of the same boards
(`rook-player --script`) were compared pixel by pixel. CanvasGroups and
ViewportFrames don't appear in captures at all, background included, so
CanvasGroup was looked at in screenshots the user took.

Code: drawing in `rook-gui`'s `draw/corner.rs`, `draw/stroke.rs`,
`draw/gradient.rs` and `draw/group.rs`, applied in `draw.rs`; shapes and
gradients in `rook-render`'s `shaders/gui_paint.wgsl`, ramps in
`renderer/gui/ramps.rs`, groups' textures in `renderer/gui/groups.rs`;
layouts in `rook-gui`'s `layout/list.rs`, `layout/table.rs` and
`layout/page.rs`; UIPageLayout's methods in `rook-lua`'s
`api/luau/page_layout.luau`.

## UICorner

- The shape is computed for each screen pixel, not drawn into the canvas
  first: zoomed to 8 screen pixels a canvas pixel the curve stays smooth.
  Across a curved edge one pixel is blended; straight edges stay hard.
- `CornerRadius` is a view of the four corners' radii (`TopLeftRadius`,
  `TopRightRadius`, `BottomRightRadius`, `BottomLeftRadius`, 8 pixels by
  default): setting it sets all four, reading it reads the top left's.
  Files save the four.
- No radius goes past half the shorter side: a scale of 0.5 and an offset
  of 60 both make a 160 × 80 frame a pill. Inference: the scale is of the
  shorter side, and UIScales grow the offset.
- It rounds the background, the image and the text's box; an object with
  a UICorner draws no border (`BorderSizePixel` 6 showed nothing).
- `ClipsDescendants` still clips to the rectangle, not the rounded shape.

## UIStroke

- Border mode, or Contextual on anything but a text object, draws a band
  round the edge, over the object's background, image and text and under
  its children, in its place among siblings.
- `BorderStrokePosition` Outer (the default) lies outside the edge,
  Center across it, Inner inside.
- Without a UICorner, `LineJoinMode` Round rounds the outside corners by
  the band's width outside the edge, Bevel cuts them, Miter leaves them
  square; Inner's outside corners are square. With a UICorner, the band
  follows it: its outside radius is the corner's plus the band's width
  outside the edge, its inside the corner's.
- Its opacity is its own `Transparency`; the object's
  `BackgroundTransparency` doesn't change it (a band shows round a frame
  with `BackgroundTransparency` 1).
- `Thickness` is in pixels and UIScales widen it even at
  `StrokeSizingMode` FixedSize (a UIScale of 2 made a 4-pixel band 8);
  ScaledSize's `Thickness` is a share of the shorter side (0.1 on a
  140 × 80 frame, 8 pixels).
- A UIGradient under the UIStroke colors the band, over the band's own
  box.
- Contextual on a text object outlines the text instead, about
  `Thickness` pixels out, in place of `TextStrokeTransparency`'s outline.
  Rook draws it as the text stroke is drawn, eight copies moved its
  width each way (Inference), so it's a little rougher than Studio's.
- Inference: `BorderOffset` moves the band out by its offset in pixels
  and its scale of the shorter side.

## UIGradient

- `t` runs from 0 to 1 along `Rotation` (clockwise, 0 left to right, 90
  top to bottom) through the box's center, over the length of that line
  inside the box: `min(width / |cos|, height / |sin|)`. Fitted from five
  frames (360 × 120, 120 × 120 and 60 × 200 at 45 degrees, 360 × 100 at 30
  and 135): the slopes agree within 1%. Past the ends the end colors hold.
- `Offset` moves the gradient by that share of the box's size (0.25
  across starts the ramp a quarter of the way in).
- Colors blend in a gamma of 2: halfway from black to white reads as
  √0.5, a quarter as 0.5, exactly. `Transparency` blends in a straight
  line.
- The gradient multiplies the color and opacity of the object's
  background, image and text (a red frame with a yellow-to-cyan gradient
  goes from red to black).
- Rook's screenshots of the same frames land within 4 of Studio's values
  at every point sampled.

## Flex

UIListLayout's `HorizontalFlex` and `VerticalFlex` (whichever is along
`FillDirection` flexes along, the other across), `ItemLineAlignment` and
`Wraps`, and UIFlexItem's `FlexMode`, `GrowRatio`, `ShrinkRatio` and
`ItemLineAlignment`:

- With `Wraps`, an item starts a new line when the line, `Padding` and it
  would pass the area's length; the first item of a line always fits.
  Lines are `Padding` apart too. Each line flexes and aligns on its own.
- Room to spare along a line is shared by grow ratio; room lacking is
  taken by shrink ratio times each item's length. An item its size
  constraints stop keeps that length, and the rest is shared again among
  the others.
- Ratios: a UIFlexItem's FlexMode Grow is (grow 1, shrink 0), Shrink
  (0, 1), Fill (1, 1), Custom its `GrowRatio` and `ShrinkRatio`, None
  (0, 0), even when the list fills. A child without a UIFlexItem is (1, 1)
  when the list's flex along is Fill, (0, 0) otherwise.
- Room still spare: SpaceAround puts half a share at each end, SpaceEvenly
  a share, SpaceBetween none (one item stays at the start); otherwise the
  alignment along places the run.
- Across, a line is as thick as its thickest item. The flex across Fill
  shares the room left among the lines; the Space modes spread them (one
  line: SpaceBetween at the start, the others centered); otherwise the
  lines go where the alignment across puts them.
- In its line, an item sits by its UIFlexItem's `ItemLineAlignment`, or
  the list's when that's Automatic: Start, Center, End or Stretch (to the
  line's thickness). Automatic stretches when the flex across is Fill and
  aligns by the alignment across otherwise, which without flex is the
  old rule.
- `AbsoluteContentSize` is the longest line (the room spreading adds left
  out, grown items counted) by the lines across.
- Children of an item flex changed lay out in its new size.

## UITableLayout

- Its children are rows and their children the cells, each list in
  `SortOrder`. A column is as wide as its widest cell and a row as tall as
  its tallest; every cell takes its column's width and its row's height.
- `Padding`'s X lies between columns and its Y between rows. A row is as
  wide as the table (`Padding` included) and as tall as its row.
- `FillEmptySpaceColumns` and `FillEmptySpaceRows` stretch the columns or
  rows in proportion to their sizes to fill the area. The table is aligned
  as a whole.
- `MajorAxis` ColumnMajor, or `FillDirection` Horizontal, makes the
  children columns.
- A cell's scale size is of the table's area, not its row's.
- Studio quirk not copied: changing a live table's `MajorAxis` used sizes
  from the layout before.

## UIPageLayout

- Each page has a slot as big as the area along `FillDirection`,
  `Padding` apart (its scale of the area's length), and sits in it by the
  alignments; the current page's slot is in view. Hidden pages take no
  slot and keep their place. `AbsoluteContentSize` is the slots along;
  across it's the area (Inference: the measured pages filled it).
- With no `CurrentPage`, the layout sets it to its first page.
- `JumpTo`, `JumpToIndex`, `Next` and `Previous` set `CurrentPage` at
  once. With `Animated`, the view eases there over `TweenTime` by
  `EasingStyle` and `EasingDirection` (Back Out, the default, overshoots a
  400-pixel page by 40), then `Stopped` fires with the page; without it
  the view moves at once and nothing fires.
- `Next` on the last page and `Previous` on the first do nothing unless
  `Circular`, which goes round. Animated, the view ends on the page;
  without `Animated` it stays past the end (a step back from the first
  page leaves the view one page before it, the last page far off to the
  right) until the next jump. An index out of range does nothing.
- `PageEnter` and `PageLeave` fired only when the layout's parent clips,
  in an order not worked out yet; Rook doesn't fire them. The wheel, touch
  and gamepad don't turn pages in Rook yet.

## CanvasGroup

- Rook draws the group's background and what it holds into a texture of
  the group's size in pixels as drawn, then lays it over its place,
  turned with it, faded by `GroupTransparency`, tinted by `GroupColor3`,
  cut by its UICorner and colored by its UIGradient; its UIStroke goes
  over the texture. What it holds is clipped to it. This follows the
  creator docs and Studio's flags (`FFlagCanvasGroupRenderStrokeOnTop`);
  the look is Inference.
- Studio 0.744 on this machine drew every CanvasGroup piece by piece
  instead, in edit mode and in a playtest alike: children unclipped,
  `GroupTransparency`, `GroupColor3` and the UICorner and UIGradient
  reaching only the group's own background, the rest as plain frames.
  Studio has a flag for drawing groups one by one when it runs out of
  texture memory (`DFFlagCanvasGroupIndividualDrawWhenOOM`, off), so this
  looks like a fallback, not how players see it; why it fell back here
  isn't known.
