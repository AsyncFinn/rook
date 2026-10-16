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

*Left for later steps:* collision and queries
([step 3](#collision-and-queries-step-3)); the Terrain API
([step 4](#the-terrain-api-step-4)); the exact shift of the rough materials; which diagonal Roblox
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
- **Textures follow the place's set** (Confirmed, 2026-10-16): Studio
  draws a legacy place's Sandstone wall with the legacy set's cracked red
  rock and moss, not the 2022 set's pale stone. This replaces the claim
  that every place draws the 2022 table, which came from the two copies
  above drawing alike, likely because one Studio process kept the
  textures it loaded first (a long-running Studio does; measure material
  sets in a fresh process).
- **Legacy colour maps are coloured, not tint masks** (Inference): their
  packs say `alphamode` 1 and `FStringTerrainColorTable`'s
  `avgAlbedoPre2022` gives every material alpha 255 and a coloured
  average. Rook multiplies the texel by the material's colour over the
  legacy set's default (`baseColorsPre2022`), so default colours draw the
  texture as it is, as in Studio; other colours in legacy places aren't
  measured yet.
- **The colour map's alpha is a tint mask** (Confirmed): the terrain packs
  say `alphamode` 2 (TintMask). The colour tints a map where it's opaque
  and leaves it where it's clear: CrackedLava's alpha covers its cracks
  (37% of the map), so Studio draws coloured cracks on dark rock; Grass's
  side map has no alpha, so its sides show plain earth whatever the
  colour; Brick's mortar stays light. Where it's opaque, though, the
  colour doesn't multiply the map: see [Colours](#colours) below.
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
- **Colours**: a place that saved no `MaterialColors` gets its set's
  defaults (a new place saves the 2022 set's). How a colour tints the textures is
  in [Colours](#colours) below. A Ground slab coloured white drew 1.7 to
  2.6 times as bright as at its default colour, where a plain multiply
  says 3.8 to 7.2; the tint below gives 2.3.

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
water is mostly the sky it reflects; Approximation). Waves came later
([below](#grass-and-waves-step-2-continued)). Not yet: the shoreline
water inside solid voxels, and how water looks from below.

*Grass*: with `Terrain.Decoration` on, Studio grows animated grass blades
on Grass's tops (scripts can't change the property). Of the archive's ten
smooth-terrain places only one has it on. Drawn since
[below](#grass-and-waves-step-2-continued).

*Loading*: terrain textures load in the background like part materials
(one pack per material and facing the place uses), and the terrain is
redrawn as they arrive.

### Colours

Fixed 2026-10-16, after the grass measurements found red Grass magenta
in Studio ([below](#found-on-the-way)). Measured in Studio 0.742 on a
16 × 4 × 16 slab of each of the 21 materials, seen from above under flat
light (`EnvironmentDiffuseScale` and `EnvironmentSpecularScale` 0,
`Brightness` 0, `Ambient` and `OutdoorAmbient` grey), beside
SmoothPlastic parts in 9 greys whose pixels undo Studio's tone curve,
pixel by pixel, before averaging. Each slab was coloured white, red,
green, blue, (255, 204, 204), (204, 255, 204), (204, 204, 255), grey 128
and grey 64; its default colour and (255, 128, 128) were held back to
check the fit.

*Where Studio does it* (Studio's own shaders): the terrain shader
(`SmoothClusterFSUnified` in the install's `shaders_glsl3.pack`, plain
GLSL) takes the albedo already coloured from a texture array, stored as
YCoCg and squared (gamma 2). No shader in the packs applies
`MaterialColors`, so Studio colours the textures on the CPU when it
builds that array. `FStringTerrainColorTable` holds no tint constants.

*What it does* (Confirmed for the shape, constants fitted): in gamma
space, where the map's tint mask is opaque, Studio keeps only the texel's
luma and multiplies it by a tint worked out from the colour per material:

- **Brightness**: the colour's largest channel (HSV value), times a
  per-material gain, at most 1. White, red, green and blue all draw as
  bright as each other; grey 128 draws half as bright as white. The gain
  is 1 for 16 materials, Rock 1.16, Ground 1.26, Asphalt 1.04, and Mud
  2.2 and Basalt 3.43, which reach full brightness at their default
  colours (Basalt is as bright in its dark grey default as in white). At
  white, Basalt draws at about 0.6 and Mud at 0.85 of the other
  materials' share of their textures' luma.
- **Chroma**: the colour's YCoCg chroma over its largest channel, times a
  per-material 2 × 2 matrix (turning and scaling it), cut to a
  per-material length. Red turns magenta on Grass and LeafyGrass (about
  20 degrees), lavender on Slate (55), teal on Basalt (175). Concrete,
  Slate, Glacier and Salt take little of it (Salt almost none); Rock,
  Basalt, Asphalt, Pavement and Snow reach their length already at a
  pale colour, so pink and red look alike on them.
- Where the tint mask is clear, the map shows as it is.

Fitted on the 7 coloured shots, the brightness and chroma reproduce the
held-back colours within 6 of 255 on 20 materials. CrackedLava misses by
up to 48: its dark rock and lit cracks average unevenly (Approximation).
With the matrix the identity and no length, the tint would be the
colour itself: a grey texture times the colour, in gamma space.

Rook (`rook_terrain::TerrainMaterial::tint`, `src/material/tint.rs`, and
the terrain branch of `shaders/parts.wgsl`): the player sends the tint
as the surface's colour, and the shader draws mix(texel, luma × tint,
alpha), luma being (r + 2g + b) / 4 of the linear texel (close to
Studio's for terrain's near-grey maps). Rendered from above, red Grass,
Sand and Basalt turn magenta, pink and teal, and in their default
colours Basalt draws about as bright as Grass, as in Studio. Rook's lighting
still flattens the contrast between materials (Sand against Grass 0.6
where Studio has 0.46, M13).

## Grass and waves (step 2, continued)

**Grass and waves**, built 2026-10-15 and measured again before 0.5.2
was released: what step 2 left of terrain's look. Measured in Studio 0.742 on
the Baseplate place with a 64 × 4 × 64 Grass slab, a 64 × 4 × 12 Grass
strip and a 64 × 4 × 64 block of Water, each beside a ruler part,
screenshots taken with narrow fields of view (2 to 8 degrees) so the
ruler gives studs per pixel, and compared by their pixels (coverage,
autocorrelation, FFT matching). Rook's renders came from a copy of the
same place holding the same voxels, from the same cameras.

*What Studio's settings and install say*: nothing about the blades' size
or the waves' shape. `StudioAppSettings.json` has only switches
(`FFlagRenderGrassUseBarycentricPlantingPoints`, `FFlagGrassGPUDensityCull`,
`FIntGrassLengthCustomization_FRMDensityScaling_UpperBound` 6, the water
transparency flags), and Grass's average texture colour in
`FStringTerrainColorTable` (`avgAlbedo2022`: (196, 196, 189), tint mask
alpha 241). The install has 25 water normal maps
(`PlatformContent/pc/textures/water/normal_01.dds` to `normal_25.dds`,
256 × 256 DXT5 with X in alpha and Y in green). `Terrain.GrassLength`
isn't a member in Studio 0.742. DevForum leads, checked below: Roblox's
water plays 25 frames; waves go one way and stop at `WaterWaveSpeed` 0.

### Grass blades

Observed:

- On Grass's tops, with `Decoration` on (the Baseplate has it on). Off,
  Rook draws none, as the property's documentation says; Studio wasn't
  checked with it off, since scripts can't change it (Inference).
  LeafyGrass grows none in Rook (Inference: the property is for Grass).
- **Shape**: each blade is one flat triangle with straight sides, about
  half a stud wide at its foot, seen magnified at 230 px per stud.
- **Size and density**: side on, how much of the view blades cover at
  each height (0.97 of it up to half a stud, 0.68 at 1.1 to 1.5 studs,
  0.22 at 2 to 2.6, 0.06 at 2.6 to 3.1, nothing above 3.6), and from
  above (half the ground), fit about 2.3 blades per square stud, 0.72 to
  3.6 studs tall, leaning every way up to about 50 degrees. Still as dense
  100 studs away, sparser but drawn 200 studs away.
- **Colour**: one flat colour per blade whatever it faces and on slopes,
  the ground's average colour around it: by luminance 0.78 of it in the
  default colour, 0.97 with Grass red. Black Grass grows black blades,
  so there's no light added of their own.
- **Sway** (no wind): tips 3 studs up swing about 0.1 studs either way
  over 2.5 to 4 seconds; blades under a stud apart move together, 1.3
  studs apart opposite, as if a wave crosses the grass.
- **Wind**: `Workspace.GlobalWind` turns the blades about their feet
  towards where it blows, their tips dropping: about 12 degrees at 5
  studs per second, 45 at 20 and 50 at 60.

Rook (`rook_terrain::grass`, `shaders/terrain.wgsl`): blades at random
points on Grass's upward triangles, fixed by each triangle's position so
the same ground grows the same blades, as above; each one triangle seen
from both sides, lit with an upward normal, in `avgAlbedo2022` tinted as
the ground is ([Colours](#colours)) at 0.7 of it in linear light (which
came out 0.82 of Rook's own ground). Measured on the same views, Rook covers
within 0.04 of Studio at every height side on, and 0.43 of the ground
from above against 0.50. The player builds blades in 32-stud tiles
within 200 studs of the camera and drops a tile's mesh when the camera
leaves; in the shader they sway (a wave of 2.6 studs crossing in 3
seconds, 0.035 studs per stud of height), turn into the wind by 50 ×
tanh(speed / 15) degrees, thin out from 60 studs away to 40% at the
edge, and sink into the ground over the last quarter. They cast no
shadow (Inference). Approximation: the planting points and sway are
Rook's own, fitted to the measurements.

### Water

Observed:

- **Waves move the surface itself**: along the water at grazing angle
  with `WaterWaveSize` 1, the block's far edge rises and falls by about
  1.2 studs in humps longer than the 64-stud block.
- **The animation runs on a phase**, `WaterWaveSpeed` × seconds summed as
  the speed changes: at speed 0 the water stays exactly as it is (two
  shots 2 seconds apart correlate 1.0) and goes on from there when the
  speed comes back. Run for set times at set speeds, then frozen, shots
  correlate 0.81 after 0.27 units of phase, 0.63 after 0.53, 0.44 after
  1, 0.18 after 2, and come back to 0.47 after 21 units (0.215 after 22):
  **the frames come round every 20 units**, 1.25 frames per unit, 2
  seconds at the default speed 10. Shots a third of a frame apart differ
  about as much as the next third, so frames fade into each other
  rather than swap (Inference).
- **The ripples stay in place**: they drift less than 0.07 studs per unit
  of phase.
- **Their size**: magnified at 382 px per stud, the ripples are smooth
  shapes 1 to 3 studs across with sharp, crinkled bright ridges; seen
  wider (57 studs across) nothing repeats within 43 studs. Matching
  single frames to the shots by FFT at 5 to 80 studs per tile finds no
  match, so Studio doesn't lay one frame plainly (Unknown how).

Rook: two sines rise and fall `WaterWaveSize` studs (wavelengths 60 and
45 studs, periods 40 and 25 units of phase); the 25 frames (the
renderer's texture array, from the install) play at 1.25 per unit, each
fading into the next, laid at 44 studs per repeat without scrolling. The
surface is `WaterColor` lit by the rippled normal, mixed towards the sky
by `WaterReflectance`, half of it looking down and all of it at grazing
angles (Schlick's Fresnel), the sky's colour going from overhead to the
horizon by where the reflection points, with a narrow sun glint. Seen
from above as in Studio, the pattern's contrast matches (luminance
deviation 2.06 against 2.07) and its autocorrelation across the ripples
too (0.36, 0.11, 0.02, -0.06, -0.10, -0.11 at 4 to 24 px against 0.47,
0.14, -0.01, -0.09, -0.13, -0.14); along them Rook's falls off a little
faster. Approximation: the tiling scale, the waves' shape and the
ripples' strength are fitted; close up Rook's ripples have a fifth of
Studio's contrast, which reflects its skybox's detail (Rook reflects two
sky colours, M13).

### Scripts

Scripts changing a Terrain property or `Workspace.GlobalWind` redraw the
terrain: the waves, wind and grass are read again, and the water and
blade entries redrawn (`RenderChanges::terrain`); a changed `SmoothGrid`
rebuilds the scene.

### Found on the way

Studio tints terrain textures differently from the tint mask step 2
assumed (Confirmed): with Grass set red, the ground under the blades
averages (197, 73, 167), magenta, where texture × mix(1, colour, alpha)
gives a dark red; with white, the blades are a blue grey (112, 143, 163)
and with grey 128 a dark navy (38, 54, 67). Measured and fixed on
2026-10-16: see [Colours](#colours).

*Costs* (release build, `--bench 300` on the largest terrain place, 1280 × 720,
the last change of the step against the build before it, five runs each from the grass and two from the
saved camera): no difference beyond the noise (medians 4.5 to 9.9 ms
for both, other builds running alongside). There are 2.3 times as many
blades as first built but each is 2 triangles instead of 6, and water takes
a second texture sample.

## Collision and queries (step 3)

**Collision and queries (step 3)**, built 2026-10-16. Measured in Studio
0.742 on a lone full Grass voxel, a 64 × 4 × 64 Grass slab (voxels 40 to
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
surface above its centre faces up. `IsRegion3Empty` reads the voxels.
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

## The Terrain API (step 4)

**The Terrain API (step 4)**, built 2026-10-16. Measured in Studio 0.742
in an empty place, reading each call's result back with `ReadVoxels`
(bytes below are `SmoothGrid`'s, 255 full) and recorded as the
conformance case `terrain/terrain_api`. Archive use, by files calling
each method name (some names are shared with other classes, so these
are upper bounds): `WorldToCellPreferSolid` 339, `GetCell` 329,
`WorldToCell` 169, `CellCenterToWorld` 166, `SetCell` 147,
`AutowedgeCells` 137, `SetWaterCell` 104, `PasteRegion` 69, `FillBlock`
65, `CopyRegion` 51, `GetMaterialColor` 42, `ReadVoxels` 41, `SetCells`
40, `GetWaterCell` 39, `WriteVoxels` 17, `WorldToCellPreferEmpty` 13,
`FillBall` 8, `CountCells` 7, `SetMaterialColor` 6, `AutowedgeCell` 4,
`FillRegion` 3, `FillWedge` and `FillCylinder` 2, `ReplaceMaterial` and
`CellCornerToWorld` 1.

*Fills* (Confirmed): each voxel near the shape gets an occupancy from
where its centre lies, rounded to the nearest byte:

- **`FillBlock`**: the least, over the box's own three axes, of how much
  of the voxel's 4-stud span the box covers. A 1-stud cube in a voxel
  fills a quarter of it (64), not 1/64; a 4-stud cube straddling eight
  voxels fills each half (128); one shifted half a stud up fills 223 and
  32. A box turned 45° fills its edge neighbours 202 and its corner
  neighbours 22, and nothing outside its world bounding box.
- **`FillBall`**: `(r - d) / 4 + 1/2` of the distance `d` from the
  centre: radius 0.5 fills its voxel 159, radius 4 its face neighbours
  128 and edge neighbours 22.
- **`FillCylinder`** (along the CFrame's Y): `(r - d) / 4` of the
  distance from the axis, rounded down (127 where a ball would give
  128), wherever the voxel reaches into the height at all (a voxel only
  touching an end is left out; one reaching 2 studs past it is filled
  whole).
- **`FillWedge`** (a WedgePart's shape, high at +Z): across, as a block;
  along the slope, `(t - y) / 4 + 1/2` where `t` is the slope's height
  half a voxel towards its low end, so a voxel-sized wedge fills
  nothing; its bottom and back cut wherever the voxel reaches into
  them at all.
- Materials terrain doesn't have (Plastic, Wood, Neon, Glass,
  ForceField, ...) fill as Grass. A negative size or radius is
  "Extents cannot be empty".

*Merging* (Confirmed): a solid material takes a voxel when it fills at
least as much as what is there (Rock 128 over Grass 128 is Rock 128;
Rock 128 over Grass 255 leaves the Grass). Water fills air and water but
never takes a solid voxel's place (full water over half Grass leaves the
Grass at 128). Air takes its occupancy away (half a voxel of Air on full
Grass leaves 127).

*Regions* (Confirmed): `FillRegion`, `ReplaceMaterial`, `ReadVoxels` and
`WriteVoxels` take a Region3 whose corners, in voxels, are truncated
toward zero (1 to 8 is voxels 0 and 1; -1.9 to 2.1 is none) and a
resolution that must be 4 ("Resolution has to be 4"). `ReadVoxels`
returns `[x][y][z]` arrays with a `Size` field, occupancies `(byte +
1) / 256` and 0 for air; more than 4,194,304 voxels is "Region is too
large". `WriteVoxels` checks the arrays' shape ("Bad argument materials
to 'WriteVoxels' (2x1x1 array expected)") and each item's type, clamps
occupancies to 0..1 and leaves air for 0. `FillRegion` fills each voxel
whole, water over solids too. `ReplaceMaterial` keeps each voxel's
occupancy (half Grass becomes half Water); Air as the target clears, and
Air as the source fills whole. `Clear` empties the terrain.

*Cells* (Confirmed): modern Roblox still honours the legacy cell
methods on smooth voxels; a cell is a voxel.

- `WorldToCell` floors; `CellCenterToWorld` and `CellCornerToWorld`
  round fractional cells half away from zero. `WorldToCellPreferSolid`
  and `WorldToCellPreferEmpty` pick the side of a face a point sits on
  that is solid (or empty).
- `SetCell` and `SetCells` write the smooth material standing in for
  the legacy one (Granite is Slate; Iron, Aluminum, Gold, CinderBlock
  and Cement are Concrete; WoodLog is WoodPlanks, Gravel Asphalt,
  MossyStone Pavement, RedPlastic Sandstone, BluePlastic Ice) with an
  occupancy for the block shape (Solid 255, the vertical and horizontal
  wedges 127, CornerWedge 85, InverseCornerWedge 170); Empty clears.
- `GetCell` names the legacy material back (Grass for materials no
  legacy one maps to, such as Rock and Basalt), a block shape for how
  full the voxel is (Solid up to byte 42, CornerWedge to 84,
  VerticalWedge to 127, InverseCornerWedge to 212, then Solid) and
  always `NegZ`.
- `SetWaterCell` fills the cell with water over whatever was there;
  `GetWaterCell` says whether it's water, with `None` and `NegX`.
  `AutowedgeCell` returns false and `AutowedgeCells` does nothing.

*Copies and colours* (Confirmed): `CopyRegion` takes a Region3int16 in
cells, both corners included ("The region can not be empty or
inverted."), and returns an unparented TerrainRegion with its
`SizeInCells` and `IsSmooth` true; `PasteRegion` puts it at a corner,
its air replacing what is there only with `pasteEmptyCells`.
`GetMaterialColor` and `SetMaterialColor` read and write
`MaterialColors`; Water and Air are "Unsupported terrain material",
other materials "Invalid terrain material".

*When collision follows* (Confirmed): reads see an edit at once, but rays
find a fill only 2 frames later, and still hit a carved hole's old
surface for a frame.

*Rook*: the edits live in `rook_terrain::edit` and `rook_terrain::cells`;
the Lua side is `rook-lua`'s `api/methods/terrain.rs`. Scripts edit the
physics world's decoded copy of Workspace's Terrain
(`rook_physics::terrain::edit`), so reads see edits at once. At the end
of the frame the colliders of the chunks the frame's edits reach (2
voxels around them, as far as the mesher reads) come down, sleeping
bodies there wake, and the voxels are written to `SmoothGrid` once,
where the renderer and replication see them. So rays see edits from the
next frame. A changed `SmoothGrid` from anywhere else (a client
receiving the server's edit) is compared with the voxels already held,
and only the chunks that differ are rebuilt, in physics and in the
renderer. Workspace always has a Terrain now, as in Roblox.

*Costs* (release build, an 86k-part place, the archive's largest terrain: 2.5 MB of
`SmoothGrid`, 3,165 chunks): decoding takes 250 to 300 ms and encoding
90 ms, so a frame with an edit there costs about that on the server and
on each client (meshing a chunk again is about 2 ms). Small terrains
cost next to nothing.

*Approximations and gaps*:

- `CountCells` counts voxels that aren't air. Studio counts differently
  for small terrain (a lone voxel, or two side by side, count 2; a full
  chunk its 32,768).
- `WorldToCellPreferEmpty` on a point where every cell around is solid
  returns the floor cell; Studio returned another cell, two away.
- The fill rules were measured with axis-aligned wedges and cylinders
  (and turned blocks); turned wedges and cylinders follow the same rules
  in their own frame (Inference).
- Water over a solid voxel fills its water channel (shorelines), which
  Rook doesn't draw yet; Rook keeping a replaced water voxel's water as
  the solid's is Inference.
- The renderer rebuilds its scene list after an edit, and every edit
  frame decodes and encodes the whole `SmoothGrid` (see Costs).
- Not implemented: `ReadVoxelChannels` and `WriteVoxelChannels` (no
  archive use), the `*_beta` and Roblox-only methods, TerrainRegion's
  transforms.
