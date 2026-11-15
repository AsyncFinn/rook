# Viewport frames

Checked against Studio 0.747 on 2026-11-14.

[M8](../../roadmap/m8-gui.md) step 8: what a ViewportFrame draws, how it
lights it, and how the picture lands on the screen.

Measured in Studio 0.747 on 2026-11-14 on Baseplate, edit mode and a
playtest. Captures leave ViewportFrames out, so the pixels were read from
`PrintWindow` captures of Studio's own window, with System.Drawing. The
lighting was read off a white SmoothPlastic face filling small frames at a
field of view of 1 or 2 degrees (one color a frame), under one
`Ambient`, `LightColor` and `LightDirection` each, about 150 frames in all;
the rest from frames holding a few parts, read pixel by pixel.

Code: the frame's quad in `rook-gui`'s `draw/objects/viewport.rs`; its scene in
`rook-scene`'s `viewports.rs`; its world, target and pass in
`rook-render`'s `renderer/passes/viewports.rs`, laid over the GUI by
`shaders/gui/gui_paint.wgsl`.

## The picture

- The frame draws its own world into a picture of its size in pixels, one
  pixel each (observed: no blurring at 300 × 200, edges a pixel sharp),
  with no anti-aliasing (observed: no blended pixels on any edge).
- The picture holds the background: it starts as `BackgroundColor3` at
  `BackgroundTransparency`, and the parts are drawn over it. The whole
  picture is then multiplied by `ImageColor3` and faded by
  `ImageTransparency` (observed: a green half-transparent background under
  a pink tint and `ImageTransparency` 0.3 drew 26, 61, 26 over 40 gray,
  which that order gives exactly; `ImageTransparency` 1 hides the
  background too). The border is drawn outside the picture as a Frame's
  is, untinted (observed with a red tint and a white 6-pixel border).
- Inside the picture, a see-through part blends over the background with
  straight alpha, and the picture's opacity adds up as layers do over one
  another: a white part at Transparency 0.5 over a fully transparent navy
  background drew halfway between white and navy, at half opacity
  (measured: 78, 78, 110 over 40 gray). Studio's
  `FFlagPremultiplyViewportframeBackgroundColor` is off, which matches.
- UICorner cuts the picture, `Rotation` turns it, and a clipping ancestor
  clips it, as for an image (observed).
- Without `CurrentCamera` only the background draws (observed). The
  camera can be anywhere, in the frame or in Workspace (observed).
  `CurrentCamera` isn't saved, so a frame copied from StarterGui into
  PlayerGui has none until a script sets it (observed: none in a playtest
  of a saved place, and the Camera under it left behind).
- Studio's view: the camera's `CFrame`, its vertical `FieldOfView`, and
  the frame's width over height (measured: a 4-stud face 8 studs away at
  70 degrees in a 200-pixel-tall frame was 72 pixels wide, 71.4 by that
  rule). Nothing past about 150,000 studs draws (observed: a part at
  99,000 drew, one at 150,000 didn't).

## Light

Measured on white SmoothPlastic, with Ambient 0 to 255, LightColor 0 to
255, the light 0 to 180 degrees from the face, the camera 0 to 75 degrees
from it, and gray parts; colored lights and parts per channel.

- Lit as the world's surfaces are, in light units (a color squared), but
  with only the frame's own lights: `Ambient` (its color squared) on
  every face, and `LightColor` (its color squared, with no Brightness)
  coming from `-LightDirection`, times the cosine, with the surface's
  highlight. White SmoothPlastic under Ambient 100 and LightColor 100 at
  60 degrees drew 124 (sqrt(100² + 100² × 0.5) is 122.5, the fog adding
  the rest); square to a light of 100 seen from in front, 114 (113.5 with
  the highlight of roughness 0.4). Doubling LightColor doubles the pixel
  at every angle.
- `LightDirection` is normalized (observed: lengths 0.2, 1 and 5 lit
  alike). A zero vector isn't taken: the property keeps its value
  (observed: setting `Vector3.zero` read back -1, -1, -1). Colors past 1
  light past white (observed: LightColor 1.2 drew white where 80/255
  drew 92).
- Nothing of `Lighting` reaches it (observed: Ambient, OutdoorAmbient,
  Brightness, ExposureCompensation, ClockTime, the environment scales
  and the fog changed nothing). No sky: `Reflectance` draws nothing, and
  metals (Foil) are lit as Plastic is, as with EnvironmentSpecularScale 0
  in the world (observed: Foil under Ambient 150 drew 150; Reflectance 1
  with no light drew the fog's black).
- Neon is unlit, at Rook's Neon glow (measured: 50 and 100 gray Neon drew
  36 and 144, 13.5 times the fourth power of the color in light units).
- Its own fog, which nothing changes: from 0 to 100,000 studs, of 0.75
  light units, by the world's fog curve (measured on black parts at 10 to
  99,000 studs: 2, 8, 14, 26, 36, 58, 80, 112, 168, 220; the fog's color
  fits 192/255 not squared within the pixels' steps). It's what keeps an
  unlit face from black: 8 at 100 studs.
- No tone curve: the picture holds light as Studio's frame does
  (sqrt(units / 4) in 8 bits) and shows it doubled (observed: every pixel
  came out even; Ambient 80 drew 80, where the world's curve draws it as
  66). Its light is multiplied by 1.019 first (measured on a face 2 to
  400 studs away: Ambient 40 to 104 drew itself, 106 drew 108, 120 to 250
  drew the value plus 2; LightColor 150 at 45 degrees drew 128, 126
  without it), 1.0095 on the 8-bit value.
- Rook's picture of the test frames (cubes with Decals, a tinted frame, a
  wall under Ambient and LightColor alone and together) matched Studio's
  playtest within one 8-bit step, mean 0.4 per channel; the steps fall on
  either side of a rounding at a few edges and in the highlight's rings.

## What draws

- Parts, meshes and Decals and Textures, also on a Transparency 1 part
  (observed).
- Not drawn (observed): Highlights, ParticleEmitters, Fire, PointLights
  (no light from them either), SurfaceGuis and BillboardGuis on its parts.
- On a SurfaceGui a ViewportFrame draws only its border, in edit mode and
  in a playtest (observed in Studio 0.747 on this machine).
