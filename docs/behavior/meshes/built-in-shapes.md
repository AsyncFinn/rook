# Built-in shapes

Checked against Studio 0.742 on 2026-10-12.

SpecialMesh's `MeshType` shapes, BlockMesh and CylinderMesh, drawn without
a mesh file.

**What each SpecialMesh `MeshType` draws** (observed with one 4 × 4 × 4
part per type, set by script because the
Properties panel hides the deprecated types):

| MeshType | Drawn as | Rook geometry |
|---|---|---|
| Brick | The part's box | The existing block |
| Torso | Identical to Brick (Roblox made it so in 2018; the old sloped torso is gone) | The existing block |
| Wedge | A wedge | The existing wedge |
| CornerWedge (deprecated) | The same shape as a CornerWedge part (checked by overlapping the two) | The existing corner wedge |
| Cylinder | A cylinder along X, like a Cylinder part | The existing cylinder |
| Sphere | A sphere (creator-docs: unlike a Ball part, it stretches on every axis) | The existing ball, scaled per axis |
| Head | The head mesh (below), fitted to the part | Rook's own head mesh, matching the measured one |
| Prism, Pyramid, ParallelRamp, RightAngleRamp (deprecated) | Nothing | None: draw nothing |

The existing shapes live in `rook-types/src/part/shape.rs` (`PartShape`), so
SpecialMesh types reuse them rather than defining their own. A SpecialMesh
changes only how a part looks: the invisible types still collide and are
hit by queries. Sphere and Cylinder meshes showed the part's surface studs,
which matters for the textures step.

**The Head MeshType**, everything measured so far:

- *What it draws:* the mesh file `content/avatar/heads/head.mesh` in
  Studio's install (a version 2.00 mesh, 28,789 bytes, about 1.2 studs each
  way at its own size). Proof: with that file renamed, setting `MeshType`
  to Head changed the enum but not the drawing (a Sphere stayed a sphere,
  with no error), and the executable names `rbxasset://avatar/heads/head.mesh`
  among the render mesh manager's strings.
- *How it's sized:* the mesh is scaled by (min(X, Z), Y, min(X, Z)) / 1.25
  of the part's Size. So it's round from above and never stretched along the
  longer of X and Z (on 2 × 1 × 50 and 50 × 1 × 2 it is a 2-stud head).
  Found by overlapping it with a FileMesh of the same mesh: on 2 × 2 × 2 the
  file at scale 1.60 sat just inside it and at 1.61 just covered it, with
  no rim anywhere; on 3 × 1 × 2 and 10 × 1 × 2 the file at
  (1.61, 0.805, 1.61) covered it the same way, seen from the side and from
  above.
- *Its shape* (measured from the user's copy with a throwaway tool,
  2026-10-12; 517 vertices, 846 triangles): a solid of revolution about Y
  with 36 segments, the first 5 degrees from +X. Its profile is a cylinder
  of radius R = 0.6012 whose straight side runs from -R/2 to R/2, each rim
  rounded by a quarter circle of radius R/2 in 4 steps of 22.5 degrees,
  and flat round caps (the top a fan, the bottom a grid). Normals are
  smooth around and all along the profile, so the rims blend into the
  caps. Rook's head is built from exactly these facts.
- *With `Scale`:* the Size rule applies to Size × Scale (see below). The
  classic R6 head, a 2 × 1 × 1 part with Scale 1.25, is therefore
  min(2.5, 1.25) = 1.25 wide and 1.25 tall before the ÷ 1.25, so it draws
  at the mesh's own 1.2 studs.

**`rbxasset://fonts/*.mesh`**, the legacy paths archived places use for
the character meshes (head, torso, arms, legs, VikingHelmet, the Composit*
clothing meshes): these are no longer files in the install. Roblox
uploaded each one in 2009 as an ordinary asset by @Roblox whose
description is its old path (`head.mesh`, asset 12221462, description
`\fonts\head.mesh`, created 2009-06-24, updated 2017-08-23; `leftarm.mesh`
12221505, `rightleg.mesh` 12221626), and Studio's executable holds a table
pairing each old path with its asset id (the id comes just before its
path). Asset 12221462 is byte-identical to the install's `head.mesh`: two
entries in Studio's download cache (`rbx-storage`) are a 37-byte `RBXH`
header followed by exactly that file. One of them was written while
`fonts/head.mesh` FileMeshes were first loaded, with the local file still
present, and `fonts/head.mesh` kept loading after the local file was
renamed, so the old path is fetched as its asset (Inference from the
timing; not yet proved by forcing a fresh download). Step 2 confirmed it
and built Rook's own path → asset id table (see "Asset fetching" below).

