# Collision shapes

What parts collide and are queried as, beyond their primitive shapes
(ROADMAP M15 step 1). Unions first; MeshParts follow.

## Unions

**Built 2026-10-14** (`rook_content::mesh::union::physics`,
`rook_physics::unions`, `rook_engine::unions`). Before this a union
collided and was hit by rays, mouse clicks and ClickDetectors as its box,
so in both core places the door buttons couldn't be clicked: they
sit in a doorway cut through a wall union, and the mouse ray hit the
wall's box. Rook now uses the geometry Studio saved for the union.

*Where the geometry is.* A union's collision geometry is a `CSGPHS` blob,
like `MeshData` saved in the union or in its asset:

| Where | 2016 core place | Modern core place |
|---|---:|---:|
| `PhysicalConfigData` in the file (version 7 / 6) | 190 of 190 | 1,555 of 1,802 |
| `PhysicsData` in the file | 0 | 0 |
| `PhysicsData` in the union's asset | 0 (of the assets checked) | 0 (of the assets checked) |
| Nowhere: `PhysicalConfigData` saved empty | 0 | 247 |

Counted by decoding every union in both files. `PhysicsData` in the
archive is mostly `CSGK` (a pointer, no geometry; see
[meshes](meshes.md#unions-step-6)).

*The format* (read from the files; Inference where it says so):

- `CSGPHS` and a 32-bit version.
- Version 0 followed by `BLOCK`: the union collides as its box. This is
  also the property's default in the API dump, so a file that doesn't
  save `PhysicalConfigData` reads as a block in Rook; Rook doesn't take
  that as the union's shape (see below).
- Versions 3 and 5: convex hulls, one after another to the end of the
  blob. Version 6 first gives the union's volume, centre of mass and
  inertia (10 floats); version 7 a geometry type byte before them. Each
  hull is a 40-byte header, the count of position floats, a 4 (Inference:
  a stride), the positions, the count of indices and the indices.
  `rbx_mesh` 0.8 reads only the first hull of version 6, so Rook reads
  versions 3 to 7 itself (the 77 × 77-stud floor union of a modern core place is
  39 hulls, not one 9.6-stud one).
- Version 8: the hulls packed with zstd and edgebreaker, read by
  `rbx_mesh`. Neither core place uses it.
- The hulls are in the same space as `MeshData`: their bounds are the
  union's `InitialSize`, centred on its origin (every union decoded in
  both files). So they are scaled by `Size / InitialSize` like the drawn
  geometry (`mesh_part_scale`).

*The rules Rook follows:*

- A union in Workspace with `CollisionFidelity` Box is its box.
- Otherwise it is its saved hulls: `PhysicalConfigData` or `PhysicsData`
  in the union, else in its asset. A saved `BLOCK` doesn't count (it is
  the default; a box union has fidelity Box).
- With no hulls saved anywhere, it is its drawn geometry from the asset
  (the triangles of its `MeshData`) (Approximation: Studio presumably
  makes a convex decomposition from the geometry; the drawn geometry is
  what that approximates).
- Until the geometry loads, and when it can't, the union is its box. A
  place's saved hulls are read before its scripts start, so scripts'
  first queries already see them; asset geometry arrives through the
  content loader.
- Rays, shape casts, `GetPartsInPart`, `GetTouchingParts` and collisions
  all use the shape; Region3 and the `GetPartBounds*` queries keep
  testing the `Size` box, as for every part.
- A union that moves (in an unanchored assembly) with `CanCollide` off
  keeps its box (Approximation, for speed: it collides with nothing, but
  its hulls would still be tested against everything near it every step
  for Touched; a modern core place's 9 spinning PowerCore rings and its fans made
  a 5-second headless run 20% slower).
- Mass and inertia stay those of the box (Approximation: Studio saves the
  real volume, centre of mass and inertia with the hulls).
- The client builds the same shapes (the mouse and ClickDetectors query
  the client's world); `PhysicalConfigData` replicates.

Inference, to be measured in Studio: that Studio's rays and mouse hit the
collision geometry (not the drawn mesh); that `Hull` and
`PreciseConvexDecomposition` change only what Studio saves; that Studio
regenerates geometry for a union with none saved.

*Checked on a core place* (`rook run-headless` with a probe script: for every
ClickDetector part, rays from 5 studs out along each axis toward it):
before, 49 of 261 ClickDetectors in a modern core place and 36 of 214 in a 2016 core place were hidden from every direction, every door button among them, by
a union's box; after, 16 and 20 are, none by a union and none a door
button (the rest are behind invisible blocker parts the places put there
on purpose, `RCBlocker`, or other plain parts). The 2016 airlock's wall
union (1 × 12 × 12.2 studs, 15 saved hulls) now lets rays through its
round doorway, and its drawn geometry gives the same opening.

*Performance* (release builds, base = 0.10.1, `rook run-headless
--seconds 5`, two runs each): a 2016 core place (no moving unions) 17.1 / 17.2 s
against 17.4 / 17.7 s; a modern core place (86 moving unions, most of them not
colliding) 14.8 / 14.7 s against 15.6 / 15.8 s, the cost of its
colliding moving unions (stall doors, elevator railings) touching their
neighbours hull by hull.
