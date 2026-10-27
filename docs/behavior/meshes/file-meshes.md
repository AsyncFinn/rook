# File meshes

Checked against Studio 0.744 on 2026-10-27.

SpecialMesh `MeshId`, FileMesh and MeshPart. How loading and drawing work
is in [architecture](../../design/architecture.md#rendering-rook-render); what's left is in
`docs/guide/stubs.md`.

From version 3.00 on, a file holds every level of detail in one face list,
and its LOD table gives where each starts (the community format
specification, Maximum_ADHD's "Roblox Mesh Format" on the DevForum);
Rook keeps them all ([levels of detail](#levels-of-detail)). Checked
against the user's `head.mesh` (version 2.00, 517 vertices, 846
triangles) when an install is found.

*What the archive uses* (`rook-tools corpus-scan`, 3,058 files that load; the
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
without an account, so their versions are unknown. A 2017 place: 40 mesh
uses, 39 decode (1.00 and 2.00), 1 refused. A large modern place: 827 uses, 788
decode (1.00, 1.01, 2.00), 39 refused, including all 7 of its MeshParts'
meshes.

*Checked in Studio 0.742* (2026-10-13; observed unless marked):

- *Where a file mesh sits:* the mesh's own origin is at the part's
  position, not its bounds' center. Version 1 of asset 12221462 is
  centered 11.6 studs off its origin and drew 11.6 studs off the part, as a
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
549): a test scene (the off-center mesh front and top, the sword, the head
with Offset, a MeshPart at Size ≠ `MeshSize` and one without
`InitialSize`): every outline within 2 px, 4 px on the sword's tip at 3.9
px per stud. A 2017 place, laid out by the same script
in both: 8 scaled meshes (top within 1 px), 8 rotated
meshes (top within 2 px) and an offset mesh
(within 1 px); in a large modern place, 54 rotated
meshes from front and top. Where masks differ by more (up to 12 px) it
is on sub-pixel fins and wires, which Studio's antialiasing fades
below the mask's threshold. A large modern place's MeshParts couldn't be compared:
their meshes need an account.

*Performance* (release builds, base = the previous commit): `rook
run-headless --seconds 10` gave identical output and time (a large modern place 20.73
s against 20.75 s, a small place 0.36 against 0.37, a 2008 place 0.27 against 0.28).
`rook-player --bench 300`: a small place and a 2008 place unchanged (0 and 1 mesh files;
render 1.49 against 1.47 ms); a large modern place draws 86 mesh files and its frame
grew by about 2.8 ms (upload 3.6 to 4.9 ms, render 11.8 to 12.9 ms).

## Levels of detail

Measured in Studio 0.744 on 2026-10-27 with `Stats.SceneTriangleCount`
(the drawn triangles, read with the mesh shown and hidden), edit mode,
default graphics quality (`EditQualityLevel` Automatic). Meshes used: a
200-stud sphere (version 4.01, levels of 9,800, 4,900, 2,418, 914 and 478
triangles, 2 high quality), a horse statue (4.01, 9,812 down to 490, 5
high quality), a small statue (4.01, 1,970 down to 75, 1 high quality)
and a 2016 MeshPart whose file Rook gets as version 2.00.

- *Only MeshParts switch.* A SpecialMesh FileMesh drew its full level at
  every distance, whatever its `Scale` or part size (the sphere's mesh to
  30,000 studs, and the 2016 mesh); so did a MeshPart made from an
  EditableMesh, which has one level.
- *Distance, not screen size:* a level begins where the camera is a set
  distance from the part's box along the world axes (the box of `Size`
  turned with the part, then boxed again: a part turned 45 degrees and seen
  along a diagonal switched within 0.1 stud of that box, 80 studs off the
  turned box's distance). The part's size matters only through where its
  box is: the sphere at Size 200 switched 350 studs from its center, at
  Size 100 at 300, and stretched to 200 x 20 x 20 at 260 seen along Z and
  350 along X.
- *The view's width scales it:* with `FieldOfView` 20 the first switch
  moved out to 992.76 studs from the box, exactly 250 x tan 35 / tan 10. The
  distances are for a 70 degree view.
- *Thresholds* (studs from the box at 70 degrees, each to 0.01): level 1
  from 250, level 2 from 500, level 3 from 2,000, level 4 from 3,000.
- *`RenderFidelity`:* Precise draws the full level always; Performance
  walks down every level; Automatic only the levels the file counts as high
  quality (the sphere stopped at level 1 out to 30,000 studs, the horse
  went down all five, the small statue never left level 0). A version 3
  file doesn't say; Rook lets Automatic use all its levels (Inference).
- *No hysteresis:* walking out and back in half a stud at a time, the
  sphere switched at 350.0 going out and back at 349.5 coming in.
- *Old meshes get levels from Roblox:* the 2016 MeshPart's mesh, which
  asset delivery gives Rook as version 2.00 with one level, drew at five
  (2,657, 1,328, 645, 246, 125) in Studio, as a MeshPart only. Studio asks
  asset delivery's batch API for a content representation
  (`contentRepresentationSpecifier` in its logs, `fidelity` and
  `skipGenerationIfNotExist` in the executable); Rook's requests for a
  `mesh` representation got the source file or "not found". Rook draws
  such meshes at their one level for now.
- *Not levels, but nearby:* at `EditQualityLevel` 1 to 10 the sphere
  wasn't drawn at all from 1,000 studs, and the 2.5-stud statue vanished
  past about 600 studs at the default quality: view distance and small
  part culling, which Rook doesn't do yet ([M21](../../roadmap/m21-performance-and-scale.md)).

*How Rook does it:* every level goes into the mesh pool, and each frame
the instances in runs a pass sees pick their level by the rule above; see
[architecture](../../design/architecture.md#rendering-rook-render). Where
Rook differs: a coarser level holds until the camera comes 5% nearer than
its threshold, so a camera hovering there doesn't flip it each frame, and
image layers draw at their part's level.

*Performance* (dev builds, base = the previous commit, two alternating
`rook-player --bench 200` runs each): a test place of 1,600 horse statues
(Automatic) on a 40-stud grid, seen from 100 studs off one side, drew
4,434,364 triangles a frame instead of 14,917,680, in 66 draws instead
of 17; render and readback took 2.5 to 2.8 ms instead of 4.5 to 4.6, and
building the draws 0.10 to 0.12 ms instead of 0.06. Every horse nearer
than 250 studs drew the same pixels; only rows past it differ. The
103k-instance place in [M21](../../roadmap/m21-performance-and-scale.md)
drew the same 5,445 draws and 10.67M triangles (its meshes have one
level), with CPU recording 6.6 to 7.2 ms against 6.7 to 7.0.

*Not done yet:* the levels Roblox makes for old MeshPart meshes (above);
MeshPart collision and mass per `CollisionFidelity`
([M15](../../roadmap/m15-physics-completeness.md), which the loader can
already hand a mesh);
CharacterMesh (with characters); mesh versions 6.00 and 7.00 if they turn
up; meshes that need an account.