So the Head MeshType, `rbxasset://fonts/head.mesh` and asset 12221462 are
the same mesh. They differ only in sizing: the Head type is fitted to the
part's Size as above, while a FileMesh ignores Size and is sized by `Scale`
alone.

Two traps when comparing meshes in Studio: a FileMesh ignores the part's
Size (only `Scale` sizes it, so `head.mesh` stays about 1.2 studs on any
part), and an unexpected `Scale` on the mesh (one test had X at about 3.07)
changes the shape completely. Check both before concluding two meshes
differ. To compare shapes, size the FileMesh by hand to where the other one
should be and overlap them in two colors; to compare sizing, give both the
same Size and Scale.

**How `Scale` combines with Size** (measured 2026-10-12: every type on a
2 × 3 × 4 part, at Scale 1 and at Scale (0.5, 1.5, 0.75), front and top
views): Scale multiplies the part's Size per axis first, and each type's
shape rule then works on that scaled size. With S = Size × Scale:

| Type | Drawn size | Measured at Scale (0.5, 1.5, 0.75): S = (1, 4.5, 3) |
|---|---|---|
| Brick, Wedge, CornerWedge, Sphere, BlockMesh | S | 1 × 4.5 × 3 |
| Cylinder (SpecialMesh; along X) | length S.X, diameter min(S.Y, S.Z) | 1 long, 3 across, round |
| CylinderMesh (along Y) | height S.Y, diameter min(S.X, S.Z) | 4.5 tall, 1 across, round |
| Head | the mesh × (min(S.X, S.Z), S.Y, min(S.X, S.Z)) / 1.25 | 0.95 × 4.29 × 0.95 (predicted 0.96 × 4.32 × 0.96) |

Applying Scale after the shape rule would have given a 1.5-stud CylinderMesh
across Z and a 1.44-stud Head across Z, so the order is settled. Note the
two cylinders differ: SpecialMesh `Cylinder` lies along X like a Cylinder
part; `CylinderMesh` stands along Y. At Scale 1, every type's sizes equal
the part's Size (2 × 3 × 4, Cylinder 3 across, CylinderMesh 2 across, Head
1.9 × 2.9 × 1.9).

**More rules found while building step 1** (Studio, 2026-10-12):

- *Several meshes on one part:* the last child counts. A SpecialMesh
  Sphere then a CylinderMesh drew the cylinder; the other order drew the
  sphere; changing the losing mesh changed nothing; removing the winner let
  the other draw.
- *A mesh replaces the part's own shape:* a Brick SpecialMesh on a
  Cylinder part draws a box.
- *A file mesh with an empty `MeshId`* (the FileMesh class, or SpecialMesh
  `MeshType` FileMesh) draws a 1 × 1 × 1 box times `Scale`, whatever the
  part's Size, without the part's Neon glow.
- *`Offset`* moves the mesh in the part's own axes (a part turned 90
  degrees about Z with Offset (0, 2, 0) drew 2 studs along world -X), as
  documented. `VertexColor` wasn't measured; it waits for textures.

**Checked against Studio** (2026-10-12): every type at Scale 1 and at
(0.5, 1.5, 0.75) on 2 × 3 × 4 parts, plus Heads on
2 × 1 × 2, 2 × 1 × 50 and the R6 head, rendered in Studio and in
`rook-player --screenshot` with the same cameras (FieldOfView 2, 600 studs
away, about 26 px/stud, front and top) and compared as red silhouette
masks: no pixel differs more than 2 px from the silhouette edges (the rest
are one-pixel rasterization rims, about 2,700 px over six 308 × 538
views). Close-ups of the three Heads (about 103 px/stud) match Studio's
outline to a pixel in width and height.
