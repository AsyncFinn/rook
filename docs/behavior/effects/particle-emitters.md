# Particle emitters

Checked against Studio 0.743 on 2026-10-21.

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

Not done yet: `VelocityInheritance` (no part velocity reaches the
renderer), `WindAffectsDrag`, `FlipbookBlendFrames` (frames swap without a
crossfade), `ShapePartial` for Cylinders, and `LocalTransparencyModifier`.
While its texture loads, an emitter draws nothing.

Cost: on an archive place with 200 ParticleEmitters, 12 Beams and 8
Trails (none in view), two alternating `--bench 300` pairs against the
build before: CPU recording 1.26 → 1.38 to 1.47 ms a frame (the
particles simulating), whole frame and GPU passes within their noise.

## Method

A fresh-enough Baseplate Studio: particles from `Emit` on 1-stud invisible
anchored parts, `TimeScale = 0` after half a second to freeze them, then
`screen_capture` from the side and from above. A white square texture
(`rbxasset://textures/blackBkg_square.png`) for sizes. Rate-driven
emission doesn't advance while Studio's window is unfocused; `Emit`
places particles even then.
