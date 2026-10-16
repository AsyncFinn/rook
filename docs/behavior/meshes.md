# Meshes

What Rook found while building M4's mesh steps: the built-in shapes (step 1), mesh files (step 3) and trusses and unions (step 6). Each rule says where it was observed. The plan itself is in [ROADMAP.md](../../ROADMAP.md#m4-content-meshes-materials-textures-decals).

## Built-in shapes (step 1)

**What each SpecialMesh `MeshType` draws** (observed in Studio 0.742,
2026-10-12, one 4 × 4 × 4 part per type, set by script because the
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
should be and overlap them in two colours; to compare sizing, give both the
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
are one-pixel rasterisation rims, about 2,700 px over six 308 × 538
views). Close-ups of the three Heads (about 103 px/stud) match Studio's
outline to a pixel in width and height.

## File meshes (step 3)

**File meshes (step 3)**, built 2026-10-13. How loading and drawing work
is in `docs/architecture.md` §8 "Rendering"; what's left is in
`docs/behavior/stubs.md`.

*What the archive uses* (`rook corpus-scan`, 3,058 files that load; the
scan now counts mesh instances by kind):

| Kind | Files | Uses |
|---|---:|---:|
| SpecialMesh `MeshType` FileMesh with a `MeshId` | 1,716 | 99,940 |
| SpecialMesh FileMesh with an empty `MeshId` | 162 | 1,838 |
| SpecialMesh of another type with a `MeshId` (ignored) | up to 25 | 444 |
| FileMesh class with a `MeshId` | 6 | 8 |
| MeshPart with a `MeshId` | 74 | 133,246 |
| MeshPart with an empty `MeshId` | 19 | 235 |
| CharacterMesh with a `MeshId` | 166 | 7,254 |
| A mesh under a union | up to 5 | 56 |
| A mesh under a truss | up to 7 | 21 |
| A mesh under a MeshPart | 0 | 0 |

Every MeshPart in the archive saved `InitialSize`. 13 of the 74 MeshPart
files are models from a 2016 Roblox install that saved `MeshID` (capital D)
instead of `MeshContent`; Rook reads either. *Mesh versions*, from `rook
content --place` (which now decodes every mesh reference and totals the
results) over 60 random archive places plus three with many MeshParts, 41
of them with meshes: of the meshes that load anonymously, 358 distinct are
version 1.00, 368 are 2.00, 12 are 1.01, 6 are 4.00, 6 are 4.01, 5 are 3.00
and 1 is 3.01; none is 5.00, 6.00 or 7.00, so reading 6.00 and 7.00 can
wait. 2,833 distinct meshes (34,329 uses, mostly MeshParts) are refused
without an account, so their versions are unknown. A 2017 disaster place: 40 mesh
uses, 39 decode (1.00 and 2.00), 1 refused. A modern core place: 827 uses, 788
decode (1.00, 1.01, 2.00), 39 refused, including all 7 of its MeshParts'
meshes.

*Checked in Studio 0.742* (2026-10-13; observed unless marked):

- *Where a file mesh sits:* the mesh's own origin is at the part's
  position, not its bounds' centre. Version 1 of asset 12221462 is
  centred 11.6 studs off its origin and drew 11.6 studs off the part, as a
  FileMesh and as a MeshPart alike. `Offset` moves it in the part's axes as
  for the other types (checked again: Offset (0, 2, 0) on a head.mesh at
  Scale (5, 3, 5)).
- *MeshPart sizing:* a MeshPart made from a mesh has Size and `MeshSize`
  equal to the mesh's bounds (head.mesh: 1.1978 × 1.2024 × 1.1978). The
  mesh is drawn at its vertices × Size / `MeshSize`, about its own origin:
  setting Size to (0.5, 0.5, 0.25) of `MeshSize` shrank it by exactly that.
  `MeshSize` is what files save as `InitialSize` and doesn't change when a
  script sets Size or when the mesh loads. A MeshPart saved without
  `InitialSize` reads `MeshSize` (0, 0, 0) and draws the mesh at its
  vertices × Size (a 1.2-stud mesh on a 2 × 4 × 8 part drew 2.4 × 4.8).
  A new MeshPart is 4 × 1.2 × 2 with `MeshSize` 0; with an empty `MeshId`
  it draws its Size box (with a placeholder checker texture). Scripts can't
  write `MeshId` ("lacking capability NotAccessible"); conformance case
  `content/meshpart`.
