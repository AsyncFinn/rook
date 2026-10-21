# Tone mapping

Checked against Studio 0.743 on 2026-10-19.

First measured in Studio 0.742 on 2026-10-16 (M7 step 3). It came from
a place where a poster Decal on a Neon union looked washed out in Rook:
the poster's dark areas drew about 40 in Rook and 12 in Studio.

Studio adds up light in **light units** (Ambient's color squared, a
light's Brightness × Color squared, Neon's glow) and draws them into a
frame holding `sqrt(light / 4)`: 4 units is the most the frame keeps.
Decals, Transparency and particles blend in that encoding, and one tone
curve at the end turns the frame into pixels. `ColorGradingEffect`'s
`TonemapperPreset` picks the curve. (Confirmed: the surface shaders in the
install's `shaders_glsl3.pack` end in `sqrt(clamp(color × scale))`, and its
bloom and composition shaders decode the frame as 4 × value²; every
number below fits this model.)

## The frame model

- **Default curve**: the Ambient ramp measured on white SmoothPlastic in
  a place without a ColorGradingEffect ([lights](local-lights.md#how-studio-draws-them):
  0.25 units draw 134, 1 unit 224, 3.5 units 253). Black Decals over white
  Neon decode to 4T² units at Transparency T, and the curve gives 9, 35,
  99, 160, 199, 224 and 243 for T = 0.05 to 0.7, against Studio's 8, 31,
  97, 158, 199, 223 and 245. Lavender Neon (0.38 units) at T = 0.2 and 0.5
  gives 12 and 68 (Studio 12 and 67). Places without a ColorGradingEffect
  get this curve.
- **Retro**: no curve; the pixel is the light's square root, so black
  Decals over white Neon draw 510 × T until they clip, as measured.
- **Neon** glows at 13.6 × c⁴ units per channel (c its sRGB color, 0 to
  1), up to the frame's 4: gray 128 is 0.865 units under both curves (237
  under Retro, 215 under the default), 64 gray 0.055 (60 under Retro),
  blue 105 0.38, and 160 gray 2.1, which is why it needs 1.4 times the
  Transparency of a capped color to draw the same (the 1.4 below).
  Colors from about 188 up all reach the cap.
- **Ambient**: white SmoothPlastic (248) under Ambient 80 is 0.093 units:
  66 under the default curve and 79 under Retro, as Studio drew both.
- **ExposureCompensation** scales light units before the curve: Ambient
  175 on white is 0.47 units (177; Studio 179), and −0.6 takes it to 0.31
  (148; Studio 148).

## Findings

Black Decals at several `Transparency` values on 3×3 parts, in a dark
scene (Ambient 80 gray, Brightness 0, OutdoorAmbient black), with the
place's ColorGrading (preset Retro) on and off. The values are the blue
channel, 0–255:

| Surface under the decal | Preset | 0.05 | 0.1 | 0.2 | 0.3 | 0.4 | 0.5 | 0.7 | 1 (no decal) |
|---|---|---|---|---|---|---|---|---|---|
| Neon (248, 248, 248) | Retro | 22 | 52 | 101 | 153 | 206 | 255 | 255 | 255 |
| Neon (255/220/192 gray) | Default | 8 | 31 | 97 | 158 | 199 | 223 | 245 | 253 |
| Neon (91, 93, 105) | Retro | 2 | 11 | 29 | 49 | 64 | 78 | 109 | 159 |
| Neon (91, 93, 105) | Default | 0 | 2 | 12 | 27 | 44 | 67 | 108 | 162 |
| Neon (128 gray) | Retro | 6 | 20 | 48 | 71 | 93 | 116 | 164 | 237 |
| Neon (128 gray) | Default | 2 | 7 | 26 | 54 | 86 | 117 | 168 | 215 |
| SmoothPlastic (248 gray) | Retro | 1 | 3 | 11 | 20 | 30 | 40 | 57 | 79 |
| SmoothPlastic (248 gray) | Default | 0 | 1 | 3 | 7 | 12 | 19 | 36 | 66 |

- **Neon is brighter than white.** Behind a black decal, white Neon still
  draws pure white at Transparency 0.5 under Retro (223 under the default
  curve). Under the default curve, Neon of 192, 220 and 255 gray all draw
  the same at every Transparency, so the glow is capped before the curve.
  160 gray reaches the same values at about 1.4 times the Transparency,
  so the cap is about 1.4 times what 160 emits.
- **Retro blends as if in gamma space.** Above the darkest values, the
  output is proportional to Transparency in sRGB values (white Neon is
  510 × Transparency, about twice white, until it clips). Values near
  black fall a little below that line.
- **The default curve crushes darks.** Every row comes out below both a
  linear-light blend and a gamma-space blend at low Transparency (lavender
  Neon at 0.5: 67, where a linear blend gives 115 and a gamma blend 81).
- **The preset also changes surfaces with no decal on them.** 128 gray
  Neon draws 237 under Retro and 215 under the default curve. Rook's Neon
  fit (128 → 237) matches Retro, though it was fitted in a Baseplate
  without ColorGrading; the default curve may have changed since, or that
  Baseplate's settings differed. Recheck this before refitting.

