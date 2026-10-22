# Terrain materials

Checked against Studio 0.743 on 2026-10-16.

Terrain is drawn with
its materials' textures, tinted by `Terrain.MaterialColors`; MaterialService
overrides put MaterialVariants on it; and water sits where Studio's does.
Measured on two copies of the [surface](surface.md) scene with blocks of
the other 18 materials added, one saved with `Use2022Materials` on and one
with it off, and on a 1.18-million-voxel archive place.

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
colors. Rook keeps these tables in `rook-terrain`
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
- **Legacy color maps are colored, not tint masks** (Inference): their
  packs say `alphamode` 1 and `FStringTerrainColorTable`'s
  `avgAlbedoPre2022` gives every material alpha 255 and a colored
  average. Rook multiplies the texel by the material's color over the
  legacy set's default (`baseColorsPre2022`), so default colors draw the
  texture as it is, as in Studio; other colors in legacy places aren't
  measured yet.
- **The color map's alpha is a tint mask** (Confirmed): the terrain packs
  say `alphamode` 2 (TintMask). The color tints a map where it's opaque
  and leaves it where it's clear: CrackedLava's alpha covers its cracks
  (37% of the map), so Studio draws colored cracks on dark rock; Grass's
  side map has no alpha, so its sides show plain earth whatever the
  color; Brick's mortar stays light. Where it's opaque, though, the
  color doesn't multiply the map: see [Colors](#colors) below.
- **Top, side and bottom textures**: a vertex takes its material's top
  texture when its normal faces more than 22.5 degrees up (closer to one
  of the five upward projection planes than to a vertical one), the
  bottom one when that far down, the side one otherwise (observed: a
  Grass ball shows grass down to about that far above its middle, earth
  below; Inference for the exact angle), and the triangles between blend
  them ([Blending](blending.md)). Materials with one texture use it
  everywhere.
- **Projection**: each point takes its texture from the plane its surface
  faces most, of 18 (the 6 axes' and the 12 at 45 degrees between two,
  zeux 2020), or of the axes' 6 for Brick and WoodPlanks, in world studs,
  so textures run on across chunks and materials. Studio picks a plane per
  vertex and blends three samples per triangle; Rook picks per pixel from
  the smooth normal, so the plane changes along curves without blending
  (a Sand ball in Studio shows similar seams; Approximation).
- **Scale**: a texture covers `4 / tiling` studs, 8 for the 2022 table's
  0.5 (Inference, consistent with the comparisons).
- **Breaking up repeats** (`terrain_taps` in `common.wgsl`): Studio's
  terrain vertex shader (`SmoothClusterVSUnified`) gives each vertex a
  random number *r* from 0 to 255 and, per corner layer, turns its
  coordinates by asin((*r* / 128 − 1) × `rotation`) and shifts them by
  (*r*, ⌊2.6651442 *r*⌋) × *k* repeats; across a triangle the corners'
  copies blend by their weights (read from the shader; Inference until
  checked closer). *k* is 1 / `detiling` (Inference: Studio's
  `materials.json` keeps an older `detiling_legacy` next to `detiling`,
  equal to 1 / `detiling` in every legacy row, and the shader has one
  such factor). So the 2022 Brick, WoodPlanks, Cobblestone and Pavement
  tops (`detiling` 1) move by whole repeats, which shows nothing, and turn
  not at all. Rook's mesh vertices don't carry *r*, so Rook takes the
  corners of a 4-stud grid over the texture's plane, split along a
  diagonal, with *r* from a hash (Approximation). A 200-stud grass block
  seen straight down shows no repeat in either (no autocorrelation peak
  over 46 studs, Studio 0.743 and Rook), where Rook used to repeat every
  16 studs. The same random number moves the layer's height map
  ([Blending](blending.md)).
