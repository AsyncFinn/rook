# Trusses

Checked against Studio 0.742 on 2026-10-13.

TrussPart is in 371 archive files (31,487 uses). How Rook builds and
batches them is in [architecture](../../design/architecture.md#rendering-rook-render); what's left is in
`docs/guide/stubs.md`.

*Measured from Studio's OBJ export* of a 2 × 2 × 2 and a 2 × 4 × 2 truss
(the exports are kept outside the repository; Confirmed): bars are 0.25
studs square. Four rails run the truss's whole length at its corners, flush
with its 2 × 2 box. The truss is cut into 2-stud segments; each has a rung
across each of its four sides at both ends, between the rails, and where
two segments meet their rungs are one bar 0.5 studs thick. A brace is 0.25
studs wide and 0.125 deep, in the plane of the rails' centers (0.875 from
the axis), running from one corner of its segment 0.875 below its middle
to the opposite corner 0.875 above it, its ends cut square and buried in
the rails (its outline reaches 0.9634 studs from the segment's middle).
Faces hidden where bars meet are left out and normals are flat. The 2 × 2
× 2 export has no braces (48 quads: the 12 edges of its box); the 2 × 4 ×
2 has 96 quads, 32 of them braces. The UVs are planar at 1/8 of a unit per
stud into the surface-pattern strip (`Truss1_diff.png` is the studs strip),
which Studio doesn't draw on trusses; the install has no truss texture
(only the Insert Object icons), so Rook draws the bars in the part's
color and material.

*Observed in Studio 0.742* (2026-10-13, a dark block inside each truss so
only its near side showed, front and side views):

- *Style:* AlternatingSupports braces all four sides of every segment, the
  first segment from the bottom leaning one way round the truss and the
  next the other (checked on 2 to 5 segments). BridgeStyleSupports braces
  only the truss's two X sides, the lower floor(n / 2) segments leaning one
  way and the rest the other, the two sides alike seen through the truss.
  NoSupports draws rails and rungs only. Exact directions are in
  `rook-types/src/part/truss.rs`.
- *The lead that a 2 × 2 × 2 draws like NoSupports and longer ones like
  AlternatingSupports whatever Style says* held only in part: a truss of
  one segment (2 × 2 × 2, 2 × 3 × 2, 2 × 1 × 2) draws no braces whatever
  its Style (Confirmed), but longer ones follow their Style (Confirmed:
  BridgeStyleSupports and NoSupports at 4, 6, 8 and 10 studs drew their own
  look, not AlternatingSupports). The lead about merging turned out to mean
  the rungs inside one truss: where two segments meet they are one 0.5-stud
  bar (Confirmed by the export). Separate trusses never merge (Confirmed:
  side by side, end to end and at a right angle, two trusses in two colors
  each drew all their own bars).
- *Length:* floor(length / 2) segments, at least one; the drawing starts at
  the box's low end along the truss's axis and the rest of the box stays
  empty (2 × 5 × 2 draws 4 studs from its bottom). A truss shorter than 2
  draws one segment centered on the part.
- *Axis:* a truss runs along the axis that isn't 2 studs. Long in Z, its
  own X stays X and its own Z points down; long in X, its X points to the
  Front and its Z down (found from which sides carry braces and how they
  lean).
- *Size* (script-visible; conformance case `content/truss`): each axis is
  kept between 0.001 and 512. When two axes are exactly 2 the third keeps
  its length, shorter than 2 or not; otherwise the longest axis (X before Y
  before Z on a tie) keeps its length, at least 2, and the others become 2:
  (3, 3, 3) reads back (3, 2, 2), (6, 2, 10) (2, 2, 10), (1, 1, 1) (2, 2,
  2), (2, 1, 2) stays. The Size before doesn't matter.
- *Looks:* Color, Transparency, Neon and Reflectance show as on any part,
  and Wood shows its wood texture (Rook draws it plain, like every 2022
  material). Decals, Textures and surface patterns don't show. A Sphere
  SpecialMesh still replaces the truss (step 3), but Studio cuts a grid of
  holes through it; Rook draws it solid.
- *Files:* `Style` is saved as `style`, which the reflection database holds
  as a separate property, so Rook read every saved truss as
  AlternatingSupports until scripts' `Style` became a view of it (found on
  a large 2016 place, whose 596 trusses save 517 AlternatingSupports, 77
  NoSupports and 2 BridgeStyleSupports).

*Compared with Studio* (`rook-player --screenshot --fov 20` against
`screen_capture`, 1270 × 795, one test scene run by the same script in
both): each Style at 2 × 2 × 2 and 2 × 10 × 2, long trusses along X and Z
in two Styles, 2 × 5 × 2, 2 × 1 × 2, a red BridgeStyleSupports 2 × 7 × 2, a
Neon truss tilted 30 degrees and a truss with a Sphere mesh, from the
front, from above and at an angle: every bar's placement and every brace's
direction match by eye, apart from lighting and the Sphere's holes.
A large 2016 place draws its 596 trusses, its NoSupports ladders without
braces.
