# Adornments

ROADMAP M7 step 6. Measured in Studio 0.742 on 2026-10-20 on Baseplate,
seen square on at a 3-8° field of view. Corpus counts are files saving
each kind (and creating it with `Instance.new`): SelectionBox 676 (749),
SelectionSphere 45 (15), SurfaceSelection 12 (89), the Box, Sphere,
Cylinder and Cone HandleAdornments 6 to 13 each (34 to 82),
LineHandleAdornment 26 (49), ImageHandleAdornment 57 (220), Handles 37
(264), ArcHandles 11 (68), FloorWire 33 (51), SelectionPartLasso 50
(362). Highlight has its own page, [highlight.md](highlight.md).

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
at 191, 67, 78. Not yet: `ZIndex`.

The next three were measured in a 1253 × 773 viewport at fields of view
of 8° and 30°, 40 to 150 studs away. Thin lines' colours aren't
readable from Studio's JPEG capture; where they are, they draw like the
shapes above, without light through the curve.

- **LineHandleAdornment**: a line from its `CFrame` `Length` (5) along
  the LookVector, about 0.35 pixels across per unit of `Thickness`
  whatever its distance and field of view: 20 drew 7 pixels, 10 drew
  3.5, 4 drew 1.4, and the default 1 a faint hairline. Rook draws it as
  a bar kept that many pixels across at every point along it
  (`rook_render::Grow::Across`), and one thinner than a pixel one pixel
  across, faded by how much of it the line would cover.
- **ImageHandleAdornment**: `Image` (default `SurfacesDefault.png`) on a
  flat `Size` (1, 1) studs at its `CFrame`, turned with it (one turned
  60° drew foreshortened) and facing along the LookVector, the image's
  left at the CFrame's +X; from behind it isn't drawn. `Color3`
  (default 242, 243, 243) tints the image: red over a white image drew
  (218, 9, 14). Rook also cuts it out by the image's alpha (Inference),
  and draws a plain `Color3` square when the image can't load.
- **WireframeHandleAdornment**: nothing until a script adds lines, in
  its CFrame's axes: `AddLine(from, to)`, `AddLines` (a line per pair of
  points), `AddPath(points, loop)`, and `Clear` empties it. Lines are
  one pixel across in `Color3`, whatever `Thickness` (1 and 5 drew
  alike), and parts in front hide them. One whose `Thickness` was set to
  10 before its `AddPath` drew nothing, unexplained; Rook draws it.
  Rook also scales its points by `Scale` (Inference). `AddText` isn't
  drawn yet.

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

Deprecated, but it draws. Measured in Studio 0.742 with parts 20 studs
apart on every axis (one over a platform), and in 0.743 (2026-10-20)
with pairs of parts in one plane seen square on at a 1° field of view
(38.5 px/stud) and a four-colour test image:

- It needs both `From` and `To`, and draws nothing until it has them.
- **It's laid along the floor**, keeping 0.2 studs off every surface
  whatever its radius or the parts' sizes: from `From`'s centre along X
  towards `To` (along Z when they line up in X) to 0.2 past `From`'s
  face, straight down to 0.2 above the top of what's under it (a
  platform, not the baseplate below it), along X to `To`'s X, then along
  Z, and straight up (or down) 0.2 short of `To`'s face, level with
  `To`'s centre. The bits inside the two parts aren't drawn (a 1-stud
  part's wire starts 0.7 from its centre, a 4-stud part's 2.2).
- Tubes `WireRadius` thick (0.0625 by default) in `Color3`, lit, with a
  ball about 1.25 × `WireRadius` at the inner corners, so they bulge;
  the two ends are flat.
- **`Texture`** draws as images on the wire, none by default:
  - Each is `TextureSize` (1, 1 by default) studs, X along the wire and
    Y across it, centred on it, as **two crossed quads**: both hold the
    wire, and they're fixed in the world, not turned to the camera. The
    image's width runs from `From` towards `To`, so on a rising or
    falling leg it stands on its side; that's the "rotated going up"
    look.
  - Across axes, by the leg's direction: up, -X and -Z; down, -X and +Z;
    +X, -Y and +Z; -Z, -X and -Y. Those are -X and +Z turned by the
    shortest arc from -Y to the leg (about X when it points up); Rook
    uses that rule for -X and +Z legs, which weren't seen.
  - One every `StudsBetweenTextures` (4) studs along the drawn wire,
    counted back from its end at `To` (the first at that end); changing
    `To`'s height moves them all, a bigger `To` part does not. An image
    whose centre is past either end isn't drawn; one near a corner
    reaches straight past it.
  - `Velocity` (2) moves them towards `To` at that many studs a second
    (1 moved 0.33 studs in 0.35 s), and `CycleOffset` puts them that
    share of the spacing further on (0.25 at spacing 4: one stud; 1: no
    change).
  - Unlit, through the tone curve (255 draws 222), not tinted by
    `Color3`, and faded by `Transparency` with the wire.

Rook draws the wire with its `From` part and the images as the
renderer's ribbons (`rook_render::FloorWire`), moved each frame. Moving
either part redraws it. It finds faces and the floor from the Workspace
parts' world boxes (Approximation: a turned part's box reaches past it),
under where the wire drops (Inference). Not measured: turned parts, a
`To` straight above `From`, and where Studio's moving images start in
time.

## SelectionPartLasso

Deprecated, but modern Studio still draws it (observed in 0.742): a bar
0.25 studs across from its Humanoid's root part to `Part`, centre to
centre, so the two parts hide its ends; 9 pixels 40 studs away, 3 at
120. With both a HumanoidRootPart and a Torso it starts at the
HumanoidRootPart (Rook falls back to the Torso, Inference). `Color3`
without light, through the curve (red drew 223, 8, 13).

Rook draws it as a round bar with the root part (Inference: only seen
side on), and moving either end draws it again.
