# M7 Lights and effects

Lights are in hundreds of files and in great numbers: PointLight 228
files (35,041 instances), SpotLight 212 (50,397), SurfaceLight 65
(7,949). Effects: Sparkles 303 files, Fire 256, Smoke 251,
ParticleEmitter 86, Trail 17, Beam 15, plus the `Explosion`s scripts
create.

1. ✅ Local lights: Brightness, Range, Color, Shadows, SpotLight Angle and
   Face, SurfaceLight; many lights per frame
   ([local lights](../behavior/lighting/local-lights.md),
   [ambient](../behavior/lighting/ambient.md)).
2. ✅ Fire, Smoke and Sparkles, drawn as Studio draws them today, as
   particles a ParticleEmitter can reuse
   ([`docs/behavior/effects/fire-smoke-and-sparkles.md`](../behavior/effects/fire-smoke-and-sparkles.md)).
3. ✅ An HDR target with Studio's tone curves, ColorGrading's presets
   included: the sun, ambient, sky, local lights and Neon add up in
   Studio's light units and go through one curve at the end, so Decals
   blend over Neon as in Studio and sunlit faces stop drawing too bright
   ([`docs/behavior/lighting/tone-mapping.md`](../behavior/lighting/tone-mapping.md)).
   Before the remaining effects, which blend in HDR too.
4. ✅ ParticleEmitter (with flipbooks), Beam and Trail
   ([`docs/behavior/effects/particle-emitters.md`](../behavior/effects/particle-emitters.md),
   [`docs/behavior/effects/beams-and-trails.md`](../behavior/effects/beams-and-trails.md)).
5. ✅ Explosion: its blast (parts hit, joints broken, the push, craters)
   and its look ([`docs/behavior/effects/explosions.md`](../behavior/effects/explosions.md)).
6. ✅ Highlight and the adornments, by files saving them (and creating them
   with `Instance.new`) in the corpus scan: SelectionBox 676 (749),
   ImageHandleAdornment 57 (220), SelectionPartLasso 50 (362),
   SelectionSphere 45 (15), Handles 37 (264), FloorWire 33 (51),
   LineHandleAdornment 26 (49), SurfaceSelection 12 (89), the Box, Sphere,
   Cylinder and Cone HandleAdornments 6 to 13 each (34 to 82), ArcHandles
   11 (68), Highlight 68 (42): outlines, fills through walls, and the
   handles' look (dragging them comes with GUI input)
   ([`docs/behavior/rendering/adornments.md`](../behavior/rendering/adornments.md),
   [`docs/behavior/rendering/highlight.md`](../behavior/rendering/highlight.md)).

7. **Match Studio**: close every difference left in what steps 1 to 6
   built.
   - *Scope*: SurfaceLight's spread (fitted to one face size, within 17
     levels); the light voxels' shadow fit; the grid catching up over a
     few seconds as Studio's does; local lights near the camera with
     `PrioritizeLightingQuality` on; the Approximations in the effects,
     adornments and Highlight pages. Out: the sun, sky and sun shadows
     (M13).
   - *Evidence*: the [stubs](../guide/stubs.md) rows taken over by this
     step, and the open Approximations and Inferences
     on the lighting, effects and adornment pages.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 6, and no stubs row names this step.

Steps 1 to 6 met the old exit criterion; step 7 holds them to Studio.
