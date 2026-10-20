# Fire, Smoke and Sparkles

ROADMAP M7 step 2. Measured in Studio 0.742 on 2026-10-18, in a fresh
Studio on a test place saved without `Lighting.Technology`, by the Method
below. Sparkles are in 303 archive files, Fire in 256, Smoke in 251.

## What Studio does

**All three are particles in the world** (Observed). Each effect under a
part (or an Attachment) emits from the part's centre; particles stay where
they were born when the part moves, so a moving Fire leaves a trail.
`Enabled = false` stops new particles and lets the living ones finish.
`TimeScale` scales their time: at 0 they freeze where they are (which is
how the measurements below were stepped).

**They go along the part's UpVector, not world up** (Observed): turned 90°
about Z, a Fire burns sideways; upside down, Smoke sinks and Sparkles
spray downward. Archive places rely on it: steam and fire-suppression
nozzles are Smoke on parts facing down, vents are Smoke on parts
facing along Z with `RiseVelocity` ±10.

**Their images** are the install's `textures/particles/` files (Observed:
the shapes on screen are these):

| Effect | Image | How it's drawn |
|---|---|---|
| Fire | `fire_main.dds`, wisps, premultiplied (its colour is already scaled by its alpha) | covers what's behind and glows |
| Smoke | `smoke_main.dds`, a soft puff, straight alpha | covers what's behind, lit |
| Sparkles | `sparkles_main.dds`: an eight-rayed star in alpha, a round glow in colour | adds light, sharp rays |

Each has a `*_color.dds` ramp beside it, 128 × 32: a colour and alpha over
a particle's life along x. Smoke's alpha rises to 0.63 a third of the way
through its life and fades out; Fire's fades in over the first eighth and
out over the last quarter, its colour falling to black by five eighths;
Sparkles' alpha is near 0 for the first fifth.

### Fire

- **Two layers** (Observed): flames in `Color` that cover what's behind
  (black `Color` draws black flames over a grey floor), and a smaller core
  and trail in `SecondaryColor` added on top (red flames with a blue
  secondary have a magenta core and a blue trail above).
- **Not lit**: the same at Ambient 0 and 255.
- **Glowing**: the default orange flames saturate to yellow-white (255,
  255, 139) in the core and draw (249, 156, 68) at the edges.
- **Heat is an upward acceleration** of about `Heat / 2` studs/s²
  (Inference: after switching off, the lowest particles of a Heat 9 fire
  rose 0.85 studs between 0.47 and 0.78 s and 1.35 between 0.78 and 1.1;
  both fit 4.5 studs/s²). Heat 0 makes a ball about 3 studs across around
  the part; Heat 25 a column 9 studs high with a trail to 15.
- **Size scales its width, not its height** (Observed, Heat 9): Size 2, 5
  and 10 are 1.1, 3.2 and 6.2 studs wide; their flames 3.2, 4 and 5 high.
- **Particles live about 1.2 s bright**, then fade as dark wisps; all are
  gone 1.5 s after switching off.

### Smoke

- **Lit** (Observed): invisible at Ambient 0, white at 255.
- **Rises at `RiseVelocity`** studs/s along the UpVector; RiseVelocity 10
  makes a plume about 40 studs high, so puffs live about 5 s.
- **Puffs grow** from about `Size` to some 15 studs across (Size 1 and 5
  make plumes of the same width at the top; Size 5 is wider at the base).
- The default (Size 1, RiseVelocity 1, Opacity 0.5) is a dome about 15
  studs wide and 11 high, cut where it meets the floor.

### Sparkles

- **Not lit** (Observed: the same at Ambient 0, 128 and 255).
- **Stars in `SparkleColor`**, 1 to 2 studs across, thrown up to 5 studs
  sideways and 8 up from the part, about 35 alive at a time; they live
  about 1.5 s.

## Rook's fit