- *Loading:* a new mesh draws nothing until its file arrives. When
  `MeshId` changes, what the mesh drew before stays until the new file
  arrives: 24 loaded heads stayed heads while their new meshes queued
  behind 1,500 other requests, then changed, and 12 empty-`MeshId` boxes
  stayed boxes. A file that fails keeps the old mesh for good: a loaded
  head given a missing asset, a missing file, `abc`, a PNG or asset 1
  stayed a head, while `""` gave the empty-`MeshId` box. A new mesh that
  fails draws nothing (step 2), for MeshParts too (not found, missing file
  and junk drew nothing). Once, with `Scale` changed in the same frame as
  `MeshId`, the meshes still loading drew nothing; Rook doesn't copy that.
  Most loads finished before a capture could catch them (144 uncached
  meshes all appeared within about half a second).
- *PreloadAsync on a client* checks a `MeshId` is a mesh: a PNG reported
  `Failure`, mesh files of versions 1.00 and 2.00 `Success`.
- *Meshes under other parts:* a MeshPart ignores mesh children (a Sphere
  under one left its own mesh drawn). On a union, a mesh child is drawn as
  if Size and `Scale` were 1: a Sphere mesh and a Brick mesh at Scale
  (2, 3, 1) each drew a 1-stud shape, untinted, on 4 × 4 × 4 and 8 × 2 × 2
  unions. A truss treats a mesh child like any part (a Sphere filled a
  2 × 10 × 2 truss; a head.mesh FileMesh drew at its own size).
- *Version 1.00 meshes:* `fonts/sword.mesh` and version 1 of asset
  12221462 drew at the size `rbx_mesh` gives them (half the stored
  positions), within 2 px at 26-50 px per stud. The V flip only shows with
  textures (step 4).

*Compared with Studio* (silhouette masks as in step 1, Studio captures
against `rook-player --screenshot --fov 20` from the same cameras, 1270 ×
549): a test scene (the off-centre mesh front and top, the sword, the head
with Offset, a MeshPart at Size ≠ `MeshSize` and one without
`InitialSize`): every outline within 2 px, 4 px on the sword's tip at 3.9
px per stud. A 2017 disaster place, its maps loaded into Workspace by the same script
in both: a beach map's 8 umbrellas (top within 1 px), a fort's
8 rotated missiles (top within 2 px) and Coastal Quickstop's lighthouse lamp
(within 1 px) and buckets; a modern core place's Christmas tree, 54 rotated
ornaments from front and top. Where masks differ by more (up to 12 px) it
is on sub-pixel fins, antlers and wires, which Studio's antialiasing fades
below the mask's threshold. A modern core place's MeshParts couldn't be compared:
their meshes need an account.

*Performance* (release builds, base = the previous commit): `rook
run-headless --seconds 10` gave identical output and time (a modern core place 20.73
s against 20.75 s, a disaster place 0.36 against 0.37, a 2008 place 0.27 against 0.28).
`rook-player --bench 300`: a disaster place and a 2008 place unchanged (0 and 1 mesh files;
render 1.49 against 1.47 ms); a modern core place draws 86 mesh files and its frame
grew by about 2.8 ms (upload 3.6 to 4.9 ms, render 11.8 to 12.9 ms).

*Left for step 4 and later:* textures on meshes (`TextureId`, MeshPart
`TextureID`), `VertexColor` and per-vertex colours, materials and the
checker on empty MeshParts; unions' own geometry (step 6, below); MeshPart collision and
mass per `CollisionFidelity` (the loader can now hand physics a mesh);
CharacterMesh (with characters); mesh versions 6.00 and 7.00 if they turn
up; meshes that need an account.

## Trusses

**Trusses**, built 2026-10-13 (TrussPart is in 371 archive files, 31,487
uses; until now each drew as a solid grey box). How Rook builds and
batches them is in `docs/architecture.md` §8 "Rendering"; what's left is in
`docs/behavior/stubs.md`.

