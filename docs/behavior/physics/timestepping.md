# Timestepping

Checked against Studio 0.745 on 2026-11-03.

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

Server, network owner nil, a single 2-stud block falling in open air with
a sideways starting speed, `Default` stepping:

| Speed (studs/s) | Rate |
|---|---|
| under about 70 | 60 Hz |
| about 70 to 115 | 120 Hz |
| above about 120 | 240 Hz |

- A block dropped from rest starts at 60 Hz and moves to 120 Hz as its fall
  speeds up: the bucket is chosen again while it flies, not once.
- Scripts still see 4 steps' worth of gravity per 60 Hz frame at every
  rate (the speed read in `PostSimulation` grows by g/60). Only the size
  of the steps changes.

**Method.** Each rate leaves its own trace in one frame's fall. Over k
semi-implicit steps of length h (k·h = 1/60 s), a frame's drop is its end
speed / 60 less g·h²·k(k−1)/2: 0 at 60 Hz, 0.0136 studs at 120 Hz and
0.0204 studs at 240 Hz, with gravity 196.2. Measured frames landed on these
to 0.0001 studs. Frames with 3 or 5 steps' worth of gravity (uneven frame
lengths) are skipped.

**Not measured yet:** the acceleration and angular speed thresholds, what
counts as a complex mechanism, whether a bucket drops back with the same
thresholds (hysteresis), the contact promotion rule, and which of the four
240 Hz ticks in a frame a 60 Hz assembly steps on.

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
