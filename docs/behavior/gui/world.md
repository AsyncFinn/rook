# World GUI

Checked against Studio 0.743 on 2026-10-21.

[M8](../../roadmap/m8-gui.md) step 3. SurfaceGui is in 1,854 archive files (about 200,000
of them), BillboardGui in 937. Measured in Studio 0.743 on 2026-10-21 in
edit mode on Baseplate, from `screen_capture` shots at FieldOfView 70 in
a 1270 × 788 viewport (world GUI shows in them, a moment after the
camera moves) and `AbsoluteSize` reads. A same-scene comparison (a sign
with a frame and scaled text, a name tag above it) lands on the same
pixels in Rook and Studio.

Code: which world GUIs show, their canvas sizes and places in
`rook-gui`'s `world` module; their contents lay out and draw as a
ScreenGui's do ([GUI](layout.md)); the canvas atlas and the world pass in
`rook-render/src/renderer/gui/world.rs` and `shaders/world_gui.wgsl`.

## Which show

- A SurfaceGui draws on its `Adornee`, or else its parent, which must be
  a BasePart; a BillboardGui hangs on its `Adornee` or parent, a BasePart
  or an Attachment. Inference: Models as adornees aren't drawn yet.
- They draw from the Workspace and from the local player's PlayerGui
  (with an `Adornee` in the Workspace), when `Enabled`. Inference: never
  from StarterGui or storage, and a BillboardGui hides from its
  `PlayerToHideFrom`.
- Every world GUI is laid out wherever it is, so scripts read its
  `AbsoluteSize` and its contents' places. A client lays out those it
  draws with the rest of its GUI each frame; a server, which draws none,
  lays one out only when a script reads it, and its BillboardGuis keep
  the size they had then.

## Defaults

`Instance.new` gives both `LightInfluence` 0 and `Brightness` 1, and
`ClipsDescendants` off; a SurfaceGui `CanvasSize` 800 × 600, SizingMode
FixedSize, `PixelsPerStud` 50 and `MaxDistance` 0 (no limit, per
creator-docs); a BillboardGui `Size` {0, 0}, {0, 0} and `MaxDistance`
inf.

## SurfaceGui

- **Canvas**: `CanvasSize`, or with SizingMode PixelsPerStud the face's
  size times `PixelsPerStud` (a Right face 1 × 2 studs at 25: 25 × 50).
  It's stretched over the whole face: on an 8 × 6 face, 800 × 600 put a
  200 × 100 frame on 2 × 1 studs.
- **Orientation**, from a cube with a canvas on every face, each with a
  frame in its top left: on the four sides the canvas lies as a Decal
  does (Front: top left at +X +Y, x along −X; Back: −X +Y, along +X;
  Right: +Z +Y, along −Z; Left: −Z +Y, along +Z). On the **Top** its top
  left is at −X +Z and x runs along −Z, y along +X; on the **Bottom** its
  top left is at +X +Z, x along −Z, y along −X. Both are a quarter turn
  from a Decal's.
- **Spilling**: with `ClipsDescendants` off, what lies past the canvas
  still draws, past the part's edges (a 300 × 300 frame at (700, 500) on
  an 800 × 600 canvas hung 2 studs off the side and bottom). With it on,
  the canvas clips. Rook draws up to 2048 pixels past each edge.
- Inference: a SurfaceGui isn't seen from behind its face; Rook lifts it
  0.01 studs off the face.

## BillboardGui

- **Size**: `Size`'s scales are studs, seen at the billboard's depth
  along the camera's view, and its offsets are pixels: at depth 40 a
  {4, 0}, {2, 0} billboard drew 56 pixels across (FieldOfView 70, 788
  high) and a {0, 100}, {0, 50} one 100 × 50 at every distance; one off
  to the side at the same depth drew the same. Its contents lay out at
  that pixel size, so `TextScaled` text grows as the camera nears. Rook
  rounds the size to whole pixels (Studio's `AbsoluteSize` read whole
  numbers) and lays out again when it changes.
- Studio's edit-mode `AbsoluteSize` for scale sizes didn't follow what
  it drew (812, 303, 170, 130 across at depths 10, 20, 40, 80 where it
  drew 225, 112, 56, 28 plus offsets); Rook reports what it draws.
- **Where**: at the part's position, or the Attachment's world position,
  plus `StudsOffsetWorldSpace`, plus `ExtentsOffsetWorldSpace` times
  half the part's size (`ExtentsOffset` 1 up put it at the top of a part
  4 high); then `StudsOffset` and `ExtentsOffset` in the camera's frame
  (x right, y up, z towards the camera; −3 x moved it 3 studs left on
  screen). `SizeOffset` moves it by that share of its own size, x right
  and y up.
- **Depth**: it's hidden by what's in front of its anchor point (a part's
  front face hid the lower half of a billboard at the part's center)
  unless `AlwaysOnTop`, which draws it over the world.
- Not yet: `DistanceLowerLimit`, `DistanceUpperLimit` and `DistanceStep`.

## Drawing

- **Color**: with `LightInfluence` 0 a world GUI's color goes through
  the tone curve as an unlit adornment's does: a red frame drew 223, as
  in Rook. `LightInfluence` 1 lights it as a surface (Approximation:
  particles' lighting); `Brightness` multiplies the unlit color.
- **Order**: Rook draws world GUI after the particles, back to front,
  then those `AlwaysOnTop`, under the screen's GUI. Inference: SurfaceGui
  `ZOffset` isn't used yet.
- `MaxDistance` hides one further than that from the camera.
- Rook draws every canvas into one atlas (at most 4096 pixels square)
  and lays each in the world as a rectangle; canvases over 2048 pixels,
  or too many to fit, draw smaller.
