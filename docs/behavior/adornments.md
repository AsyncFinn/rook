# Adornments

ROADMAP M7 step 6. Measured in Studio 0.742 on 2026-10-20 on Baseplate,
seen square on at a 3-8° field of view. Corpus counts are files saving
each kind (and creating it with `Instance.new`): SelectionBox 676 (749),
SelectionSphere 45 (15), SurfaceSelection 12 (89), the Box, Sphere,
Cylinder and Cone HandleAdornments 6 to 13 each (34 to 82), Handles 37
(264), ArcHandles 11 (68), FloorWire 33 (51).

Every kind here draws only on its `Adornee` (Observed: one merely inside
a part draws nothing), wherever it is in Workspace. Rook also draws those
under a PlayerGui (Inference: adornments draw there in Roblox). `Visible`
off hides them, `Transparency` fades them, the default `Color3` is
(13, 105, 172), and parts in front hide them unless noted. Rook draws
them as extra entries riding along with their part
(`apps/rook-player/src/scene/adornments/`), and any change to one
redraws its part.

**Colours.** Studio draws most adornments in their colour without light,
through the tone curve: red (1, 0, 0) draws (222, 0, 0), as a flat
colour does. A few draw in exactly their colour, past the curve: red
draws (253, 0, 0). Rook draws both ways (`rook_render::Light::Unlit` and
`Exact`).

## SelectionBox

- **Edges** (Measured, a 4-stud cube, LineThickness 0.5): each edge is a
  bar `2 × LineThickness` across, centred on the part's edge, so the
  outline reaches `LineThickness` past the part (2.5 studs from the
  centre) and `LineThickness` in (1.5).
- **Colour**: `Color3` without light, through the curve. `Transparency`
  fades the lines.
- **Fill**: `SurfaceColor3` over the part's box at `SurfaceTransparency`
  (default 1: none); blue at 0.5 over grey 128 draws (25, 35, 190).

Rook draws twelve bars and a fill box 0.02 studs outside the part's
faces, so the two don't flicker. The X bars cover the corners and the
others stop at them, so a see-through box doesn't darken its corners
(Inference: not checked in Studio). Not yet: a Model as Adornee (its
bounding box).

## SelectionSphere

- **A ring facing the camera** (Measured, cubes of 1, 2, 4 and 10 studs
  and a 4 × 2 × 6 box, from in front and from 45° above): a flat ring of
  about 24 sides that looks the same from any side, from `s/2 - 0.8` to
  `s/2 + 0.2` studs out, where `s` is the part's smallest side. A 10-stud
  cube's ring runs from 4.2 to 5.2 studs; below about 1.6 studs it has no
  hole (a 1-stud side gives a 0.7-stud disc).
- **Colour**: exactly `Color3`, past the curve (253, 0, 0 for red).
- **Fill**: `SurfaceColor3` at `SurfaceTransparency` (default 1: none),
  through the curve: blue draws (0, 0, 226). It is a disc of radius
  `s/2` under the ring: opaque it shows only in the hole, at 0.5 it
  tints the ring's inner part.

Rook draws the ring as a flat mesh the shader turns to the camera, and
fills the hole only (the tinted band under a see-through fill is
missing).

## SurfaceSelection

- **An outline of one face** (Measured, a 4-stud cube's Front and Right
  faces): four flat bars on `TargetSurface` (default Right), 0.4 studs
  wide and centred on the face's edges, reaching 0.2 past them. Seen
  from the side, nothing shows on the neighbouring faces.
- **Colour**: `Color3` without light on the Right face (218-223 for
  red); the Front face drew darker (185), unexplained.

Rook draws four bars 0.02 studs thick, a hair outside the face, in
`Color3` through the curve.

## HandleAdornments

Box, Sphere, Cylinder and Cone (Measured, from in front and from the
side):

- **Placement**: the shape sits at `CFrame` from the part's centre, in
  its axes, shifted by `SizeRelativeOffset` × half the part's size (an
  offset of (0, 0, 2) on a 2-stud part moved a box 2 studs).
