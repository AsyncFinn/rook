# Tone mapping

Measured in Studio 0.742 on 2026-10-16, for M13's post-processing. It came
from a place where a poster Decal on a Neon union looked washed out in
Rook: the poster's dark areas drew about 40 in Rook and 12 in Studio.

Rook has no tone curve. It draws to an 8-bit sRGB target, fits each look
straight to Studio's final pixels (Neon's 18c², for example), and blends
Decals and Textures in linear light. Studio draws in HDR, blends there,
and then applies a tone curve, which `ColorGradingEffect.TonemapperPreset`
can change. Blending before the curve gives a different result from
blending colours that have already been through it.

## Findings

Black Decals at several `Transparency` values on 3×3 parts, in a dark
scene (Ambient 80 grey, Brightness 0, OutdoorAmbient black), with the
place's ColorGrading (preset Retro) on and off. The values are the blue
channel, 0–255:

| Surface under the decal | Preset | 0.05 | 0.1 | 0.2 | 0.3 | 0.4 | 0.5 | 0.7 | 1 (no decal) |
|---|---|---|---|---|---|---|---|---|---|
| Neon (248, 248, 248) | Retro | 22 | 52 | 101 | 153 | 206 | 255 | 255 | 255 |
| Neon (255/220/192 grey) | Default | 8 | 31 | 97 | 158 | 199 | 223 | 245 | 253 |
| Neon (91, 93, 105) | Retro | 2 | 11 | 29 | 49 | 64 | 78 | 109 | 159 |
| Neon (91, 93, 105) | Default | 0 | 2 | 12 | 27 | 44 | 67 | 108 | 162 |
| Neon (128 grey) | Retro | 6 | 20 | 48 | 71 | 93 | 116 | 164 | 237 |
| Neon (128 grey) | Default | 2 | 7 | 26 | 54 | 86 | 117 | 168 | 215 |
| SmoothPlastic (248 grey) | Retro | 1 | 3 | 11 | 20 | 30 | 40 | 57 | 79 |
| SmoothPlastic (248 grey) | Default | 0 | 1 | 3 | 7 | 12 | 19 | 36 | 66 |

- **Neon is brighter than white.** Behind a black decal, white Neon still
  draws pure white at Transparency 0.5 under Retro (223 under the default
  curve). Under the default curve, Neon of 192, 220 and 255 grey all draw
  the same at every Transparency, so the glow is capped before the curve.
  160 grey reaches the same values at about 1.4 times the Transparency,
  so the cap is about 1.4 times what 160 emits.
- **Retro blends as if in gamma space.** Above the darkest values, the
  output is proportional to Transparency in sRGB values (white Neon is
  510 × Transparency, about twice white, until it clips). Values near
  black fall a little below that line.
- **The default curve crushes darks.** Every row comes out below both a
  linear-light blend and a gamma-space blend at low Transparency (lavender
  Neon at 0.5: 67, where a linear blend gives 115 and a gamma blend 81).
- **The preset also changes surfaces with no decal on them.** 128 grey
  Neon draws 237 under Retro and 215 under the default curve. Rook's Neon
  fit (128 → 237) matches Retro, though it was fitted in a Baseplate
  without ColorGrading; the default curve may have changed since, or that
  Baseplate's settings differed. Recheck this before refitting.

The vending machine place this came from: its poster is a Decal at
Transparency 0.1 on a Neon union with `UsePartColor` off, so the union
draws its mesh's lavender (Rook gets that right). Blending in gamma
space gives Studio's 12 exactly; Rook's linear blend gives 40. With
ColorGrading off, Studio's poster stays dark and saturated.

Not measured yet: the other presets; colour (not grey) decals; how
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
Rook (Ambient 175 grey, Brightness 0, ExposureCompensation −0.6). In a
Baseplate with that Lighting, −0.6 against 0 took white SmoothPlastic
from 179 to 148, 163-grey SmoothPlastic from 112 to 84 and 163-grey Metal
from 92 to 67; the sky went from (85, 163, 202) to (61, 129, 174).

Rook multiplies linear light by 2^ExposureCompensation (`Lighting::exposure`,
applied in `fog` and to the sky), which lands within about 8 of those.
Local lights take it before their tone curve, as Studio does, so lit walls
don't dim as much as unlit ones. In the room, unlit consoles now draw
95 to Studio's 91. Its carpet floor is still 59 to Studio's 24: a black
Texture (alpha about 0.65, Transparency 0.2) over Metal, the
blend-before-the-curve case above.

## For Rook

Draw to an HDR target (Rgba16Float), leave Neon unclamped up to its cap,
blend Decals and Textures there, and finish with the default curve or
Retro's, chosen by ColorGrading. The colour fits that have the curve baked
in (Neon's, the lighting constants) then need refitting against it.
Blending only Decals and Textures in gamma space would match Retro but not
glowing Neon behind a see-through decal, and it still needs a different
colour target.
