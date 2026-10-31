# Sleeping and resting contact

Checked against Studio 0.745 on 2026-10-30.

When Roblox stops simulating an assembly that holds still, and how deep a
resting part sits in what holds it up. Rook's world doesn't match either
yet (see *Rook today* below).

Observed in Studio (server, network owner nil, frictionless 2-stud blocks
on an anchored floor) unless marked otherwise:

- **Linear**: a block sliding at 0.32 studs/s or less stops dead about
  0.51 s after it was set moving: it moved 0.513 × its speed and then read
  zero `AssemblyLinearVelocity`. At 0.34 studs/s or more it slid on at
  full speed for 8 s. So an assembly sleeps once it has stayed under
  about 0.33 studs/s for half a second, and sleeping zeroes its velocity.
- **Angular**: a block spinning about Y on the same floor slept at 0.2
  rad/s and not at 0.3 for a 2 × 2 × 2 block; at 0.08 and not 0.1 for an
  8 × 2 × 8 one. The bound shrinks as the block grows, so Roblox judges
  rotation by how fast the block's points move, not by the angular speed
  alone. The radius it uses isn't pinned down: half the widest side gives
  0.2 to 0.3 studs/s and 0.32 to 0.4, the half-diagonal 0.28 to 0.42 and
  0.45 to 0.57. Spinning blocks also slowed down on a frictionless floor
  (0.3 rad/s fell to 0.26 in 3 s).
- **Rest depth**: a block resting on the floor sits 0.0176 studs into it
  (2-stud cube) or 0.0189 (8 × 2 × 8), whatever its density (0.01 to 100).
  In a stack of five 2-stud cubes each block sat about 0.05 studs lower
  than the one below it would hold it ideally (the top contact 0.02).

## Rook today

Rapier puts a body to sleep when its farthest point moved less than twice
`normalized_linear_threshold × length_unit` per second for
`time_until_sleep` (Rapier 0.36's pose-drift rule, solver position
corrections included), and pushes contacts apart once they're deeper than
`normalized_allowed_linear_error × length_unit`. Rook leaves
`length_unit` at 1 (one stud read as one meter), so a body must hold
under 0.1 studs/s for 0.5 s, and resting contacts are pushed out past
0.005 studs.

Rapier's own guidance is to set `length_unit` to the world's units per
meter. Roblox documents a stud as 0.28 m, and 1 / 0.28 = 3.57 gives
0.0179 studs of allowed depth (Roblox rests at 0.0176) and a sleep speed
of 0.36 studs/s (Roblox's is about 0.33): the scale Roblox's numbers
point at. `length_unit` also scales Rapier's contact recycling distance,
and at 3.57 a resting train with free-turning wheels slid off its rails
on reused contacts; keeping that distance at today's 0.05 studs kept it
on (Inference, from one place's train).
