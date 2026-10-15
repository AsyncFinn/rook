# Collision shapes

What parts collide and are queried as, beyond their primitive shapes
(ROADMAP M15 step 1). Unions first; MeshParts follow.

## Unions

**Built 2026-10-14** (`rook_content::mesh::union::physics`,
`rook_physics::unions`, `rook_engine::unions`). Before this a union
collided and was hit by rays, mouse clicks and ClickDetectors as its box,
so in both large places the buttons couldn't be clicked: they
sit in a doorway cut through a wall union, and the mouse ray hit the
wall's box. Rook now uses the geometry Studio saved for the union.

*Where the geometry is.* A union's collision geometry is a `CSGPHS` blob
saved in the union (and, in a union asset's `PartOperationAsset`, beside
its `MeshData`):

| Where | large 2016 place | Modern large place |
|---|---:|---:|
| `PhysicalConfigData` in the file (version 7 / 6) | 190 of 190 | 1,555 of 1,802 |
| `PhysicsData` in the file | 0 | 0 |
| `PhysicsData` in the union's asset | 0 (of the assets checked) | 0 (of the assets checked) |
| None: `PhysicalConfigData` saved empty | 0 | 247 |

Counted by decoding every union in both files. `PhysicsData` in the
archive is mostly `CSGK` (a pointer, no geometry; see
[meshes](meshes.md#unions-step-6)).

*The format* (read from the files; Inference where it says so):

- `CSGPHS` and a 32-bit version.
- Version 0 followed by `BLOCK`: the union collides as its box. This is
  also the property's default in the API dump, so a file that doesn't
  save `PhysicalConfigData` reads as a block.
- Versions 3 and 5: convex hulls, one after another to the end of the
  blob. Version 6 first gives the union's volume, centre of mass and
  inertia (10 floats); version 7 a geometry type byte before them. Each
  hull is a 40-byte header, the count of position floats, a 4 (Inference:
  a stride), the positions, the count of indices and the indices.
  `rbx_mesh` 0.8 reads only the first hull of version 6, so Rook reads
  versions 3 to 7 itself (the 77 × 77-stud floor union of a large modern place is
  39 hulls, not one 9.6-stud one).
- Version 8: the hulls packed with zstd and edgebreaker, read by
  `rbx_mesh`. Neither large place uses it.
- The hulls are in the same space as `MeshData`: their bounds are the
  union's `InitialSize`, centred on its origin (every union decoded in
  both files). So they are scaled by `Size / InitialSize` like the drawn
  geometry (`mesh_part_scale`).

*Checked in Studio 0.742* (2026-10-14, in a test folder of an open
Studio, by script):

- *Rays follow the collision geometry, by `CollisionFidelity`.* A
  12 × 12 × 1 plate with a 6-stud round hole cut through it
  (`SubtractAsync`): with Default and PreciseConvexDecomposition, rays
  through the hole's centre and 0.1 stud inside its edge miss; with Hull
  and Box they hit the face, as they do through the plate's solid part
  under every fidelity. (The mouse and ClickDetectors use the same rays
  in Rook; Inference that Studio's do too.)
- *Files don't save `CollisionFidelity`*: neither large place has it, and
  Studio derives it from the saved geometry.
- *No hulls saved: a box.* The 2016 wall union without its
  `PhysicalConfigData` (as a file that doesn't save it) and a large modern place
  union with it saved empty both read `CollisionFidelity` Box in Studio,
  and every ray across them (8 × 8 per axis) hit their box, through the
  wall's doorway too. Studio doesn't rebuild hulls from the drawn
  geometry (three seconds after loading).

*The rules Rook follows:*

- A union in Workspace is the hulls saved in its `PhysicalConfigData`,
  or, when that is a block (its default), in its `PhysicsData` (older
  files; Inference, no file at hand saves hulls there).
- With no hulls, or a block, it is its box (observed). Rook doesn't fetch
  the union's asset for them: none of the assets checked saves any.
- The saved hulls are read before the place's scripts start, so their
  first queries already see them.
- Rays, shape casts, `GetPartsInPart`, `GetTouchingParts` and collisions
  all use the shape; Region3 and the `GetPartBounds*` queries keep
  testing the `Size` box, as for every part.
- A union that moves (in an unanchored assembly) with `CanCollide` off
  keeps its box (Approximation, for speed: it collides with nothing, but
  its hulls would still be tested against everything near it every step
  for Touched; a large modern place's 9 spinning spinning rings and its fans made
  a 5-second headless run 20% slower).
- Mass and inertia stay those of the box (Approximation: Studio saves the
  real volume, centre of mass and inertia with the hulls).
- The client builds the same shapes (the mouse and ClickDetectors query
  the client's world); `PhysicalConfigData` replicates.
- Scripts reading `CollisionFidelity` still get Default in Rook, where
  Studio derives Box for a union without hulls (left for later).

*Checked on a large place* (`rook run-headless` with a probe script: for every
ClickDetector part, rays from 5 studs out along each axis toward it):
before, 49 of 261 ClickDetectors in a large modern place and 36 of 214 in a large 2016 place were hidden from every direction by
a union's box; after, 16 and 20 are, none by a union and none a button (the rest are behind invisible blocker parts the places put there
on purpose, `RCBlocker`, or other plain parts). The 2016 wall union's
union (1 × 12 × 12.2 studs, 15 saved hulls) now lets rays through its
round doorway.

*Performance* (release builds, base = 0.10.1, `rook run-headless
--seconds 5`, two runs each): a large 2016 place (no moving unions) 17.1 / 17.2 s
against 17.4 / 17.7 s; a large modern place (86 moving unions, most of them not
colliding) 14.8 / 14.7 s against 15.6 / 15.8 s, the cost of its
colliding moving unions (stall doors, elevator railings) touching their
neighbours hull by hull.
