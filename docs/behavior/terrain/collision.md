# Terrain collision and queries

Checked against Studio 0.742 on 2026-10-16.

Measured on a lone full Grass voxel, a 64 × 4 × 64 Grass slab (voxels 40 to
44 studs up, so its surface is at 46), the same in Brick (cubified, so at
44) and a 64 × 16 × 64 block of Water (surface at 56), made with
`FillBlock` in an empty place, with scripts in Edit mode for queries and
in Play for physics.

*What terrain collides as* (Confirmed): the smooth surface rays hit, not
voxel boxes. Dropped and left 4 seconds:

| Part | On | Studio rests at | Surface | Rook |
|---|---|---:|---:|---:|
| Ball, 1 stud | Lone Grass voxel | 43.831 | 43.333 (its box: 44) | 43.833 |
| Block 2 × 1 × 2 | Grass slab | 46.495 | 46 | 46.50 |
| Block 2 × 1 × 2 | Brick slab | 44.495 | 44 | 44.50 |
| Block 2 × 1 × 2 | Water | 55.80 (floats) | 56 | sinks through |

`Touched` fires with Terrain as the other part, on solid terrain and on
water. A ball dropped on the slab's sloping edge rolls off.

*Queries* (Confirmed):

- **Rays** (`Raycast`, `FindPartOnRay` and its ignore and whitelist
  versions) hit the surface and report Terrain, the position and normal
  where they hit it, and the material of the surface hit (Grass, Brick,
  Water). They hit water's surface unless `RaycastParams.IgnoreWater` or
  the legacy `ignoreWater` argument is set, and then pass through it.
  The legacy `terrainCellsAreCubes` changes nothing: a ray into the side
  of the lone voxel hits its smooth side, 0.667 studs from the voxel's
  face, either way.
- **A ray starting inside terrain** hits nothing on its way out, up or
  down through the slab, or inside water. From below, a ray hits the
  slab's underside (at 38, normal down).
- **Filters** treat Terrain as a part: an exclude list with Terrain skips
  it, an include list without it skips it, and a ray in a collision
  group that doesn't collide with `Default` (Terrain's group) misses it.
- **`Spherecast` and `Blockcast`** hit it too: a sphere of radius 1 cast
  down from 60 stops 13 studs down, touching the slab at 46; water unless
  `IgnoreWater`.
- **`GetPartBoundsInBox`, `GetPartBoundsInRadius`, `FindPartsInRegion3`
  and `GetPartsInPart`** never return Terrain, even inside the slab.
- **`GetTouchingParts`** lists Terrain when the part overlaps the solid
  surface (its bottom 0.05 studs below it) or sits inside the terrain (a
  4-stud block whose top meets the slab's surface from below), even with
  `CanCollide` off and nothing connected to `Touched`. Not 0.5 studs
  above it, nor in water.
- **`IsRegion3Empty`** tests voxels, not the surface: a box overlapping
  any voxel that isn't air, water included, by even 0.01 studs isn't
  empty, though the voxel's surface may be over a stud away (a corner of
  the lone voxel); a box in the air voxel above the slab is empty though
  the slab's surface reaches 2 studs into it; touching a voxel's face
  doesn't count. Listing Terrain in `IsRegion3EmptyWithIgnoreList`
  doesn't change this.

*Rook* (`rook_physics::terrain`): each chunk's surface from
`rook_terrain::mesh_chunk`, one triangle mesh per material on a fixed
body, so parts rest on exactly the surface Rook draws, a hit's material
is its mesh's, and parts slide with that material's default friction and
elasticity (Inference: terrain has no `CustomPhysicalProperties`). Water's
mesh is a sensor: hit by rays and touched, but not solid. Rays skip the
back faces of terrain's triangles, which gives the inside rule above;
`GetTouchingParts` counts a part inside terrain when the nearest
surface above its center faces up. `IsRegion3Empty` reads the voxels.
The colliders are rebuilt when `SmoothGrid` changes or the Terrain comes
or goes.

*Building on demand* (Rook): a chunk's colliders are built the first time
a moving part's box (grown by how far it moves in the frame), a ray, a
shape cast or `GetTouchingParts` comes within 8 studs of the chunk (its
surface reaches up to 7 studs outside it). Building an 86k-part place's whole
surface (12.1 million triangles in 3,319 meshes) took 3.5 seconds on half
the cores, and the player has two physics worlds (server and client).
Built on demand, an 86k-part place's headless run is as long as before the step
(11.5 to 14.1 seconds against 12.3 to 14.5 for 6 simulated seconds,
two runs each), and a 2-stud part dropped onto its snow comes to rest a
stud above the surface the first ray found there.

*Approximations and gaps*:

- Parts don't float: Roblox's water is buoyant (the block above floated
  0.2 studs below the surface). Left for characters' swimming (M9),
  which needs the same water volume.
- The surface is a shell with no thickness, and Rook doesn't sweep
  moving parts, so a part that moves farther than its own size in one
  240 Hz step can pass through it (Inference that Roblox stops it; not
  measured).
- Bent quads split along Rook's diagonal (step 1), so collision there
  can differ from Studio's by a little, as rays do.
- Touches with water begin and end as a part crosses its surface, not
  while it is under it (Approximation).
