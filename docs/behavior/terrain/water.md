# Terrain water

Checked against Studio 0.743 on 2026-10-19.

## Shape

Confirmed: surface nets moved down half a voxel,
the table's `"water": 0.5`. A lone full water voxel spans 3.333 to 0.667
studs below its center, a half-full one 2.667 to 1.333 below; a full
one-voxel slab spans 6 below to 2 above, its edge sloping as a solid's
does, 2 studs lower. Rook draws it in `WaterColor` at `WaterTransparency`,
mixed halfway towards the sky's color by `WaterReflectance` (Studio's
water is mostly the sky it reflects; Approximation).

## Shorelines

Measured in Studio 0.744 on 2026-10-28, built with `FillBlock`,
`FillWedge`, `FillBall` and `WriteVoxelChannels`, read back with
`ReadVoxelChannels` (`SolidMaterial`, `SolidOccupancy`,
`LiquidOccupancy`), and captured from the side and above at 4 to 20
degree fields of view (camera distance and field of view give studs per
pixel). Studio meshes terrain a moment after an edit, so a capture right
after one can show the old surface: capture again.

*What Studio stores*: a voxel holds a solid material with its occupancy
and, beside it, water (`LiquidOccupancy`). Water filled over terrain
(`FillBlock` of Water to y = 10 over a Sand slope) gives each partly
filled solid voxel the water the fill would have given air (Sand 0.25,
0.51 and 0.75 holding 1.0 below the water's top row, 0.5 in it), and
full solids none. `WriteVoxelChannels` stores what it's given, except
that a full solid drops its water (Rock 1.0 with water 1.0 reads back
water 0) and a solid of occupancy 0 becomes a Water voxel. `ReadVoxels`
reports a solid voxel's solid only. The channels read occupancy as the
byte over 255 (127 is 0.498), `ReadVoxels` as (byte + 1) / 256 (0.496).

Observed:

- **A solid voxel's water is drawn by its water alone**: lone Rock voxels
  of occupancy 0, 0.25 and 0.5, each holding water 0.25, 0.5, 0.75 and
  1.0, draw water boxes 0.64, 1.43, 2.11 and 2.87 studs across (lone
  Water voxels that full: 0.67, 1.33, 2.0 and 2.67, by [Shape](#shape)), the
  same for every solid occupancy, half a voxel low as Water voxels are,
  and whole where the rock overlaps them.
- **Solids count as inside water's surface**: a dry Rock voxel (0.25 or
  full) set into the top layer of a water slab leaves its surface whole,
  with no hole or dip to see from above or along it; a camera inside a
  rock wall 3 studs from the water beside it sees no water face at the
  wall, where one would lie if water stopped at solids.
- **Dry terrain away from water has no water**: a 5 × 5 slab of Rock at
  0.25, seen from below, shows only rock, where a water surface at the
  voxels' centers moved half a voxel down would show beneath it.
- So shores round off: water continues over the partly filled solids at
  its edge, under their surface, and the shoreline is where the two
  surfaces cross (a Grass ball in a lake meets the water in a circle).

- **Shores are smooth where the land under the water is dry**: in
  TerrainTest (2026-10-28), lakes whose last voxels meet partly filled
  solids holding no water still lie flat at their level up to where the
  land rises through them, and the shoreline follows the land's own
  curve, not the voxels' steps.
- **Thin dry land under the water's level stays covered**: dry Grass
  strips at 0.35 in a water layer's voxels (Studio 0.744, 2026-10-28,
  from above): 1 and 2 voxels wide, flat water covers them and only the
  blades show; 3, 4 and 6 wide, the land shows a voxel narrower than the
  strip; a 13 × 10 patch shows likewise.

Rook: water's surface is surface nets over every non-air voxel. A cell
has a water vertex when one of its corners is non-air, and a quad is
kept when one of its four cells has water at a corner (wet), so the
surface runs one cell on past the water into the land around, and dry
terrain away from water has none; a chunk with no water skips the pass.
In a wet cell each crossing is placed by how far water's surface reaches
out of a voxel (`Voxel::water_surface_fill`): a Water voxel's occupancy,
the water a solid holds, a whole voxel for a dry solid. So the water lies
flat at its level over the dry solids at its edge, and so does every
vertex inside the surface (all the quads around it kept), so water covers
thin dry land it surrounds. A vertex on the surface's edge, in a cell
with no water at a corner, places a dry solid's crossing by its own
occupancy instead, so the surface dips down under the land's surface
there and its edge is never in sight: the shoreline is where the land rises through the flat
water (Inference, matching Studio's shores in TerrainTest: the grass spit
at (-832, -4, -100) draws as Studio's). A few stair steps remain where
flat land lies just under the water's level, as the dip then shows
through it (Approximation). Keeping inner vertices flat costs little: the 25-million-water-voxel place meshes whole in 375 to 393 ms against 356 before, with 0.06% more triangles. Far rings average the same
reach ([detail](detail.md#how-rook-does-it)), so lakes keep their level. Benched against the build before
(2026-10-28, `--bench 300`, alternating runs): on the archive's largest
water place (25 million water voxels) frames and loading are unchanged
within noise (median frame 1.90 and 2.12 ms before, 2.03 and 2.19
after); on a 1,024-stud lake with 64 islands, 2% more triangles (the
shores) and frames unchanged.

## Seen from below

With the camera under the island lake's surface (Studio 0.744), Studio
draws the surface's underside: rippled and pale, the sky and the island
above the water showing faintly through it, the island's foot below the
water clear. Its water shaders have no side of their own (below), so the
underside is the same surface seen from below: its reflection weight
takes the view's cosine either way, and it reflects the sky's lower half.
Rook draws water in a transparent pass of its own that culls neither
side, with the same shader; from 20 studs under a lake (2026-10-28) both
show a rippled sky-blue underside over the shore's grass.

## What Studio's water shaders say

Studio's smooth water shaders (the entries using `WaterParams` in the
install's `shaders/shaders_glsl3.pack`, version-5a21b87bbc2641cf, read
2026-10-28) settle what the measurements below fitted:

- **Normal frames** lie 20 studs a repeat (coordinates of 0.05 per stud)
  on one of the 18 planes terrain textures use, picked per vertex, each
  vertex shifting them by its own random number, `(r, floor(r × 2.665))
  × 0.1`, the three vertices' samples blended across the triangle: the
  detiling terrain textures get. Two frames are bound and crossfaded.
  That's why no single frame laid plainly matched Studio's ripples.
- **The rippled normal** is the frames' whole slope; the reflection uses
  the surface normal moved a quarter of the way to it.
- **The reflection's weight** is `WaterReflectance × (clamp(0.78 - 2.5 ×
  |n·v|, 0, 1) + 0.3)`: 0.3 looking down, 0.65 at 10 degrees, 0.86 at 5,
  0.95 at 3 (measured: 0.27 to 0.3, 0.69, 0.80, 0.91).
- **The sky** is the skybox along the reflection at full detail (mip 0),
  times the sky's gradient, below the horizon as well as above.
- **The glint** is a GGX highlight off the full ripples, the frames' red
  channel giving the roughness (0.089 to 1), with a Fresnel of 0.02.
- **Distance**: the blended variant's opacity rises with the view depth
  × 0.004, so water lets nothing through from 250 studs.
- **Waves** move each vertex by `(sin((z - x)k - t) + sin((z + x)k + t)) ×
  A`, times a per-vertex weight and a fade with view depth; `k`, `A` and
  the fade come from uniforms not measured yet.

## Waves

Measured on the Baseplate place with a 64 × 4 × 64 block of Water beside
a ruler part,
screenshots taken with narrow fields of view (2 to 8 degrees) so the
ruler gives studs per pixel, and compared by their pixels (coverage,
autocorrelation, FFT matching). Rook's renders came from a copy of the
same place holding the same voxels, from the same cameras.

*What Studio's settings and install say*: nothing about the waves' shape;
`StudioAppSettings.json` has only the water transparency flags. The
install has 25 water normal maps
(`PlatformContent/pc/textures/water/normal_01.dds` to `normal_25.dds`,
256 × 256 DXT5 with X in alpha and Y in green). DevForum leads, checked below: Roblox's
water plays 25 frames; waves go one way and stop at `WaterWaveSpeed` 0.

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
45 studs, periods 40 and 25 units of phase; Approximation, Studio's two
diagonal sines of one wavelength not measured yet); the 25 frames (the
renderer's texture array, from the install) play at 1.25 per unit, each
fading into the next, laid as Studio's shaders lay them (above: 20
studs a repeat on the plane the surface faces most, shifted by a random
number in tenths per point of the 4-stud lattice, as Rook's terrain
textures are, since its vertices don't carry one). The surface is
`WaterColor` lit as a part's top is (on every face, 1.2 times brighter),
mixed towards the skybox at full detail, sampled where the reflection
points, by Studio's weight, with Studio's GGX glint; a fifth of
`WaterTransparency` of what's behind shows through, falling to none at
250 studs. Until 2026-10-28 `WaterReflectance` never reached the shader
(only Plastic's reflectance was passed on), so Rook's water reflected no
sky at all: from 320 studs above the island lake its middle drew 12,
120, 141 where Studio's draws 35, 128, 160; it now draws 40, 127, 159,
and along the surface it is the pale, rippled sky Studio's is. The
color table below was measured against that build. Approximation: the
waves' shape and how far the glint fades.

## Color and transparency

Measured on a 128 × 12 × 128
pool over the baseplate with sand 3 and 6 studs down, a white slab 4
down and a red pillar, from straight above (Studio 0.742, Brightness 3,
ClockTime 14.5, the default skybox, waves frozen):

| Water | Over | Studio | Rook |
|---|---|---|---|
| Defaults (transparency 0.3) | baseplate, 10 down | 36, 125, 156 | 38, 125, 157 |
| | sand, 6 down | 47, 136, 163 | 45, 136, 164 |
| | sand, 3 down | 51, 140, 166 | 46, 139, 168 |
| | white slab, 4 down | 56, 144, 173 | 50, 140, 170 |
| Transparency 1 | baseplate / sand 6 | 49, 124, 153 / 84, 157, 170 | 46, 126, 155 / 77, 152, 172 |
| Transparency 0, reflectance 0 | | 5, 119, 143 | 4, 121, 145 |
| ... and white | | 241, 245, 248 | 237, 242, 248 |
| Transparency 0, black | | 28, 58, 94 | 33, 54, 91 |

- Water is drawn at transparency 1, still teal: only a fifth of what's
  behind it shows through (half that in red), and depth from 3 to 10
  studs barely changes it.
- Its sides away from the sun are as light as its top, ripple as it does
  and show what's behind them as the top does.
- Its reflection rises steeply at low angles: against the sky it mirrors,
  0.3 at 20 to 30 degrees down, 0.69 at 10, 0.80 at 5 and 0.91 at 3
  (seen 1 stud above it, `WaterWaveSize` 0). So its top looks pale and
  sky-colored from a low camera and meets its teal sides in a crisp
  line. Rook within 20 levels at each angle, and its sky itself is 13
  levels bluer at the horizon than Studio's.
- The edge between top and sides has the same shape in both (profiles
  along it match within a few pixels at 38 px per stud).
- The sun glints off small ripples towards the sun only: 1.8% of the
  water is near white from above on the sun's side and 0.1% away from
  it, 5% seen along it towards the sun, and none along it away from the
  sun (Rook 1.6%, 0.7%, 1.9% and none).
- At Automatic quality, which every value here was measured at, Studio's
  water reflects only the skybox, as Rook's does (Confirmed, Studio 0.743,
  2026-10-19). An earlier read of a pillar "reflected upside down" was its
  underwater half showing through: the pillar was centered on the water
  line, so that half sits where a mirror image would. It goes with
  `WaterTransparency` 0 and when the pillar is lifted out of the water.
  The dark shapes beside a pillar are its shadow on the water and, offset
  by the sun's angle, its shadow on the floor seen through the water; both
  go with `GlobalShadows` off.

## Reflections at high quality

For when Rook gets a quality setting (see
[antialiasing](../rendering/antialiasing.md#for-rook)). From `EditQualityLevel` Level16
up, Studio's water also reflects parts: none at Automatic, Level08, 12, 14
or 15, a strong, rippled reflection at 16 and 21. It's screen-space
(Confirmed):

- With the pillar off screen (camera looking down on the water), the
  reflection is gone.
- With the pillar's top off the top of the screen, the reflection stops
  where the mirror image of the screen's top edge falls (measured: 648 px,
  predicted 648), not at the mirror image of the pillar's top (691).
- It follows the ripples (its edges are torn), so it's traced along the
  rippled normal, not a flat mirror plane.

Not measured yet: its strength against the sky reflection at each angle,
how far a ray reaches, how it fades near the screen's edges, whether
terrain and transparent parts show in it, and which in-game Graphics
Quality steps map to Level16.

*Method*: the pool above with the 4 × 20 × 4 red pillar raised to stand
on the water (center y 20.5), camera at (200, 16, 20) looking at
(232, 8, 20), FOV 70; set `settings().Rendering.EditQualityLevel`, wait,
capture; put it back to Automatic afterwards.
