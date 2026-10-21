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
`sun_color` in `crates/rook-view/src/lighting.rs`) adds the sun and
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
