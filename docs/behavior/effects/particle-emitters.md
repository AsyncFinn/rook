# Particle emitters

Checked against Studio 0.745 on 2026-11-04.

[M7](../../roadmap/m7-lights-and-effects.md) step 4. ParticleEmitter is in 86 archive files. Sources: the
creator-docs reference (ParticleEmitter, its enums, the "Particle
emitters" guide), and measurements in Studio 0.743 on 2026-10-19 by the
Method below.

## What Studio does

**Where particles are born** (creator-docs): under a part, anywhere in its
`Shape` sized to the part (Box, the default, is its bounding box); under
an Attachment, at the Attachment. Particles live in the world: one that's
born stays put when its part moves, unless `LockedToPart`.

**Which way they go** (creator-docs, and Observed for the shapes):

| Shape | Born | Goes |
|---|---|---|
| Box | in the box (Volume) or on its faces (Surface) | along `EmissionDirection` |
| Sphere | in the ellipsoid, or on it | out from its center |
| Cylinder | in a cylinder whose axis lies across `EmissionDirection` | out from the axis |
| Disc | on the face `EmissionDirection` points to | along `EmissionDirection` |

The Cylinder's axis is the part's X axis for EmissionDirection Top and its
Z axis for Right (Observed): the right of a view looking along the
direction. `ShapeInOut` Inward sends them the other way, InAndOut either
way at random. `ShapePartial` keeps a cap of a Sphere around the
direction's pole (1 the whole sphere, 0.5 half) and is a Disc's inner
radius as a share of its outer (creator-docs).

**Speed** is random between `Speed`'s two, turned at random by up to
`SpreadAngle` degrees about the two axes across the direction
(Inference: up to ± each angle; 360 on one axis makes a circle, on both a
sphere, as creator-docs says). `Acceleration` is in world axes; `Drag`
halves the speed every 1 / Drag seconds (creator-docs).

**A particle's square is 2 × Size across** (Observed: Size 1 drew 1.98
studs beside a 1-stud ruler cube, Size 4 drew 8). **Squash** scales its
width by 2^−Squash and its height by 2^Squash (Observed: at Size 4,
Squash 0 drew 151 × 151 px, Squash 1 75 × 302 and Squash −1 302 × 75).

**Size and Transparency** are NumberSequences over each particle's life;
a keypoint's envelope moves it by a random amount chosen per keypoint per
particle (creator-docs). **Color** is a ColorSequence over its life.
`Rotation` and `RotSpeed` are random per particle, positive clockwise.

**Flipbooks** (creator-docs): the texture is a grid of frames
(`FlipbookLayout` 2 × 2, 4 × 4, 8 × 8 or `FlipbookSizeX` × `FlipbookSizeY`),
played at `FlipbookFramerate` (at most 30) as Loop, PingPong, Random, or
OneShot (once over the particle's life, whatever the framerate);
`FlipbookStartRandom` starts each at a random frame.

**Orientation**: FacingCamera (the default), FacingCameraWorldUp (turning
about world up only), VelocityParallel (lengthwise along its velocity),
VelocityPerpendicular (square on to it). `ZOffset` moves particles towards
the camera without changing their size on screen.

**Light**: `LightEmission` 0 covers what's behind, 1 adds to it;
`LightInfluence` 0 draws in its own color times `Brightness`, 1 lit by
the scene (creator-docs: 0 by default from `Instance.new`, 1 when inserted
in Studio).

**Scripts**: `Emit(count)` births `count` particles at once (16 when not
given), even while disabled; `Clear()` removes all of them. A server
script's `Emit` shows on clients. Destroying the emitter or its part
removes its particles at once, however long they had left (Studio
2026-10-21: 60 s particles gone within 1 s; untouched control stayed).

**Cost** (Studio 0.745, 2026-11-04, `Stats.RenderCPUFrameTime` averaged
over 2 s in edit mode, 50 emitters 60 studs ahead with 5 s lifetimes):

| Particles alive | Render CPU |
|---|---|
| none | 5.8 ms |
| about 10k | 8.8 ms |
| about 25k | 11.2 ms |
| about 50k | 11.7 ms |
| about 100k | 12.5 ms |

- Particles cost the render thread's CPU, growing with their number and
  leveling off past about 25k (the 16,000 draw budget below).
- The same 100k behind the camera cost nothing (5.6 ms, the same as none).
- They still exist there: emitting only while the camera looked away, then
  turning back, showed the full cloud's cost at once (12.6 ms), as did
  turning away for 1 s or 4 s and back: emitters out of view are paused,
  below.

**What's drawn, what runs** (Studio 0.745, 2026-11-04, edit mode, camera
pinned per frame; a grid of 100 emitters of `Emit`ted particles seen from
above, single particles measured in nearly orthographic captures):