Rook simulates particles on the CPU (`crates/rook-render/src/effects/particles.rs`)
from an `Emitter` per layer: its place and axes, `Enabled`, `TimeScale`
and an `EmitterLook` (rate, lifetime, spawn radius, velocity and
acceleration along the emitter's axes, a random spread, drag, size and
opacity curves over the life, colour times a brightness curve, how much it
adds light, how far it's lit). ParticleEmitter uses the same model
(docs/behavior/particle-emitters.md). The player maps the three effects
onto it
(`apps/rook-player/src/scene/effects/classic.rs`, Approximations fitted on the
comparison below):

| | Fire flames | Fire core | Smoke | Sparkles |
|---|---|---|---|---|
| Rate (/s) | 40 | 40 | 10 | 30 |
| Lifetime (s) | 1.0–1.4 | 1.3–1.9 | 4.5–5.5 | 1.0–1.6 |
| Up | Heat/2 studs/s² | 1.1 × that | RiseVelocity studs/s | 2 studs/s |
| Spread (studs/s) | Size/20 | Size/20 | 3, drag 0.4 | 8, drag 1 |
| Size (studs) | 0.4 → 0.15 × Size | 0.3 → 0.1 × Size | Size + 1 → Size + 14 | 1.6 × 0.4–1.4 |
| Colour | Color × 20 fading to 0 | SecondaryColor × 4 | Color, lit | SparkleColor × 1.8 |
| Opacity | in 0.1, out from 0.7 | in 0.15, out | the install's ramp × 3.6 × Opacity | ramp up to 0.85 at 0.45 |
| Blend | covers | adds | covers | adds |

Particles are drawn after everything else, sorted back to front, as quads
facing the camera, testing depth without writing it, with premultiplied
colour (`shaders/particles.wgsl`): colour × opacity added, what's behind
dimmed by opacity × (1 − light emission). Lit ones take the light a
surface facing the camera would (ambient, sun, local lights). The install's
images are converted on load to straight alpha: Fire's unpremultiplied,
Sparkles' colour made white so the star in its alpha shows. Without an
install Rook draws its own puff, wisps and star.

Scripts changing an effect or moving its part redraw it: an effect's
emitters are read again when its properties change, and only moved when
its part or Attachment moves. A screenshot runs the particles 6 seconds
before drawing, so effects look as they do once going.

## Rook against Studio

The same scene in Studio and `rook-player --screenshot`: a default Fire,
Smoke and Sparkles on invisible parts in front of a grey wall, Ambient 128
and 255, Brightness 0, seen from 80 studs at a 30° field of view. Mean
light over each effect's area above the background, Studio → Rook:

| Effect | Ambient 128 | Ambient 255 |
|---|---|---|
| Fire (red, green) | 20.6, 16.4 → 26.2, 17.7 | 17.2, 12.8 → 21.9, 14.0 |
| Smoke (grey) | 37.6 → 31.0 | 70.4 → 58.5 |
| Sparkles (red, blue) | 9.7, 12.8 → 7.9, 14.7 | 12.8, 15.9 → 6.8, 12.8 |

Smoke's lower half matches within about 10 levels; Studio's is up to 40
brighter in the middle of the dome. Studio's star centres are whiter
(its brightest sparkle pixels are (249, 110, 255)); Rook's stay purple.
Turned parts send all three the same way as in Studio.

Cost: no measurable change on an archive place with 16 effects on and 96
off (300 frames, whole frame 11.45 → 11.03 ms, GPU main pass 0.37 → 0.39
ms).

## Method

1. A fresh Studio on an rbxlx with only Workspace and Lighting; effects on
   1-stud invisible anchored parts, red Neon ruler cubes every 4 studs at
   the effects' depth for the scale.
2. Run the effect a few seconds, then set `TimeScale = 0` to freeze it,
   and `screen_capture` at a 20° or 30° field of view. To follow particles
   over time: switch `Enabled` off, then step with `TimeScale = 1` for a
   timed few tenths of a second and back to 0 between captures.
3. Read pixels with System.Drawing (mean light over each effect's area
   above the background, and 5 × 5 averages on a grid).

A freshly created effect needs a moment before its first particles show,
and an emitter switched on for a single frame emits nothing visible.