- **Colors**: a place that saved no `MaterialColors` gets its set's
  defaults (a new place saves the 2022 set's). How a color tints the textures is
  in [Colors](#colors) below. A Ground slab colored white drew 1.7 to
  2.6 times as bright as at its default color, where a plain multiply
  says 3.8 to 7.2; the tint below gives 2.3.

*MaterialVariants* (Inference, by parts' rules): a MaterialVariant that
MaterialService's override names for a terrain material (`GrassName` and
so on), of that base material, replaces the material's textures on
terrain, tiled at its `StudsPerTile` and tinted by the material's color.
Voxels can't name a variant, so only overrides apply.

*Loading*: terrain textures load in the background like part materials
(one pack per material and facing the place uses), and the terrain is
redrawn as they arrive.

## Colors

Found while measuring [grass](grass.md), and fixed on 2026-10-16: with
Grass set red, the ground under the blades averaged (197, 73, 167),
magenta, where texture × mix(1, color, alpha) gives a dark red; with
white, the blades were a blue gray (112, 143, 163) and with gray 128 a
dark navy (38, 54, 67). Measured in Studio 0.742 on a
16 × 4 × 16 slab of each of the 21 materials, seen from above under flat
light (`EnvironmentDiffuseScale` and `EnvironmentSpecularScale` 0,
`Brightness` 0, `Ambient` and `OutdoorAmbient` gray), beside
SmoothPlastic parts in 9 grays whose pixels undo Studio's tone curve,
pixel by pixel, before averaging. Each slab was colored white, red,
green, blue, (255, 204, 204), (204, 255, 204), (204, 204, 255), gray 128
and gray 64; its default color and (255, 128, 128) were held back to
check the fit.

*Where Studio does it* (Studio's own shaders): the terrain shader
(`SmoothClusterFSUnified` in the install's `shaders_glsl3.pack`, plain
GLSL) takes the albedo already colored from a texture array, stored as
YCoCg and squared (gamma 2). No shader in the packs applies
`MaterialColors`, so Studio colors the textures on the CPU when it
builds that array. `FStringTerrainColorTable` holds no tint constants.

*What it does* (Confirmed for the shape, constants fitted): in gamma
space, where the map's tint mask is opaque, Studio keeps only the texel's
luma and multiplies it by a tint worked out from the color per material:

- **Brightness**: the color's largest channel (HSV value), times a
  per-material gain, at most 1. White, red, green and blue all draw as
  bright as each other; gray 128 draws half as bright as white. The gain
  is 1 for 16 materials, Rock 1.16, Ground 1.26, Asphalt 1.04, and Mud
  2.2 and Basalt 3.43, which reach full brightness at their default
  colors (Basalt is as bright in its dark gray default as in white). At
  white, Basalt draws at about 0.6 and Mud at 0.85 of the other
  materials' share of their textures' luma.
- **Chroma**: the color's YCoCg chroma over its largest channel, times a
  per-material 2 × 2 matrix (turning and scaling it), cut to a
  per-material length. Red turns magenta on Grass and LeafyGrass (about
  20 degrees), lavender on Slate (55), teal on Basalt (175). Concrete,
  Slate, Glacier and Salt take little of it (Salt almost none); Rock,
  Basalt, Asphalt, Pavement and Snow reach their length already at a
  pale color, so pink and red look alike on them.
- Where the tint mask is clear, the map shows as it is.

Fitted on the 7 colored shots, the brightness and chroma reproduce the
held-back colors within 6 of 255 on 20 materials. CrackedLava misses by
up to 48: its dark rock and lit cracks average unevenly (Approximation).
With the matrix the identity and no length, the tint would be the
color itself: a gray texture times the color, in gamma space.

Rook (`rook_terrain::TerrainMaterial::tint`, `src/material/tint.rs`, and
the terrain branch of `shaders/parts.wgsl`): the player sends the tint
as the surface's color, and the shader draws mix(texel, luma × tint,
alpha), luma being (r + 2g + b) / 4 of the linear texel (close to
Studio's for terrain's near-gray maps). Rendered from above, red Grass,
Sand and Basalt turn magenta, pink and teal, and in their default
colors Basalt draws about as bright as Grass, as in Studio. Rook's lighting
still flattens the contrast between materials (Sand against Grass 0.6
where Studio has 0.46, M13).
