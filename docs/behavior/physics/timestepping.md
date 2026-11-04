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

## Rook today

Every assembly steps at 240 Hz (`rook-physics/src/lib.rs`, `STEP_HZ`). The
design is in [architecture](../../design/architecture.md) (Physics,
*Adaptive timestepping*).