*Measured from Studio's OBJ export* of a 2 × 2 × 2 and a 2 × 4 × 2 truss
(the exports are kept outside the repository; Confirmed): bars are 0.25
studs square. Four rails run the truss's whole length at its corners, flush
with its 2 × 2 box. The truss is cut into 2-stud segments; each has a rung
across each of its four sides at both ends, between the rails, and where
two segments meet their rungs are one bar 0.5 studs thick. A brace is 0.25
studs wide and 0.125 deep, in the plane of the rails' centres (0.875 from
the axis), running from one corner of its segment 0.875 below its middle
to the opposite corner 0.875 above it, its ends cut square and buried in
the rails (its outline reaches 0.9634 studs from the segment's middle).
Faces hidden where bars meet are left out and normals are flat. The 2 × 2
× 2 export has no braces (48 quads: the 12 edges of its box); the 2 × 4 ×
2 has 96 quads, 32 of them braces. The UVs are planar at 1/8 of a unit per
stud into the surface-pattern strip (`Truss1_diff.png` is the studs strip),
which Studio doesn't draw on trusses; the install has no truss texture
(only the Insert Object icons), so Rook draws the bars in the part's
colour and material.

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
  side by side, end to end and at a right angle, two trusses in two colours
  each drew all their own bars).
- *Length:* floor(length / 2) segments, at least one; the drawing starts at
  the box's low end along the truss's axis and the rest of the box stays
  empty (2 × 5 × 2 draws 4 studs from its bottom). A truss shorter than 2
  draws one segment centred on the part.
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
  a 2016 core place, whose 596 trusses save 517 AlternatingSupports, 77
  NoSupports and 2 BridgeStyleSupports).

*Compared with Studio* (`rook-player --screenshot --fov 20` against
`screen_capture`, 1270 × 795, one test scene run by the same script in
both): each Style at 2 × 2 × 2 and 2 × 10 × 2, long trusses along X and Z
in two Styles, 2 × 5 × 2, 2 × 1 × 2, a red BridgeStyleSupports 2 × 7 × 2, a
Neon truss tilted 30 degrees and a truss with a Sphere mesh, from the
front, from above and at an angle: every bar's placement and every brace's
direction match by eye, apart from lighting and the Sphere's holes.
A 2016 core place draws its 596 trusses, its NoSupports ladders without
braces.

## Unions (step 6)