- *Draw budget*: a frame draws at most 16,000 particles. Emitters in view
  are taken nearest the camera first, each whole; one that would pass the
  budget is left out and farther ones still fill it (100 × 161 drew 99
  emitters; 100 × 300 drew 53; three 5,000s then ten 100s past a fourth
  5,000). The same at edit quality level 1. Emitters left out by the
  budget keep running (a Rate emitter behind it kept emitting).
- *Out of view* is the frustum on where the particles are, not where the
  emitter is (an off-screen emitter spraying into view draws), and not
  occlusion (emitters behind a wall still take budget). An emitter out
  of view takes no budget and is **paused**: its particles don't age,
  move or die, and Rate emission stops. Emitted, looked away 1.5 s, then
  back 1.0 s: the particles showed 0.79 s of motion.
- *Distance*: nothing stops drawing (single `Emit` particles drew at
  3,000 studs, no fade), but `Rate` thins with the emitter's distance
  from the camera: 1.0 of it at 150 studs, 0.82 at 300, 0.59 at 500,
  about 0.37 at 700, 0.12 at 900, none by 1,000; a straight fall from
  150 to 1,000 fits all to 0.02.
- *Motion is stepped per frame*, not worked out from age: each frame
  moves the position by `v·dt + ½a·dt²`, then sets `v = (v + a·dt) ·
  2^(−Drag·dt)`. At 15 fps (dt 0.068, age 1.357 s, Speed 10 / Acceleration
  10 along X): Speed alone 13.571 studs; Acceleration alone 9.197 (½at²
  9.209); Speed with Drag 1 9.009 (this step 9.004, the exact curve
  8.795); Acceleration with Drag 1 6.898 (this step 6.896). A frame's
  step is at most 1/15 s (frames of 0.1 to 0.18 s moved them 1/15 each).
- A frozen emitter (`TimeScale` 0) set back to 1 didn't move again in
  edit mode; fresh emitters were used for every run.

## Rook's fit

The player reads each ParticleEmitter into the renderer's `EmitterLook`
(`crates/rook-view/src/effects/emitter.rs`), the same model Fire,
Smoke and Sparkles use (docs/behavior/effects/fire-smoke-and-sparkles.md); the renderer simulates
it on the CPU (`crates/rook-render/src/effects/particles.rs`, shapes in
`effects/particles/spawn.rs`) and draws each particle as a rectangle of its image
(`shaders/particles.wgsl`): the flipbook frame's part of it, squashed,
turned to its orientation, mixed between its own color and the lit one by
LightInfluence.

`Emit`, `Clear` and `Beam:SetTextureOffset` are noted on the World
(`EffectCall`) and reach the renderer with the render changes; a server's
go to each client as a message, and calls on an emitter that hasn't
replicated yet wait for the next updates. The player keeps a count of
each per effect, and the renderer births (or clears) on the change.

The renderer runs particles as Studio does (measured above): only emitters
whose particles or birthplace are in the view frustum move on, once a
frame at draw time by the frame's time (at most 1/15 s) with Studio's
step; the rest keep their particles exactly as they were until the camera
turns back. `Rate` thins with distance (all of it within 150 studs, none
by 1,000), and a frame draws whole emitters nearest first up to 16,000
particles. A locked emitter's particles are kept along its axes. Fire,
Smoke, Sparkles and explosions go through the same particles (Inference:
Studio's are particle emitters too; not measured separately).

Not done yet: `VelocityInheritance` (no part velocity reaches the
renderer), `WindAffectsDrag`, `FlipbookBlendFrames` (frames swap without a
crossfade), `ShapePartial` for Cylinders, and `LocalTransparencyModifier`.
While its texture loads, an emitter draws nothing.

Cost, `--bench 600` against the build before, medians of the particles'
stage: on a place with about 1,000 particles, all in view of the still
bench camera, 0.41 → 0.46 ms (the same work); turning the camera, where
most emitters spend most frames out of view, 0.93 → 0.48 ms; on a place
with 115 emitters and no live particles, 0.03 → 0.07 ms (each emitter's
box tested against the view).

## Method

A fresh-enough Baseplate Studio: particles from `Emit` on 1-stud invisible
anchored parts, `TimeScale = 0` after half a second to freeze them, then
`screen_capture` from the side and from above. A white square texture
(`rbxasset://textures/blackBkg_square.png`) for sizes. Rate-driven
emission doesn't advance while Studio's window is unfocused; `Emit`
places particles even then.