The vending machine place this came from: its poster is a Decal at
Transparency 0.1 on a Neon union with `UsePartColor` off, so the union
draws its mesh's lavender (Rook gets that right). Blending in gamma
space gives Studio's 12 exactly; Rook's linear blend gives 40. With
ColorGrading off, Studio's poster stays dark and saturated.

Not measured yet: the other presets; color (not gray) decals; how
Bloom, ColorCorrection and `ExposureCompensation` fit in; whether a part's
own Transparency blends the same way; `Lighting.Technology` (scripts
can't read it, and the test Baseplate's may differ from the place's
Future).

## Method

1. Copy the place's Lighting properties and effects into the Baseplate
   Studio. A place file loaded with `game:GetObjects` comes back without
   Workspace, so first save its Workspace children at the top level of an
   `.rbxm` (rook-formats `load`, `transfer_within` to the root, `save`),
   and put it in the install's `content` folder.
2. Put a solid black 16×16 PNG in `content` and use it as
   `rbxasset://` Decal textures on rows of parts, one Transparency per
   column.
3. Capture face-on through Studio's MCP server with a 30° field of view,
   and average a 5×5 grid of pixels in the middle of each square (the
   captures are JPEG).
4. Toggle `ColorGradingEffect.Enabled` and capture again. Afterwards,
   restore the Baseplate's Lighting and Workspace, and delete the files
   from `content`.

## ExposureCompensation

Measured on 2026-10-16 from a control room that looked far too light in
Rook (Ambient 175 gray, Brightness 0, ExposureCompensation −0.6). In a
Baseplate with that Lighting, −0.6 against 0 took white SmoothPlastic
from 179 to 148, 163-gray SmoothPlastic from 112 to 84 and 163-gray Metal
from 92 to 67; the sky went from (85, 163, 202) to (61, 129, 174).

Rook multiplies linear light by 2^ExposureCompensation (`Lighting::exposure`,
applied in `fog` and to the sky), which lands within about 8 of those.
Local lights take it before their tone curve, as Studio does, so lit walls
don't dim as much as unlit ones. In the room, unlit consoles now draw
95 to Studio's 91. Its carpet floor is still 59 to Studio's 24: a black
Texture (alpha about 0.65, Transparency 0.2) over Metal, the
blend-before-the-curve case above.

## Fog

Studio's surface and particle shaders keep exp2(z × distance + x) − w of
a surface, clamped to 0 to 1, and mix the rest towards FogColor (a
particle's opacity fades instead). Measured in Studio 0.743 on
2026-10-19: white SmoothPlastic squares facing the camera at 12 distances
(5 to 120 studs), lit by Ambient white alone, FogColor black, read as
light units:

| t = (distance − FogStart) / (FogEnd − FogStart) | 0.1 | 0.2 | 0.3 | 0.4 | 0.5 | 0.6 | 0.7 | 0.8 | 0.9 |
|---|---|---|---|---|---|---|---|---|---|
| surface kept | 0.89 | 0.76 | 0.64 | 0.52 | 0.41 | 0.32 | 0.23 | 0.14 | 0.07 |

(averaged with FogColor 128 gray, which reads 1 − kept). FogStart 50 /
FogEnd 100 and 0 / 200 land on the same values at the same t, and the
kept share is 2^(1 − t) − 1 within 0.025 (x = 1, w = 1, z = −1 /
(FogEnd − FogStart), FogStart shifting the start). Full fog draws
FogColor's color squared in light units (128 gray drew 130, the curve
gives 129). The sky isn't fogged.

Rook: `fog_amount` and `fog_units` in `shaders/lighting.wgsl`, before
the exposure.

## How Rook does it

The main pass draws into a frame (`renderer/tonemap.rs`, Rgba16Float to
avoid the 8-bit frame's banding) holding Studio's encoding, `frame` in
`shaders/tone.wgsl`, and every pipeline blends there as before. The tone
pass then decodes it and applies the default curve or Retro's, from the
first enabled ColorGradingEffect in Lighting.

Light adds up in light units (`diffuse_light` in `shaders/lighting.wgsl`):
the ambient as its color squared, the sky's diffuse light and the local
lights as measured, the sun and its highlight as Studio's shaders add
them ([lights](sun.md)), Neon as above, fog as below, and the
exposure before the curve; surface colors are their sRGB values squared.
Unlit particles are their image times color, squared, times Brightness,
as Studio's particle shaders draw them. The looks still fitted on
Studio's pixels reach the frame through the curve's inverse (`frame_of` in
`tone.wgsl`): skyboxes and the plain sky, and ForceField's rim; water's
glint is in light units but unmeasured (as bright as the sun's light on a
face square to it). Glass's filter is a scale on the light, before the
curve.

Against the table above (`rook-player --screenshot` of a test place with
black parts at each Transparency over Neon, which blend as Decals do),
Rook lands within 6 levels everywhere under the default curve (white Neon
9, 35, 100, 160, 199, 224, 243, 253; gray 128 and lavender within 6), and
Retro's uncovered Neon draws Studio's 237 and 159. Rook's old linear fog lifted
near-black see-through covers by up to 5 levels with Roblox's defaults
(FogStart 0, FogEnd 100000); the fog below tints less at a given
distance.

Not done: ColorGradingEffects under the Camera, the other presets, and
Bloom and ColorCorrection (M13).
