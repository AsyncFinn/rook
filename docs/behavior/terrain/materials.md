# Terrain materials

Checked against Studio 0.744 on 2026-10-28.

Terrain is drawn with
its materials' textures, recolored by `Terrain.MaterialColors`; MaterialService
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
`tiling`, `detiling`, `rotation` and `blend` settings (`blend` goes
unused: [Blending](blending.md)); Water has
`"water": 0.5`. `FStringTerrainColorTable` holds each set's default
colors (`baseColors...`) and each map's average (`avgAlbedo...`: the root
mean square of its texels, in linear light; Grass's legacy top map
measures (89, 94, 50) against the table's (91, 96, 51)). Studio shapes
terrain by the Pre2022 table only and textures it by the place's set's
(below); Rook keeps both in `rook-terrain` (`src/material/looks.rs`)
with both sets' default colors.

- **Shapes follow the Pre2022 table in every place** (Confirmed): every
  shape step 1 measured matches it (Concrete quantize 0.5, Pavement 0.25,
  Cobblestone 0.1499, Brick and WoodPlanks cubify 1, Slate and CrackedLava
  both shift 0.1), measured in a place saved with `Use2022Materials` on,
  where the 2022 table would step Concrete by whole cells and shift
  neither Slate nor Basalt. Studio drew the copy saved with it off alike.
- **Textures follow the place's set** (Confirmed, Studio 0.744,
  2026-10-28): `MaterialService.Use2022Materials` (saved as
  `Use2022MaterialsXml`) switches terrain between the Pre2022 and 2022
  tables' packs: switched in Studio on a large terrain place, the
  terrain's textures changed with it (Sand from tan to near white, Mud
  from brown to gray-violet), and with it off, Mud set to red drew bright
  pink, the legacy recolor, where the 2022 tint would draw it dark red.
  This reverses an earlier same-day claim that two copies of a place
  saved with it on and off drew terrain alike (Studio keeps material
  textures cached; see the measurement notes on fresh processes).
- **The color maps are colored** ([Colors](#colors)): the legacy packs
  say `alphamode` 1 (opaque) and every texel is recolored; the 2022 packs'
  alpha is a tint mask (`alphamode` 2) and the texels are tinted.
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
  16 studs.
- **Colors**: a place that saved no `MaterialColors` gets its set's
  defaults (a new place saves the 2022 set's). How a color colors the
  textures is in [Colors](#colors) below.

*MaterialVariants* (Inference, by parts' rules): a MaterialVariant that
MaterialService's override names for a terrain material (`GrassName` and
so on), of that base material, replaces the material's textures on
terrain, tiled at its `StudsPerTile` and tinted by the material's color.
Voxels can't name a variant, so only overrides apply.

*Loading*: terrain textures load in the background like part materials
(one pack per material and facing the place uses), and the terrain is
redrawn as they arrive.

## Colors

Measured 2026-10-28 in Studio 0.743 on a 16 × 4 × 16 slab of each of the
21 materials, seen from straight above (field of view 8 degrees) under
flat light (`Brightness` 0, `EnvironmentDiffuseScale` and
`EnvironmentSpecularScale` 0, `Ambient` and `OutdoorAmbient` white), beside
SmoothPlastic parts in 9 grays whose pixels undo Studio's tone curve, in
fresh processes of the legacy place and its 2022 copy. Each slab was
shot in its default color, white, red, green, blue, gray 128 and (255,
128, 128); the slabs sit in the same voxels in every shot, so each pixel
pairs a texel in the default color with the same texel in the others.

*Where Studio does it* (Studio's own shaders): the terrain shader
(`SmoothClusterFSUnified` in the install's `shaders_glsl3.pack`, plain
GLSL) takes the albedo already colored from a texture array, stored as
YCoCg (luma in green, Co and Cg in red and blue biased by 0.5) and squared
after decoding. No shader in the packs applies `MaterialColors`, so
Studio colors the textures when it builds that array.

*What it does with the legacy textures* (Confirmed): each texel (sRGB, 0
to 1) is moved in YCoCg by how the color differs from the material's
legacy default color (`FStringTerrainColorTable`'s `baseColorsPre2022`):

- **Luma** times *k* = min(2, the color's HSV value over the default's).
  White and gray 128 draw Grass at 2 and 1 times its default luma; Mud's
  and Basalt's dark defaults reach the cap of 2 already at gray 128, so
  they draw alike in gray and white (as the old fit's per-material gains
  2.2 and 3.43 were, which are 128 over their defaults' values).
- **Chroma** (Co, Cg) turned by the color's HSV hue less the default's,
  and scaled by *k* times min(2, the color's HSV saturation over the
  default's). White and grays have no saturation, so they draw the
  texture gray; red turns Grass magenta and Basalt teal, as the
  default's hue lies on the other side of the texture's.
- Luma stays within 0 to 1 and chroma within 0.5 either way, as the
  array stores them; decoded channels can pass 1.

Per pixel, this rule draws the 126 shots within 4.5 of 255 on average,
JPEG noise included (Limestone and Ice, the brightest, within 9.3). Caps
of 1.5 or 2.5 instead of 2 miss by 23 and 11; turning the hue in HSV per
texel instead of in the chroma plane misses by 22.

*What it does with the 2022 textures* (Confirmed, from its shaders; checked
on Mud): each texel (sRGB) is multiplied by `mix(1, color, alpha)`, the
color map's alpha being its tint mask, as the terrain shaders in
`shaders_glsl3.pack` tint each layer (`mix(vec3(1.0), tint, albedo.w) *
albedo.xyz`, before squaring). Mud's 2022 texture, its average albedo 193
of 255, draws about 107 of 255 lit in its 2022 default color (121, 112,
98), beside Sand drawn by the same rule. On the large terrain place
with the setting on (Mud in its legacy default color, nearly untinted
where its mask is low), Studio and Rook draw Mud 104, 105, 112 and 110,
109, 113, Sand 210, 212, 204 and 212, 213, 203, and Ground 109, 112, 111
and 119, 120, 115. Grass blades take the 2022 top
texture's average (`avgAlbedo2022`) times the color (Approximation: that
average is a root mean square, and the mask is taken as whole).

Rook (`rook_terrain::TerrainMaterial::coloring`, `src/material/recolor.rs`,
`recolor` in `shaders/terrain.wgsl` and the tint in `parts.wgsl`): the
legacy gain and turn, or the 2022 tint, go to the renderer in the
surface's color, and the shader colors each filtered texel in sRGB before
squaring it as Studio does (Studio recolors before filtering; the two
differ only where the clamps act).
