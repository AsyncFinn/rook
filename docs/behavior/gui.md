# GUI

ROADMAP M8. ScreenGui is in 600 archive files, Frame in 558. Measured in
Studio 0.742–0.743 on 2026-10-21, in a playtest client on Baseplate, a
1561 × 793 window, by reading `Absolute*` properties and
`PlayerGui:GetGuiObjectsAtPosition` (which lists objects top first).
The conformance cases `gui/layout`, `gui/ui_layouts` and
`gui/layout_timing` hold the decisive values; Rook passes all three.

Code: layout and drawing order in `rook-gui`; the script side in
`rook-lua` (`api/instance/access.rs` reads, `api/methods/gui.rs`); the
GUI pass in `rook-render/src/renderer/gui.rs`.

## The screen

- **The top bar** is 58 pixels: `GuiService:GetGuiInset()` gives
  (0, 58) and (0, 0) on a client. A ScreenGui is the window less those 58
  pixels, starting below them (1561 × 735 at (0, 0) in GUI coordinates);
  with `IgnoreGuiInset` it's the whole window, at (0, −58). Inference:
  `ScreenInsets` DeviceSafeInsets and None have no inset on a desktop.
- A server has no top bar: its `ScreenPointToRay` and `ViewportPointToRay`
  agree (conformance `physics/queries`).
- **The mouse** in GUI coordinates: `Mouse.X`/`Y`, `Mouse.ViewSizeY` and
  mouse InputObjects' `Position` all exclude the inset;
  `UserInputService:GetMouseLocation()` alone includes it (the same
  point read (500, 350) and (500, 408)).
- Rook draws no top bar yet ([CoreGui](../../ROADMAP.md)), so the strip
  shows the world.

## Layout

- **Position and size**: each `UDim`'s scale is of the parent's size (or,
  inside a UIPadding, of what's left inside it), plus its offset.
  `AnchorPoint` moves the object back by that share of its own size.
  Nothing is rounded: a third of 200 is 66.67. `UDim` offsets are
  integers, so `0.5` offsets read 0. Negative sizes stay negative.
- **SizeConstraint** RelativeXX or RelativeYY scales both axes of `Size`
  by the parent's width or height; `Position` still uses both.
- **Folders** pass through: a Frame in a Folder is placed in the Folder's
  parent. A Frame with no GUI ancestor is placed in nothing: its offsets,
  at (0, 0).
- **Rotation** turns an object about its centre, whatever its
  `AnchorPoint`. Children turn with it: their centre is turned about the
  parent's centre, and `AbsolutePosition` is that centre less half the
  size (a 10 × 10 child at the corner of a 100 × 50 frame turned 90°
  read (165, 75) from (100, 100)). `AbsoluteRotation` adds up.
- **Visible** off doesn't stop layout: hidden objects keep reading their
  place, and so does everything under a disabled ScreenGui.
- **UIScale** scales an object about its anchor point, with everything
  in it; under a ScreenGui it scales the ScreenGui's contents from its
  top left, not the ScreenGui's own `AbsoluteSize`. Rook uses the first
  UIScale.
- **UISizeConstraint** clamps the size between `MinSize` and `MaxSize`,
  before `AnchorPoint` moves the object. One with `MinSize` above
  `MaxSize` on either axis does nothing at all.
- **UIAspectRatioConstraint** (width / height = `AspectRatio`), after the
  size constraints: FitWithinMaxSize takes the largest size of that ratio
  inside the object's size, whatever `DominantAxis` says.
  ScaleWithParentSize keeps the dominant axis and fits the result inside
  the parent when it's bigger (ratio 4, Height, a 200 × 150 object in
  400 × 300: 400 × 100). One under a UIGridLayout holds its cells.
- **UIPadding**: left and right scale with the object's width, top and
  bottom with its height.

### UI layouts

The first UIListLayout or UIGridLayout among an object's children places
its visible GUI children; a second one changes nothing. Children's
`Position` and `AnchorPoint` don't count. **Hidden children** keep the
place the layout last gave them. Children sort by `SortOrder`:
LayoutOrder, or Name by bytes (`Z` before `a`); ties keep child order.
`AbsoluteContentSize` is what was placed.

- **UIListLayout** (flex and `Wraps` are step 6): items follow each other
  along `FillDirection`, `Padding` apart (its scale of the length that
  way); the run is aligned as a whole along it, each item on its own
  across it. Content is the run's length by the widest item.
- **UIGridLayout**: every child takes `CellSize`; as many cells fit along
  `FillDirection` as the area holds, `CellPadding` apart (at most
  `FillDirectionMaxCells` when it's set); lines follow across. The block
  of cells is aligned as a whole, and `StartCorner` mirrors the order
  inside it. `AbsoluteCellCount` is (columns, rows).

### AutomaticSize

An object grows on its `AutomaticSize` axes to hold its visible
children, never below `Size`, then its size constraints apply.
Children's offsets count from the padding, plus the right and bottom
padding; a child anchored at (1, 0) at x = 10 reaches only to 10.
Nested automatic sizes add up from the inside.

- **With a UI layout**, children sized by scale are sized against the room
  left in the parent past the object's `Position`, and the object grows to
  the layout's content (a child 1 wide in a list, 45 pixels into a 400
  box: 355).
- **Without one**, Studio's results for children sized by scale depend on
  earlier layouts (a 0.5-wide child with offset 30 gave a 50-wide parent).
  Rook sizes them against the object's `Size` (Approximation); children
  sized by offset, the usual case, are exact.
- Text (`TextLabel` growing to its text) comes with step 2.

### When layout runs

- **Reading** an `Absolute*` property lays out at once: `AbsoluteSize`
  right after setting `Size` gives the new size.
- Otherwise the frame lays out **after RenderStepped**.
- **Changed** fires only for values that moved since the last layout:
  setting a size and setting it back between frames fires nothing; a
  read that moves a value fires it as a frame's layout would.

## Drawing

- **Order**: ScreenGuis by `DisplayOrder`, later ones on top on a tie.
  ZIndexBehavior Sibling sorts siblings by `ZIndex` (later on top on a
  tie), each drawn with all it holds over it; Global sorts every object in
  the ScreenGui by `ZIndex` alone, later in the tree on top on a tie. A
  ScreenGui made with `Instance.new` is Global.
- **ClipsDescendants** clips what an object holds to its rectangle,
  unless it or an ancestor is turned, which turns clipping off
  (creator-docs; a turned clipper's child was still found outside it).
- **Backgrounds** fill `BackgroundColor3` at `BackgroundTransparency`;
  the border is `BorderSizePixel` of `BorderColor3` outside the
  rectangle (`BorderMode` Outline), across its edge (Middle) or inside it
  (Inset). Inference: the border takes the background's transparency and
  doesn't overlap it, and UIScales thicken it.
- **Blending**: GUI blends over the finished screen's sRGB pixels
  (Inference, as Highlights measurably do): Rook draws GUI premultiplied
  into an 8-bit layer the tone pass lays over the frame.
- Text comes with step 2, world GUI (SurfaceGui, BillboardGui) with
  step 3, images and ScrollingFrame with step 4 and the modern UI objects
  with step 6; until step 3 only the local player's ScreenGuis draw.
