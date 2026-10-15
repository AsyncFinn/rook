# Terrain

What Rook found while building M6. The plan itself is in [ROADMAP.md](../../ROADMAP.md#m6-terrain).

## Voxels and their surface (step 1)

**Voxels and their surface (step 1)**, built 2026-10-15. `rook-terrain`
decodes `Terrain.SmoothGrid` into voxels (`Grid`) and meshes them as
Roblox does (`mesh_chunk`); the player draws the surface in a flat colour
per material. Everything was measured in Studio 0.742 with `WriteVoxels`,
`FillBlock`/`FillBall` and `Workspace:Raycast`, whose hits give the
collision surface's exact positions and normals. The recordings are in
`crates/rook-terrain/data/` and replayed by its tests.

*What the archive has* (`rook terrain --scan`, run once over the archive's
10,939 files, 6,378 unique, 231 s):

| Terrain | Files |
|---|---:|
| No Terrain instance | 5,682 |
| A Terrain with no voxels or legacy data | 582 |
| Smooth voxels (`SmoothGrid`) | 10 |
| Legacy blocks (`ClusterGrid`, `ClusterGridV2` or `ClusterGridV3`) | 104 |
| Both | 0 |

So 696 unique files have a Terrain (the ROADMAP's 977 came from an older count),
and very few hold voxels. The smooth ones are Ragdoll Engine (25.2 million
voxels, 99% of them Water), an FPS place (1.18 million, Grass with
Concrete, Brick and WoodPlanks buildings), a rocky snow place (751,000: Rock and
Snow), two versions of a chase place (water only), a lab place (75,000:
Snow and Glacier), a keycard test place, a zombie place, a waterpark and a dance
club. Over all ten, Water is 93% of voxels, then Grass, Rock and Snow. The
per-material totals were counted before Rook's decoder kept voxels with
occupancy byte 0 (below), so they are a few hundred voxels short; the file
counts are unaffected. The legacy files are mostly 2008 to 2012 places and
Roblox's own test places (`ROBLOX-Pre-Jekyll`, the RCHIVE admin places).

*Legacy terrain in modern Studio* (Confirmed, Studio 0.742, three places
opened from a copy): Studio loads **no voxels** from legacy terrain. Happy
a house place (March 2012) and a fortress place (both
`ClusterGridV2`) log "Legacy terrain data is malformed, voxels will not be
loaded: Error while decoding data: unexpected end at offset 407" (and
"offset 16"), and a terrain demo (`ClusterGrid`, XML) loads with
no message. In all three `Terrain:CountCells()` is 0 and `ReadVoxels` finds
no voxel between -2048 and 2048 studs. Rook does the same: it reads only
`SmoothGrid`. (The legacy XML upgrade still restores `ClusterGrid`'s
control characters, so the file round-trips.)

*`SmoothGrid`* (Confirmed): the layout in checkraisefold's notes for
rbx-dom (pull request 444, not merged) holds, with one correction to the
occupancy. A version byte (1) and the chunk size's log2 (5), then chunks
of 32³ voxels, each after its offset from the previous chunk in chunks
(three big-endian `i32`s, bytes interleaved), its voxels Y slowest and X
fastest in runs: a flag byte (material index, and bits for an occupancy
byte and a count byte), the occupancy, and the count (0 instead says one
voxel whose shoreline water occupancy follows). A missing occupancy is full.
Occupancy byte `b` is `(b + 1) / 256` full: a place built from known bytes
reads back in `ReadVoxels` as exactly that, so byte 255 is full and byte 0
is 1/256 full (a voxel, not air). Rook decodes a lab place exactly as
Studio reads it: Snow 60,059, Glacier 15,194 and Grass 98 voxels, with
the same summed occupancy (55,756.99, 14,122.13, 85.02). Voxel `v` covers
studs `4v` to `4v + 4`, and chunk `c` starts at `128c`. A damaged grid
(truncated, an unknown material, a run past its chunk) loads no terrain
and prints a warning.

*The surface: surface nets* (Confirmed; zeux, "Roblox: Large-scale
terrain", SIGGRAPH 2020, names the method):

- The corners of the dual grid are voxel centres. Every cell (the cube
  between eight voxel centres) where solid meets air has one vertex, the
  mean of the points where its edges cross the surface. Each voxel edge
  that crosses gets a quad joining the vertices of the four cells around
  it.
- An edge from a solid voxel of occupancy `o` to air crosses `4o` studs
  from the solid voxel's centre. So a lone full voxel is a cube reaching
  1.333 studs from its centre, a half-full one 0.667, and a full one-voxel
  slab is 8 studs thick (its top 4 above its voxels' centres, at the
  centre of the air above). The edge of a full slab slopes down 1 in 2,
  of a half-full one 1 in 3.
- Edges between two solid voxels never cross, whatever their occupancies
  or materials.
- Each quad is made of its solid voxel's material, which raycasts report.
- Measured: a layer of random occupancies over a full layer (2,209 rays
  from above, worst error 0.006 studs, the occupancy rounding), and a 4³
  block of random voxels in Grass, Snow and Sand (2,300 rays from all six
  sides). Two-voxel cases (faces, edges and corners touching, mixed
  occupancies and materials) match to 0.003 studs.

*Which diagonal splits a quad* (partly known): every quad of the
heightfield above splits the same way (Rook's `DIAGONALS`), but in the
random block about a third of the quads that aren't flat split the other
way. Shortest diagonal, best angles, flattest fold, outward bulge, closest
to the edge's own crossing, and the cells' materials all fail to predict
it. Rook splits every quad the heightfield's way (Approximation). In the
random block every ray but six of 2,300 hits Rook's surface within 0.01
studs with one split or the other; only bent quads differ, and smooth
terrain's quads are nearly flat.

*Materials that move their vertices* (zeux 2020: "shift", "cubify",
"quantize"), each vertex by its cell's dominant material, the one with the
most occupancy among its corners. Measured with a lone voxel, a slab, and
the same random block in one material:

| Material | What it does | Lone full voxel reaches | Confidence |
|---|---|---:|---|
| Grass, LeafyGrass, Sand, Snow, Mud, Ground, Asphalt, Salt, Glacier, Limestone, Sandstone | Nothing: plain surface nets | 1.333 | Confirmed |
| Brick, WoodPlanks | Cubify all the way: each vertex at its cell's centre, so voxels are 4-stud boxes at any occupancy | 2.00 | Confirmed (random block exact) |
| Cobblestone | Cubify a tenth of the way (a slab's top 3.8 above its centres), plus a small pseudo-random shift (0.12 studs typical) | 1.40 | Inference; the shift isn't reproduced |
| Concrete | Quantize to halves of the cell (2 studs) | 2.00 | Confirmed (random block exact) |
| Pavement | Quantize to quarters of the cell (1 stud) | 1.00 | Confirmed (random block exact) |
| Rock, Slate, CrackedLava, Basalt, Ice | A pseudo-random shift of each vertex, up to about 1 stud, fixed by position (Slate and CrackedLava shift alike, Rock, Basalt and Ice each their own way) | 0.4 to 1.6, varying | Approximation: drawn as plain surface nets |
| Water | Its own rules ("special math for water"); a full lone water voxel is hit 0.67 below its centre | | Approximation: meshed like a solid (step 2) |

Quantizing rounds halves away from the cell's centre: the edge of a
half-full Concrete slab comes in to its voxels' centres. Where a cell
mixes these materials, Rook moves its vertex by the dominant material, and
13% of the rays into a random block of Brick, WoodPlanks, Pavement,
Concrete and Grass miss (Approximation; neither the fullest single voxel,
corner counts, nor a blend of each material's move does better). The
shift's pseudo-random offsets depend on Roblox's own hash, so Rook can't
reproduce Rock and the other rough materials' facets yet.

*Shading* (observed): Brick, WoodPlanks, Concrete and Pavement are drawn
faceted (hard edges) and Grass and Snow smooth ("materials also define
soft/hard edges"). Rook gives those four flat normals per triangle and
every other material smooth normals, averaged over the faces around each
vertex across chunk and material seams (Inference for the materials not
seen).

*Drawing it* (Rook): `rook-player` meshes Workspace's Terrain when the
place loads (and again when its `SmoothGrid` changes), a chunk per thread,
and draws one mesh per chunk and material through the mesh-file path, in
the material's `Terrain.MaterialColors` colour (the default table when the
place saved none). Water is drawn in `WaterColor` at `WaterTransparency`
and casts no shadow. In Play mode the client draws its own copy: Rook
replicates `SmoothGrid`, which the API dump marks NotReplicated because
Roblox streams voxels to clients its own way. Every chunk is meshed at full detail: Roblox's level
of detail and chunk skirts aren't needed at the archive's sizes.

*Screenshot comparison* (2026-10-15): a FillBall of Grass, FillBlocks of
Brick and Concrete, a FillBall of Pavement, a random Snow heightfield and
a block of Water, built in Studio, read back and written into a copy of
Studio's Baseplate template. Rook's render from the same camera has the same
outlines: a smooth Grass ball, a sharp Brick box, the faceted Concrete
block and Pavement ball, and the Snow heightfield's shape. Colours,
textures and the water surface are step 2's. A lab place, from the
same camera in both, shows the same snowy landmass.

*Costs* (release build, meshing only): a lab place 20 ms (56,000
triangles), an FPS place 63 ms (426,000), a rocky snow place 92 ms (553,000),
Ragdoll Engine 317 ms (1.73 million, mostly the flat water). `rook terrain
<file>` prints what a file's Terrain holds and how long it takes to mesh.

*Left for later steps:* materials, MaterialVariants, grass and water's
look and surface (step 2); collision and queries (step 3); the Terrain API
(step 4); the shift of the rough materials; which diagonal Roblox picks
for bent quads.
