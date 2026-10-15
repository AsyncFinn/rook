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
and very few hold voxels. The smooth ones are a 25-million-voxel place (25.2 million
voxels, 99% of them Water), a 1.18-million-voxel place (1.18 million, Grass with
Concrete, Brick and WoodPlanks buildings), a 751,000-voxel place (751,000: Rock and
Snow), two water-only places (water only), a 75,000-voxel place (75,000:
Snow and Glacier) and four smaller ones. Over all ten, Water is 93% of voxels, then Grass, Rock and Snow. The
per-material totals were counted before Rook's decoder kept voxels with
occupancy byte 0 (below), so they are a few hundred voxels short; the file
counts are unaffected. The legacy files are mostly 2008 to 2012 places and
other places (`a place`, the an archive admin places).

*Legacy terrain in modern Studio* (Confirmed, Studio 0.742, three places
opened from a copy): Studio loads **no voxels** from legacy terrain. Two places saved with `ClusterGridV2` log "Legacy terrain data is malformed, voxels will not be
loaded: Error while decoding data: unexpected end at offset 407" (and
"offset 16"), and one with `ClusterGrid` (XML) loads with
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
is 1/256 full (a voxel, not air). Rook decodes a 75,000-voxel place exactly as
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
| Cobblestone | Quantize to 0.15 of the cell (0.6 studs; a slab's top 3.8 above its centres). Step 1 read this as cubifying a tenth of the way plus a random shift; Studio's table (step 2) shows it quantizes, which predicts both measurements | 1.40 | Confirmed (lone voxel and slab) |
| Concrete | Quantize to halves of the cell (2 studs) | 2.00 | Confirmed (random block exact) |
| Pavement | Quantize to quarters of the cell (1 stud) | 1.00 | Confirmed (random block exact) |
| Rock, Slate, CrackedLava, Basalt, Ice | A pseudo-random shift of each vertex, up to about 1 stud, fixed by position (Slate and CrackedLava shift alike, Rock, Basalt and Ice each their own way) | 0.4 to 1.6, varying | Approximation: since step 2, Rook's own pseudo-random shift of the table's size |
| Water | Half a voxel lower than a solid's surface (step 2) | | Confirmed |

Quantizing rounds halves away from the cell's centre: the edge of a
half-full Concrete slab comes in to its voxels' centres. Where a cell
mixes these materials, Rook moves its vertex by the dominant material, and
13% of the rays into a random block of Brick, WoodPlanks, Pavement,
Concrete and Grass miss (Approximation; neither the fullest single voxel,
corner counts, nor a blend of each material's move does better). The
shift's pseudo-random offsets depend on Roblox's own hash, so Rook can't
reproduce Rock and the other rough materials' facets exactly.

*Shading* (observed): Brick, WoodPlanks, Concrete and Pavement are drawn
faceted (hard edges) and Grass and Snow smooth ("materials also define
soft/hard edges"). Since step 2 Studio's table decides: its `hard` and
`hardsoft` materials (Concrete, Brick, WoodPlanks, Rock, Basalt,
Cobblestone, Pavement) get flat normals per triangle, the rest smooth
normals averaged over the faces around each vertex across chunk and
material seams.

*Drawing it* (Rook): `rook-player` meshes Workspace's Terrain when the
place loads (and again when its `SmoothGrid` changes), a chunk per thread,
and draws one mesh per chunk and material through the mesh-file path, in
the material's `Terrain.MaterialColors` colour (the default table when the
place saved none; textured since step 2). Water is drawn in `WaterColor` at `WaterTransparency`
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
textures and the water surface are step 2's. A 75,000-voxel place, from the
same camera in both, shows the same snowy landmass.

*Costs* (release build, meshing only): a 75,000-voxel place 20 ms (56,000
triangles), a 1.18-million-voxel place 63 ms (426,000), a 751,000-voxel place 92 ms (553,000),
a 25-million-voxel place 317 ms (1.73 million, mostly the flat water). `rook terrain
<file>` prints what a file's Terrain holds and how long it takes to mesh.

*Left for later steps:* collision and queries (step 3); the Terrain API
(step 4); the exact shift of the rough materials; which diagonal Roblox
picks for bent quads.

## Materials and water (step 2)

**Materials and water (step 2)**, built 2026-10-15. Terrain is drawn with
its materials' textures, tinted by `Terrain.MaterialColors`; MaterialService
overrides put MaterialVariants on it; and water sits where Studio's does.
Checked in Studio 0.742 on two copies of the step 1 scene with blocks of
the other 18 materials added, one saved with `Use2022Materials` on and one
with it off, and on the 1.18-million-voxel archive place.

*Studio's terrain material table* (Studio's own data; how each field is
used comes from the measurements below): Studio's settings
(`%LOCALAPPDATA%\Roblox\ClientSettings\StudioAppSettings.json`) hold
`FStringTerrainMaterialTablePre2022` and `FStringTerrainMaterialTable2022`:
for each terrain material its `shift`, `cubify` and `quantize` amounts, its
`type` (`hard`, `hardsoft`, else soft), `mapping` (`cube` for Brick and
WoodPlanks), and one TexturePack for every face or separate
`texture_top`, `texture_side` and `texture_bottom` ones, each with
`tiling`, `detiling`, `rotation` and `blend` settings; Water has
`"water": 0.5`. `FStringTerrainColorTable` holds each set's default
colours. Rook keeps these tables in `rook-terrain`
(`src/material/looks.rs`).

- **Shapes follow the Pre2022 table in every place** (Confirmed): every
  shape step 1 measured matches it (Concrete quantize 0.5, Pavement 0.25,
  Cobblestone 0.1499, Brick and WoodPlanks cubify 1, Slate and CrackedLava
  both shift 0.1), measured in a place saved with `Use2022Materials` on,
  where the 2022 table would step Concrete by whole cells and shift
  neither Slate nor Basalt. Studio drew the copy saved with it off alike.
- **Textures follow the 2022 table in every place** (Confirmed): Studio drew
  both copies alike, CrackedLava with cracks in the place's magenta
  colour, which only the 2022 set's dark texture with its crack mask gives
  (the legacy set's texture is orange).
- **The colour map's alpha is a tint mask** (Confirmed): the terrain packs
  say `alphamode` 2 (TintMask). The colour tints a map where it's opaque
  and leaves it where it's clear: CrackedLava's alpha covers its cracks
  (37% of the map), so Studio draws coloured cracks on dark rock; Grass's
  side map has no alpha, so its sides show plain earth whatever the
  colour; Brick's mortar stays light. Rook draws texture × mix(white,
  colour, alpha) in linear light, as for SurfaceAppearance's TintMask.
- **Top, side and bottom textures**: a triangle takes its material's top
  texture when it faces more than 22.5 degrees up (closer to one of the
  five upward projection planes than to a vertical one), the bottom one
  when that far down, the side one otherwise (observed: a Grass ball shows
  grass down to about that far above its middle, earth below; Inference
  for the exact angle). Materials with one texture use it everywhere.
- **Projection**: each point takes its texture from the plane its surface
  faces most, of 18 (the 6 axes' and the 12 at 45 degrees between two,
  zeux 2020), or of the axes' 6 for Brick and WoodPlanks, in world studs,
  so textures run on across chunks and materials. Studio picks a plane per
  vertex and blends three samples per triangle; Rook picks per pixel from
  the smooth normal, so the plane changes along curves without blending
  (a Sand ball in Studio shows similar seams; Approximation).
- **Scale**: a texture covers `4 / tiling` studs, 8 for the 2022 table's
  0.5 (Inference, consistent with the comparisons).
- **Not drawn yet** (Approximation): the random shift and turn that break
  up repeats (`detiling`, `rotation`), the height-based blending between
  materials and between top and side (`blend`; the slides' low-quality
  mode has sharp seams too), and the packs' height maps.
- **Colours**: a place that saved no `MaterialColors` gets the 2022 set's
  defaults (a new place saves those). Studio's colours still differ in
  places: Brick's bricks are paler and Pavement's sides greener in Studio,
  Mud and Snow brighter. Part of it is lighting (Studio's whole scene has
  a blue cast, M13), part unknown (Approximation). A Ground slab coloured
  white drew 1.7 to 2.6 times as bright as at its default colour, where a
  plain multiply says 3.8 to 7.2.

*MaterialVariants* (Inference, by parts' rules): a MaterialVariant that
MaterialService's override names for a terrain material (`GrassName` and
so on), of that base material, replaces the material's textures on
terrain, tiled at its `StudsPerTile` and tinted by the material's colour.
Voxels can't name a variant, so only overrides apply.

*Water* (Confirmed for its shape): surface nets moved down half a voxel,
the table's `"water": 0.5`. A lone full water voxel spans 3.333 to 0.667
studs below its centre, a half-full one 2.667 to 1.333 below; a full
one-voxel slab spans 6 below to 2 above, its edge sloping as a solid's
does, 2 studs lower. Rook draws it in `WaterColor` at `WaterTransparency`,
mixed halfway towards the sky's colour by `WaterReflectance` (Studio's
water is mostly the sky it reflects; Approximation). Not yet: waves
(`WaterWaveSize`, `WaterWaveSpeed`), the shoreline water inside solid
voxels, and how water looks from below.

*Grass* (not drawn): with `Terrain.Decoration` on, Studio grows animated
grass blades on Grass's tops (scripts can't change the property). Of the
archive's ten smooth-terrain places only one has it on.

*Loading*: terrain textures load in the background like part materials
(one pack per material and facing the place uses), and the terrain is
redrawn as they arrive.
