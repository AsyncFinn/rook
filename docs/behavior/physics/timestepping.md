# Timestepping

Checked against Studio 0.747 on 2026-11-13.

How often Roblox steps each moving assembly. Rook doesn't match yet: it
steps every assembly at 240 Hz (see *Rook today* below).

## Documented

- `Workspace.PhysicsSteppingMethod`: `Default` (0) resolves to `Adaptive`
  (2), the default since 2023; `Fixed` (1) steps every assembly at 240 Hz.
  Scripts can't read it (not a valid member of Workspace from a script), so
  read it from the place file.
- Adaptive puts each assembly in a 240, 120 or 60 Hz bucket. 240 Hz goes to
  assemblies with high velocity or acceleration and to complex mechanisms;
  the exact rules aren't published and may change.
- A slower assembly touching a 240 Hz one is stepped at 240 Hz.
- Studio's "Are Timesteps Shown" setting outlines parts by their rate, and
  the MicroProfiler's `LDLPGSSolver::solve` lists the islands in each bucket
  (1dt, 2dt, 4dt).

## Observed

Server, network owner nil, parts falling in open air, `Default` stepping.

**Speed against size.** The bucket follows speed against the assembly's
extent along its direction of motion (the width of its box seen along the
velocity), not speed alone. A 4 × 1 × 1 bar moving lengthwise steps like a
4-stud cube, and moving sideways like a 1-stud cube; a 4 × 4 × 1 plate
moving edge-on steps like the sideways bar. Cubes moving sideways:

| Size (studs) | 60 to 120 Hz at (studs/s) | 120 to 240 Hz at (studs/s) |
|---|---|---|
| 0.1 | 21 | 61 |
| 0.2 | 23 | 69 |
| 0.5 | 28 | 96 |
| 1 | 43 | 143 |
| 2 | 71 | 227 |
| 4 | 131 | 405 |

Both columns lie near straight lines in the extent e: about 29·e + 14 for
120 Hz and 87·e + 56 for 240 Hz (a fit, not a published rule; the smallest
sizes sit a few studs/s above it). An earlier run on 0.745 put a 2-stud
block at 240 Hz above about 120 studs/s; this sweep, with the extent
measured along the motion, replaces it.

**Changing buckets.** The bucket is chosen again while an assembly flies.
A 1-stud cube thrown straight up at 200 studs/s left 240 Hz at 141 and
120 Hz at 41 as it slowed, where constant-speed cubes change. Falling back,
it reached 120 Hz only at 73 and 240 Hz at 160: speeding up, it moves up
later than slowing down moves it down. Why (a delay, or a rule on
acceleration) isn't known.

**Spin.** A 2-stud cube spinning about its vertical axis with no linear
speed stays at 60 Hz up to 60 rad/s and steps at 240 Hz at 120 rad/s
(skipping 120 Hz).

**Mechanisms.** Two 2-stud cubes from rest joined by a HingeConstraint,
BallSocketConstraint, RopeConstraint or WeldConstraint step at 60 Hz like a
lone cube. Joined by a SpringConstraint (default properties) both step at
120 Hz from rest.

**Characters.** A player's R15 character falling from rest steps at 240 Hz
from its first frame and at every speed, on its client.

**Contacts.** An assembly finds contacts only at its own steps. Fired at an
anchored wall with gravity off, from 20 start points each:

| Cube, wall (studs) | Speed (studs/s) | Bucket | Passed | Per 240 Hz tick | Per own step |
|---|---|---|---|---|---|
| 1, 1 | 300 | 240 Hz | 5/20 | about 4/20 | about 4/20 |
| 0.2, 0.2 | 50 | 120 Hz | 12/20 | about 1/20 | about 10/20 |

The last two columns are what each rule predicts: a part passes when its
center is past the wall's middle at the first step it overlaps, or when no
step overlaps at all. A 1-stud cube at 300 studs/s once a frame would pass
about 16 of 20.

- Scripts still see 4 steps' worth of gravity per 60 Hz frame at every
  rate (the speed read in `PostSimulation` grows by g/60). Only the size
  of the steps changes.

**Method.** Each rate leaves its own trace in one frame's fall. Over k
semi-implicit steps of length h (k·h = 1/60 s), a frame's drop is its end
speed / 60 less g·h²·k(k−1)/2: 0 at 60 Hz, 0.0136 studs at 120 Hz and
0.0204 studs at 240 Hz, with gravity 196.2. Measured frames landed on these
to 0.0001 studs. Frames with 3 or 5 steps' worth of gravity (uneven frame
lengths) are skipped. The size sweep read every frame of parts launched at
speeds 12% apart; the thresholds are where neighbors differ.

**Not measured yet:** the spin threshold between 60 and 120 rad/s and how
it scales with size, the acceleration rule behind the late move up, what
in a SpringConstraint promotes it, a character's bucket while walking on
the ground, and which of the four 240 Hz ticks in a frame a 60 Hz assembly
steps on.

## Long frames

Checked against Studio on 2026-11-04. A frame that takes longer than
usual doesn't make the next frame simulate all the time it lost: Roblox
simulates a frame's time only up to a cap and drops the rest.

| Side | Frame length | Simulated in the next step | Cap |
|---|---|---|---|
| Server | 0.30 s, 1.00 s | 1/15 s each (16 steps of 240 Hz) | 1/15 s |
| Client | 0.35 s, 1.05 s | 0.117 s, 0.121 s (28 and 29 steps) | about 0.12 s |

**Method.** A busy loop in a `Heartbeat` (server) or `RenderStepped`
(client) handler held one frame for the length shown, while a block
(server, network owner nil) or the local character (client, teleported
3,000 studs up) fell freely. The speed gained across the long frame,
divided by gravity (196.2), is the time simulated. Ordinary frames gained
their own length's worth, so the cap only clips long ones. The client test
ran at about 15 fps (an unfocused Studio), and its later frames of 0.118 s
were simulated whole, so its cap lies between 0.118 and 0.121 s.

## Rook today

Every assembly steps at 240 Hz (`rook-physics/src/lib.rs`, `STEP_HZ`), as
`Fixed` does. A frame's time is capped at 0.1 s (24 steps; the player's
`App::update` and the bench), near the client's measured 0.12 s, so a long
frame costs the next one at most 24 steps. Adaptive is deferred: see [architecture](../../design/architecture.md)
(Physics, *Adaptive timestepping*).
