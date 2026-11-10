# The sky over the day

Checked against Studio 0.745 on 2026-11-05.

How Studio darkens and tints the sky at dawn and dusk, and how that
reaches what the sky lights and what reflects it. Measured in
`version-fee454b8e2094ba9`.

## The tint

From 6:30 to 17:30 the skybox draws as its images. Before and after, it
is multiplied by a tint that goes by `ClockTime` alone: captures at
`GeographicLatitude` 0 and 60 (a different sun height at each time) came
out the same, and a capture 1 s and 6 s after a change were identical, so
it doesn't fade in. The tint is about the same overhead, 30 degrees up and
at the horizon (within 0.05), so one color per time describes it.

Shares of the noon sky, per channel, read as light units (each pixel
through the inverse of the default tone curve, [tone
mapping](tone-mapping.md)), averaged over the three heights, on the
default sky:

| ClockTime | Tint (R, G, B) |
|---|---|
| 6.0 and before | 0 (the night sky) |
| 6.1 | 0.14, 0.11, 0.09 |
| 6.2 | 0.28, 0.24, 0.22 |
| 6.29 | 0.45, 0.41, 0.39 |
| 6.38 | 0.65, 0.63, 0.61 |
| 6.44 | 0.82, 0.80, 0.78 |
| 6.5 to 17.5 | 1 (byte-identical to noon) |
| 17.62 | 0.72, 0.66, 0.60 |
| 17.71 | 0.55, 0.45, 0.37 |
| 17.8 | 0.40, 0.29, 0.21 |
| 17.9 | 0.25, 0.15, 0.08 |
| 18.0 | 0.14, 0.06, 0.01 |
| 18.3 and after | 0 (the night sky) |

Dawn stays gray; dusk turns orange as it darkens. After about 18.3 and
before about 6.0 the clouds are gone and only a dark night gradient shows,
with a purple glow along the horizon and stars.

Read as squared pixels instead, the zenith and the horizon seemed to take
different shares; that was the tone curve's toe, not the tint.

## What it reaches

- **The sky's diffuse light** (`EnvironmentDiffuseScale`) takes the tint
  once: a white SmoothPlastic part lit only by it (Brightness 0, Ambient
  and OutdoorAmbient black) kept (0.49, 0.43, 0.35) of its noon light at
  6.29.
- **Reflections of the sky** take it twice. Black opaque water lit by
  nothing but its reflection kept 0.41 at 6.38 and 0.17 to 0.20 at 6.29,
  the square of the sky's 0.65 and 0.43 there; a Reflectance 1
  SmoothPlastic part kept (0.18, 0.11, 0.06) at 6.29. Studio's water and
  surface shaders (`shaders_glsl3.pack`) multiply the reflection cube's
  sky by `mix(SkyGradientBottom, SkyGradientTop, clamp(R.y × 1.588))`;
  the cube itself holds the sky as drawn, so the tint comes in once with
  it and once with the gradient.
- **The water's reflection** is also scaled by the light grid's sky
  visibility at the surface, with a tenth of the local lights' light added
  (the same shaders).
- `Brightness` doesn't change the sky or its reflection: at noon, black
  opaque water reflects the sky as strongly at Brightness 0 as at 3. Water
  that is see-through looks darker at Brightness 0 only because the unlit
  ground under it shows through.

## How Rook does it

`sky_tint` in `crates/rook-scene/src/lighting/sky.rs` holds the table and
interpolates linearly between the measured times (`Lighting::sky_tint`).
The renderer multiplies the drawn sky by it (`sky.wgsl`), the sky's diffuse
light and the covered gray by it (`sky_light`, `sky_covered` in
`lighting.wgsl`), and every reflection of the sky by its square
(`sky_reflection_tint`: parts through `sky_reflected`, water in
`water_surface`). It replaced a fitted guess that dimmed the sky by the
sun's height.

The drawn sky also takes `ExposureCompensation` in light units, as Studio's
shaders scale everything; scaling it before the light units made bright
sky channels jump past the curve's top, a saturated blue at exposure 0.5.
The water's sky is in light units as the reflection cube holds it (the
image's sRGB color squared), not through the curve's inverse.

Against Studio at 6.29, exposure 0.5, Brightness 0 (a pool beside a
wall): the sky lands within 9 levels at the horizon, and overhead within
4 in red and green but 22 too blue. The water was brighter than Studio's
(11, 26, 35 against 7, 12, 16) until see-through water let through what
Studio's does and only its nearest surface drew ([water](../terrain/water.md),
2026-11-05): black see-through water seen low over black ground now
lands within 2 levels far off and 10 to 30% above Studio's light near the
camera.

## A sky that won't load

Measured in Studio 0.745 on 2026-11-06, Baseplate, ClockTime 14. A Sky with
five good night images and `SkyboxFt` set to an id that isn't an image
(`rbxassetid://1`) drew the default sky on every side, including the sides
with good images. Setting that one image to a good id drew the Sky's night
images on every side, so a Sky is all or nothing. Rook draws the default
sky without a Sky, until all six of a Sky's images are there, and when one
won't load. Its own plain sky is left for having no default sky at all: no
Roblox packages or install.

A Sky with an empty image is not settled: one captured after the images had
loaded before showed no hole, which may be Studio's cache. Rook treats it as
an image that won't load (Inference). Studio's sky while images load isn't
measured; that it's the default sky follows from the failed case (Inference).

## Not done

- The night sky: the dark gradient, its purple horizon glow and the stars
  (M13 step 1). Rook draws black there.
- 18.0 to 18.3 wasn't sampled between its ends.
