# Explosions

ROADMAP M7 step 5. Measured in Studio 0.742 on 2026-10-19, in a play
session on a baseplate, by the Method below. The corpus summary counts
`Explosion` in 2,315 files; nearly all are made by scripts (rockets,
bombs, landmines), not saved in places.

## The blast

An Explosion goes off once, when it enters Workspace (Observed: `Hit`
fired about one frame, 0.016 s, after the script parented it; not
during the assignment). The server removes it 2.7 to 2.9 s later
(Observed at 0.1 s resolution, `Visible` on or off); Rook removes it after
2.75 s of its own time, so `TimeScale` 0 keeps it.

**What it hits** (Observed): every part whose surface comes within
`BlastRadius` of `Position`. A 1-stud cube centred 10.4 studs away
(surface 9.9) is hit by radius 10, one at 10.6 isn't, and a cube turned
45° at 10.6 is (its corner reaches 9.9). A part reaching in from far away
counts: a 1 × 1 × 10 part centred 12 studs away, its end 7 away, is hit.
A sphere wholly inside a part misses it: radius 0.5 and 0.8 at the centre
of a 2-stud cube fire nothing, 0.95 does (the nearest face is 0.9 away);
a 0.5-stud cube around the same centre is hit. Parts with `CanQuery` off
aren't hit; `CanTouch` off doesn't matter; anchored parts are hit; Terrain
fires nothing. `Hit` passes the part and its centre's distance.

**Joints** (Observed, `DestroyJointRadiusPercent` 0.5, radius 10): Weld,
ManualWeld, Snap, Glue, Motor and Motor6D break (their `Parent` becomes
nil) when either part is within the joint radius, a Weld from a part 4.5
studs out to one 6.5 out included. WeldConstraint, Rotate, RotateP,
RotateV, VelocityMotor and the constraints (Hinge, Rod, Rigid) stay. A
modern R15 character's AnimationConstraints don't break either.

**The push** (Measured, Workspace.Gravity 0, parts owned by the server):

- A part's impulse is `BlastPressure × |Size|² × 5.4825e-5`, straight
  away from `Position` towards its centre, whatever its distance, shape,
  density or orientation. At BlastPressure 5,000: 1-stud cube 0.8224;
  1 × 1 × 2 in any orientation 1.6447; 2 × 2 × 1 2.4671; 1 × 1 × 3
  3.0154; 1 × 1 × 4 4.9342; 2 × 3 × 4 in three orientations 7.9496;
  3 × 3 × 3 7.4013; 1 × 1 × 10 27.96. So a 1-stud Plastic cube (mass 0.7)
  leaves at 1.175 studs/s per 5,000 of pressure, 117.5 at the default
  500,000, and a cube of twice the density at half that. Impulses scale
  with the pressure (100: 0.02 studs/s).
- It also spins the part about that line by `impulse × |Size| / 4`
  (1-stud cube: ω 3.04 rad/s at pressure 5,000; 1 × 1 × 10 lengthwise and
  across: angular momentum 70.4 both ways).
- Anchored parts don't move; the push comes after the joints break, so a
  welded pair inside the joint radius flies apart.

**ForceField** (Observed on a character): with a ForceField in the
character, every part is still hit, but none moves and the Humanoid lives;
without one, the same explosion kills it (Health 0, state Dead).
Rook skips the push and joint breaking for parts under anything holding a
ForceField (Inference: the class's documented rule); killing Humanoids
waits for M9's health and death.

**Terrain**: `ExplosionType` Craters (the default) carves Terrain exactly
as `FillBall(Position, BlastRadius, Air)` does (Measured: after radius 8
on full Grass, voxels 6.63 studs out keep 0.16, 8.72 out 0.68, both
`(d - r) / 4 + 1/2`); NoCraters leaves it alone.

**Clients** (Observed): an Explosion a LocalScript makes fires `Hit` and
breaks joints on that client. Rook's clients don't yet (they can't tell
their own Explosions from the server's); they draw every Explosion.

## The look

The same whatever `BlastRadius` (Observed: radius 4 and 12 draw the same
fireball, 8 studs across, and the same 30-stud flash). In stages, at
TimeScale-stepped times (Observed, 30° field of view at 60 studs):

| Time | Studio |
|---|---|
| 0 | a yellow-white fireball 8 studs across in a white flash 28–30 studs across |
| 0.1 s | fireball 10 studs, the flash brighter and wider |
| 0.3 s | fireball 13–15 studs, dark red-brown with an orange rim; a hundred-odd pale specks out to 20 studs; faint pale rings sweeping out past 40 studs; the flash fading |
| 0.6 s | a dark brown cloud 18 studs across, specks out to 30 |
| 1 s | a black cloud 20–22 studs across, risen about 4 studs, red-brown underneath |
| 1.6–2.4 s | the cloud thins and rises, a pale haze under it |

The install's `textures/particles/` hold the images: a white glow
(`explosion01_implosion_main.dds`), a fireball puff, premultiplied as
Fire's is (`explosion01_core_main.dds`), a grey puff
(`explosion01_smoke_main.dds`, alpha at most 85 of 255, flat across its
middle and reaching the middle of each edge) and a ring
(`explosion01_shockwave_main.dds`).

## Rook's fit

The server's blast is `rook_physics::Physics::blast` and `push`
(`crates/rook-physics/src/explosion.rs`); the engine notices new
Explosions at the end of a frame and sets them off in the next step
(`crates/rook-engine/src/bridges/explosions.rs`), firing `Hit` after the
step. The look is five bursts on the particle emitters
(`apps/rook-player/src/scene/effects/explosion.rs`, Approximations fitted
to the stages above):

| | Flash | Fireball | Smoke | Specks | Ring |
|---|---|---|---|---|---|
| Particles | 1 | 10 | 14 | 160 | 1 |
| Lifetime (s) | 0.6 | 0.6–0.8 | 2.0–2.6 | 1.0–1.8 | 0.9 |
| Size (studs) | 36 → 44 | 7 → 16 | 6 → 14 → 20 | 0.8 × 0.6–1.4 | 10 → 150 |
| Motion | | 4 studs/s spread | 3 up, 7 spread, drag 1.2, rising | 40–90 out, drag 2.5, falling | |
| Colour | white × 3 | white, orange, brown; × 6 falling to 0.7 | dark red to black | pale × 1.5 | pale blue |
| Blend | adds | covers | covers | mostly covers | adds |

Particles now follow their emitter's key (instance and layer) rather than
its place in the scene's list, so Explosions coming and going leave other
effects' particles alone. The smoke image is faded to nothing at its
inscribed circle: as installed it drew faint arms out of a big cloud.

Rook against Studio, the same scene by `rook-player --screenshot` with
the Explosion's `TimeScale` set so the 6 s settle lands on 0.3 s and 1 s:
the stages, sizes and colours match by eye; Rook's 0.3 s fireball is
redder than Studio's brown, and Studio's 1 s cloud is redder underneath.

## Method

1. Physics: a play session on Baseplate, Workspace.Gravity 0, parts with
   CanCollide off and `SetNetworkOwner(nil)` (without it the server reads
   the client's velocities a few frames late), separate explosions 200
   studs apart, velocities read 5 Heartbeats later. Joints: pairs on a
   3-stud circle, each kind once.
2. Terrain: `FillBlock` Grass, an explosion at its surface, `ReadVoxels`
   across the crater.
3. Look: an Explosion made by the client with `TimeScale` 0, stepped by
   `TimeScale` 0.1 for timed seconds, `screen_capture` between. A
   client Explosion stepped this way restarts its look about every 3 s.