**Unions (step 6)**, built 2026-10-14. A union (`UnionOperation`,
`NegateOperation`, `IntersectOperation`) draws its own geometry, decoded
by `rook_content::mesh::union` into the same `Mesh` as mesh files and
loaded, uploaded and drawn through the same path (`Want::Mesh`, the GPU
mesh cache). What unions collide as (their saved hulls, since M15) is in
[collision](collision.md#unions).

*What the archive uses* (`rook corpus-scan`, which now counts unions by
form; 6,245 files loaded from a larger archive than step 3's, so the
counts aren't comparable with the tables above):

| Kind | Files | Uses |
|---|---:|---:|
| UnionOperation | 1,895 | 2,037,188 |
| ... with an `AssetId` (the geometry's asset) | 1,751 | 2,020,095 |
| ... without one | 407 | 17,093 |
| ... `MeshData` in the file: empty | 1,818 | 2,030,554 |
| ... `MeshData` in the file: `CSGK` (no geometry, only an id) | 158 | 6,616 |
| ... `MeshData` in the file: `CSGMDL2` geometry | 6 | 18 |
| ... geometry in the file and no `AssetId` | 1 | 1 |
| ... `PhysicsData` `CSGK` / `CSGPHS` 3 and 5 | 582 / 1 / 2 | 446,243 / 1 / 2 |
| ... with `ChildData` | 194 | 33,263 |
| ... `UsePartColor` true | 1,617 | 1,142,650 |
| ... resized (Size isn't `InitialSize`) | 1,634 | 996,075 |
| ... under another union | 150 | 4,904 |
| NegateOperation (with an `AssetId`) | 18 (17) | 108 (106) |
| IntersectOperation | 7 | 36 |
| Decals / Textures on unions | 669 / 590 | 44,031 / 109,428 |
| SpecialMesh / BlockMesh / CylinderMesh under a union | 52 / 21 / 5 | 249 / 202 / 18 |

Newer files also save `MeshData2` (a shared string) and
`PhysicalConfigData`; the decoder reads `MeshData2` too, but the scan that
counts it was stopped, so how often it holds geometry isn't known. Rook
draws only geometry behind an `AssetId`: in-file geometry without one is a
single union in the archive.

*Which assets load* (`rook content --place`, which now decodes union
assets; a sample of 3 named places and about 20 random ones, stopped
partway): every union asset that loaded anonymously holds `CSGMDL2`
geometry, from 2015-2016 places (a disaster place 4 of 4
distinct, a 2016 core place 49, a computer-core place 50). Union assets of newer
places are refused without an account (most of them
of the random sample), so their `MeshData` versions are unknown. No Open
Cloud key was available to count what a key unlocks. A union asset is a
model holding one `PartOperationAsset` with `MeshData`, `PhysicsData`
and `ChildData` (the parts it was made from).

*The geometry* (`rook content` on a disaster place's assets): its bounds equal the
union's `InitialSize`, centred on its origin. Each vertex carries a
colour (that of the part it came from), the `NormalId` of the box face it
was cut from, and UVs that are planar and in studs from the union's
centre.

*Checked in Studio 0.742* (2026-10-14, a test place of a disaster place's unions plus
variants, opened in a fresh Studio and laid out by the same script in
Studio and in `rook-player --script`, compared at FieldOfView 20 from the
same cameras). Observed unless marked:

- *Size:* the geometry is drawn at its vertices × Size / `InitialSize`
  about the union's position, the MeshPart rule (`mesh_part_scale`): a
  Turbine resized from (4, 5, 5) to (8, 5, 2.5), and a disaster place's rings saved at
  sizes other than their `InitialSize`, matched Studio's outlines.
  Scripts read `InitialSize` as `MeshSize`, and a new union reads
  (1, 1, 1) (conformance case `content/union`).
- *`UsePartColor`:* off, the union draws its vertices' colours whatever
  its Color (a union set to red still drew its parts' gold); on, its
  Color.
- *No geometry:* a union whose asset isn't a union (asset 1) and one with
  neither `AssetId` nor `MeshData` both draw nothing. While the asset
  loads Rook draws nothing, as for a new mesh file (Inference: Studio
  loads too fast to catch it). Assets that need an account draw nothing
  in Rook; Studio, signed in, draws them.
- *Decals* go on the geometry cut from their face's side of the box,
  stretched over the box face (a face decal on a cylinder union covered
  its front only, not its rims). *Textures* tile from each face's centre,
  the origin of the union's UVs: on a Bar resized to 12.4 × 4.5 × 4.5 the
  tiles sat a quarter stud from where corner tiling puts them, and moved
  there when Rook tiled from the centre. Rook tiles material textures the
  same way (Inference).
- *Surfaces:* a Plastic union with Studs on its Front drew none.
- *Materials* show on unions (a Wood union drew wood grain over its
  faces); Studio's looks lighter and finer than Rook's (Approximation, as
  for parts in step 5).
- *NegateOperation:* draws its geometry in a fixed orange-red, whatever
  its Color and `UsePartColor` (a gold one and a blue one alike; Neon made
  it white, so its material still applies). The colour, (252, 140, 90),
  is fitted against plain parts under the same light (Approximation). A
  new NegateOperation is named `NegativePart`, a new union `Union` and an
  IntersectOperation `Intersection` (conformance case).
- *Unions inside unions* each draw their own geometry where they are.
- *Mesh children* still replace the geometry with a 1-stud shape, as
  observed in step 3.

*Performance* (release builds, base = the build before this step, one `rook-player --bench
300` run each): whole-frame medians a disaster place 1.86 against 1.93 ms, a 2016 core place
19.0 against 18.7 ms, a computer-core place (182 union uses) 17.7
against 19.8 ms, where the server step, which unions don't touch, moved
by as much (8.6 to 9.6 ms): within run-to-run noise.

Left for later: unions whose assets need an account; in-file geometry
without an asset; `MeshData` versions 4 and 5 are decoded but no archive
asset Rook can fetch uses them, so they're unchecked against Studio; the
NegateOperation colour's exact value. `PhysicsData` hulls came with M15
([collision](collision.md#unions)).
