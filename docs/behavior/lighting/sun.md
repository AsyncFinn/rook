# The sun

Checked against Studio 0.743 on 2026-10-19.

Measured after [tone mapping](tone-mapping.md) (M7 step 3), in the Baseplate under the default curve (no ColorGradingEffect).
Pairs of white and black SmoothPlastic slabs floated 400 studs up, their
faces turned 0° to 120° from the sun in the plane of the sun and the
camera (the camera 60° from the sun, so the highlight peaks at 30°),
with Ambient and OutdoorAmbient black and both environment scales 0.
Pixels were read as light units through the curve's inverse: the white
slab less the black one is the sun's diffuse light, the black slab alone
its highlight.

Studio's surface shaders (`shaders_glsl3.pack`) add the sun as
`Lamp0Color` (a color in light units) times the cosine to the sun
(times the shadow) times the albedo, which is the sRGB color squared,
plus a highlight that takes no albedo:

- **0.5 light units per `Brightness`** on a face square to the sun:
  white minus black over the cosine gave 0.49 to 0.52 at every angle at
  Brightness 0.5 and 1, and 0.50 to 0.58 at 3 (the curve's inverse is
  coarse above 1 unit). Linear in Brightness and in the cosine, as the
  shaders say.
- **The highlight** is the shaders' own distribution, not textbook GGX:
  roughness remapped to r = 0.089 + 0.911 × roughness, a = r⁴,
  F × (a + a²) / (d² × (3 L·H + 0.5) × (0.75 N·H + 0.25)) with
  d = (N·H)² (a − 1) + 1 and Schlick's F on L·H (base 0.04), times the
  sun's light and the cosine. Not scaled by `EnvironmentSpecularScale`.
  On black SmoothPlastic it gave 0.003, 0.012, 0.112, 0.022 and 0.002
  units per Brightness at 0°, 15°, 30°, 45° and 60°; roughness 0.4 fits
  every one within 0.005 (0.002, 0.012, 0.116, 0.025, 0.002). Plastic
  fits 0.65 (0.007, 0.017, 0.034, 0.019, 0.005 measured).
- **`EnvironmentSpecularScale`** scales metalness in these shaders (a
  metal at scale 0 shades as a plastic), and takes F × scale × (1 −
  metalness) of the sun's diffuse light away for the highlight.