- **Shapes**: a box of `Size`; a sphere of `Radius`; a cylinder of
  `Radius` along the CFrame's Z, `Height` long and centred, with a hole
  of `InnerRadius`; a cone whose base of `Radius` is at the CFrame and
  whose tip is `Height` along the LookVector.
- **`Shading`**: Default draws `Color3` without light, through the curve
  (223 for red); Shaded is lit (217 on a face half to the sun); XRay and
  XRayShaded draw the same and also show where parts in front hide them,
  at about half strength (red behind grey 126, 152, 165 drew
  190, 64, 75); AlwaysOnTop draws in exactly `Color3`, past the curve.
- **`AlwaysOnTop`** true draws in exactly `Color3` too. Studio's MCP
  capture leaves out what it draws over everything: there, both
  AlwaysOnTop kinds are hidden by parts in front, and those under
  CoreGui with ZIndex 1 vanish entirely. Rook follows the API reference
  and draws them over everything (Inference), after transparent parts
  (particles still draw over them).

Rook draws XRay as the shape plus a copy at half its opacity over
everything: where the shape shows, the same colour over itself changes
nothing. The same scene in `rook-player --screenshot` draws Default red
at 224, Shaded at 215, 24, 32 (Rook's light, M13), and the XRay ghost
at 191, 67, 78. Not yet: LineHandleAdornment (its `Thickness` drew
1-2 pixels at 150 studs, neither studs nor pixels clearly),
ImageHandleAdornment and WireframeHandleAdornment, and `ZIndex`.

## Handles and ArcHandles

Measured on 1-, 4- and 8-stud cubes, 60 to 120 studs away, at fields of
view of 20° and 40°. Not deprecated: Studio's own move dragger is built
from Cone and Cylinder HandleAdornments. Dragging them comes with GUI
input (M8).

- **The grips keep one size on screen**: a ball is 2.2 studs across 60
  studs away and 4.3 at 120, the same at either field of view (so they
  grow with distance, not with the view's width). Where they sit stays
  in studs.
- **Handles**, on each of `Faces` (all six by default), in `Color3`, lit
  and a little see-through:
  - Style Resize (default): a ball whose centre is 2 studs out from the
    face (4.0 from a 4-stud cube's centre, 6.0 from an 8-stud cube's).
  - Style Movement: an arrow out of the face: a shaft from about 0.4
    studs off it and a cone, reaching 11.3 studs past the face 60 studs
    away (0.19 × the distance); the cone is 0.04 × the distance long and
    0.021 × wide, the shaft 0.0057 × thick.
- **ArcHandles**, for each of `Axes`: a thin ring around the axis (X red,
  Y green, Z blue), about two pixels thick, its radius the part's
  bounding-sphere radius plus 2 studs (2.9, 5.5 and 8.9 for 1-, 4- and
  8-stud cubes); on it a grip ball at either end of the next axis (the X
  ring's at ±Z, Y's at ±X, Z's at ±Y), coloured like the ring.

Rook builds the grips at their size 100 studs away and scales each about
its anchor (a ball's centre, an arrow's foot on the face) by its
distance from the camera in the vertex shader
(`rook_render::PartInstance::grows`). Its rings are 0.1 studs thick in
the world, and the see-through amount (0.2) is a guess from the part
showing through Studio's grips. The same scenes in `rook-player
--screenshot` put every grip, arrow and ring on Studio's pixels to within
a few.

## FloorWire

Deprecated, but it draws. Measured with parts 20 studs apart on every
axis, one of them over a platform:

- It needs both `From` and `To`, and draws nothing until it has them.
- **It's laid along the floor**: from `From`'s centre straight down to
  the top of what's under it (a platform, not the baseplate below it),
  along X, then along Z, at that height (the wire's centre on the top),
  then straight up (or down) to `To`'s centre.
- Tubes `WireRadius` thick (0.0625 by default) in `Color3`, lit, with a
  ball about 1.25 × `WireRadius` at the corners, so they bulge.

Rook draws it with its `From` part and finds the floor from the
Workspace parts' world boxes (Approximation: a turned part's box reaches
past it). Not yet: its moving `Texture`, and redrawing when only `To`
moves.