- **Time of day** colors the sun only near the horizon (at latitude 0;
  the sun's height y is the sine of its elevation): white from about
  y = 0.15, then (0.91, 0.92, 0.51) × 0.5 per Brightness at y = 0.096,
  (0.70, 0.70, 0.03) at 0.048, (0.40, 0.40, 0) at 0.024, and nothing at
  the horizon.
- **At night the moon lights** from `GetMoonDirection()`: 0.031 units per
  Brightness, white, with the moon 66° up (only height measured). The
  shaders' second lamp (`Lamp1Color`, on faces turned away) gave nothing
  with ColorShift_Bottom black.
- **`ColorShift_Top`** tints the sun, not simply: (255, 128, 0) took it
  from 0.5 per Brightness in each channel to (1.0, 0.35, 0.13), and
  `ColorShift_Bottom` (0, 128, 255) lit the turned-away faces a little
  blue. Not modeled.

Rook (`sun_light` and `shade_pbr` in `shaders/lighting.wgsl`;
`sun_color` in `crates/rook-scene/src/lighting.rs`) adds the sun and
its highlight in light units as above: `Lighting::sun_color` is
0.5 × Brightness, yellowed near the horizon (1 − e^(−y/0.04) on red and
green, a smoothstep from y = 0.045 to 0.15 on blue; fitted), or the
moon's 0.031 × Brightness while the sun is down. Albedos are the sRGB
color squared (`albedo_units` in `shaders/tone.wgsl`). SmoothPlastic and
Plastic take the fitted roughness. The sky's reflection is Studio's (see
[materials](../rendering/materials.md#reflections)).

Against Studio: the same rig in white, red (196, 40, 28), gray 128, blue
(13, 105, 172) and black, at Ambient 70, OutdoorAmbient 128, Brightness
2, ClockTime 14, latitude 0, both environment scales 1, the default sky
(`rook-player --screenshot --script` on the Baseplate, from the same
camera). 40 faces, 120 channel values: before, Rook was off by 14 levels
on average (sunlit faces 10 to 28 too bright, the gray 13 too dark); now
by 3.4. Sunlit faces of white and gray land within 5, red and blue
within 5 in their bright channels. Left: the dim channels of red read 5
to 7 high (15 high in blue on the face square to the sun), and black
faces away from the highlight 5 to 10 high, from the sky's reflection.

## Shadows

Checked against Studio 0.744 on 2026-10-26, in the Baseplate (LightingStyle
Soft), by `screen_capture` with `PrioritizeLightingQuality` set from a
script each way. Studio no longer has `Lighting.Technology`: every place
gets the same lighting, whatever a file saved (scripts still can't read
the hidden property).

**`PrioritizeLightingQuality` picks how the sun shadows** (Observed):

- **On: a shadow map.** Sharp shadows: a slab's square, a post's long
  thin shadow. A 2048-stud deck 286 studs up shaded the ground out to
  about 580 studs from a camera 40 studs up, and lit it beyond: the ring.
- **Off: the light voxels.** No sharp shadow at all. Shadows are soft,
  blocky blobs from the same 1024 × 256 × 1024-stud grid of 4-stud voxels
  the local lights use ([local lights](local-lights.md#how-far-the-grid-reaches)),
  so they reach only as far as the grid. The deck 286 studs up cast
  nothing (it's above the grid); moved down to 60 it cast a soft shadow
  that leaked light at the seams between its plates. From 200 studs up,
  looking down, nothing on the ground cast anything (the grid followed
  the camera's focus up, off the floor). A 6-stud cube on the ground
  leaves only a faint shadow.
- **A part blocks the whole voxel, and the shadow softens with distance**
  (measured in Studio 0.747 on 2026-11-14, `PrioritizeLightingQuality`
  off, ClockTime 14.5, latitude 0, looking straight down at the darkest
  point of each shadow on a white floor that drew 231). Square slabs 32
  studs wide, 20 studs up, 0.2, 1, 2, 4 and 16 studs thick all drew 82:
  a voxel a part touches stops the sun as a full one does, and a run of
  them counts once. Wider and lower is darker, and the shadow spreads
  and pales the further it falls, as through a cone that widens with
  distance: 4-stud slabs 20 up drew 224, 177, 82 and 74 at 8, 16, 32 and
  64 wide; 16 wide, 74, 177 and 216 at 10, 20 and 40 up; 32 wide, 82, 157
  and 202 at 20, 40 and 80 up. 74 is the full shadow. Two 1-stud, 16-wide
  slabs 12 studs apart, one over the other, drew 163, darker than either
  alone; side by side across the cone (each covering half of it from the
  point) 130: blockers apart each take their share.
- **A full voxel shadow keeps a little of the sun** (same setup). With
  the sun off (`Brightness` 0) the floor drew 55 everywhere, under the
  slabs and in the open; with the shadow map (`PrioritizeLightingQuality`
  on) the floor under a 64-stud slab drew 55 too. Through the voxels it
  drew 74 under slabs 64, 96, 128 and 160 studs wide alike, and 65 at
  `Brightness` 1.5 (open floor 198): the voxels never block all of it.
- **Terrain shadows the sun too** (measured in Studio 0.747 on
  2026-11-14, `PrioritizeLightingQuality` off, ClockTime 14.5, latitude
  0, looking down from 140 studs: 16 × 4 × 16 slabs of Rock one voxel
  thick, 20 studs over a white floor, each at one occupancy). A full
  slab's shadow is as deep as a part slab's (the floor drew 231, the
  shadows 180 to 186), and stays that deep down to 0.35 occupancy; below
  that it fades: 205 at 0.25, 220 at 0.15, 224 at 0.1, 228 at 0.05, and
  nothing at the least occupancy. A slab of water casts nothing.
- **Unsaved, it's off** (Observed by the user: an archive place showed
  no ring and no shadow from the deck until they turned it on). Studio's
  Baseplate template saves it on.

**Rook** (`sunlit` in `shaders/light_volume.wgsl`, `voxel_sun_visibility`
in `shaders/lighting.wgsl`; Approximation, fitted): with
`PrioritizeLightingQuality` on, the shadow map as before, one 2048²
cascade out to 300 studs, fading over the last 60. With it off, no
shadow pass: the light volume also holds how much of the sun reaches
each voxel, through a cone towards the sun (voxel cone tracing, as in
Crassin's 2011 global illumination). The sun cover
(`renderer/lighting/local/cover.rs`, built by `shaders/sun_cover.wgsl`)
holds how much of the sun each voxel stops, all of it in a voxel any
opaque `CastShadow` part touches, with mips that average each layer over
2 to 16 voxels across. Each voxel's cone runs up a layer at a time,
widening by 0.2 voxels in radius per voxel it runs, and reads at each
layer the share of its footprint the cover stops, from the mip as wide
as the footprint. A run of layers that stop some counts as one blocker,
the layer stopping most, and runs apart each take their share. Surfaces
read the result 6 studs out along their normal, like the local lights,
and even a full shadow keeps 0.03 of the sun (`SUN_SHADOWED`, fitted:
under the 64-stud slab Rook draws 10 and 19 levels above its sunless
floor at `Brightness` 1.5 and 3, as Studio does).
The voxels take the sun again when it turns more than about half a
degree. The volume moves by whole 64-stud tiles, so the mips' texels
stay put in the world.
Terrain is marked into the same voxels
(`renderer/lighting/local/terrain.rs`), each voxel by how full of solid
terrain it is (air and water count as empty), and stops a share of the
sun by it, fitted to the terrain slabs above (`SOIL_SUN`: all of it from
0.35 up, 0.54, 0.23, 0.15 and 0.06 of it at 0.25, 0.15, 0.1 and 0.05), so
its shadows fade with occupancy as Studio's do.

Against Studio, on the slabs above, as a share of each one's full
shadow (Rook's floor draws 228 and its full shadow 57, before the full shadow kept some of the sun): within 0.05 of
Studio's everywhere, 0.03 on average. The 32-wide slab 20 up: 0.91
against 0.95; 16 wide: 0.31 against 0.34, 1 against 1 at 10 up, 0.08
against 0.1 at 40 up; 32 wide 40 and 80 up: 0.49 and 0.16 against 0.47
and 0.18; 8 wide: 0.08 against 0.04; 64 wide: full in both. The slabs'
thicknesses don't change Rook's shadows either.

The deck 286 studs up casts nothing in either.

Left: with `PrioritizeLightingQuality` on, Rook's shadow map still
reaches 300 studs where Studio's reaches about 580. Rook's sunless
floor is 2 levels brighter than Studio's (57 against 55), so its full
voxel shadow is too (76 against 74). Blockers stacked apart are not yet
compared against Studio's.
