# Sleeping and resting contact

Checked against Studio 0.745 on 2026-11-05.

When Roblox stops simulating an assembly that holds still, and how deep a
resting part sits in what holds it up. Rook sleeps assemblies as Roblox
does; rest depth doesn't match yet (see *Rook today* below).

Observed in Studio (server, network owner nil, frictionless 2-stud blocks
on an anchored floor) unless marked otherwise:

- **Linear**: a block sliding at 0.32 studs/s or less stops dead about
  0.51 s after it was set moving: it moved 0.513 × its speed and then read
  zero `AssemblyLinearVelocity`. At 0.34 studs/s or more it slid on at
  full speed for 8 s. So an assembly sleeps once it has stayed under
  about 0.33 studs/s for half a second, and sleeping zeroes its velocity.
- **Acceleration**: with gravity off, a block drifting slowly and pushed
  by a VectorForce slept at 0.24 studs/s² or less and never at 0.25 or
  more (0.25 to 2.0 tried, for up to 6 s, though its speed stayed under
  0.33 for seconds as it slowed, stopped and came back). Free-floating
  blocks with no force slept at 0.20, 0.30 and 0.32 studs/s like sliding
  ones. A BodyPosition (P 10,000, D 1,250) pulling a 5.6-mass block up to
  its goal slept 2.6 s in, about a second after its speed fell under
  0.33, 0.0099 studs short of the goal.
- **When the half-second starts**: blocks set moving at 0.4 studs/s and
  slowed at 0.18 to 0.24 studs/s² all slept 0.53 s after they were set
  moving, already moving under 0.31 then, though they were still over
  0.33 for the first 0.1 to 0.4 s. Slowed at 0.05, 0.1 and 0.15 from 0.5,
  0.6 and 0.4 they slept 0.27, 0.16 and 0.13 s after falling under 0.33,
  at 0.31 to 0.32. So the speed the half-second is timed against isn't
  simply the current one; what Roblox times it against isn't pinned down.
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
corrections included). Rook sets the threshold to half of 0.33 studs/s and
the time to 0.5 s on every body, so a block sleeps under the speed
Roblox's does, and judges spin by the body's bounding sphere, a little
more strictly than Studio. Rapier has no acceleration rule, so each step
Rook restarts the sleep timer of a body whose farthest point sped up or
slowed down by 0.25 studs/s² or more over it. Rook starts the half-second
once the body is under 0.33 studs/s, so a body slowing through that speed
can sleep up to about 0.3 s later than in Studio (see *When the
half-second starts*). The BodyPosition block above comes to rest at
1004.969 in Rook against Studio's 1004.990; whether sleep or the
BodyPosition's own pull makes the 0.02 isn't pinned down.

Rapier pushes resting contacts apart once they're deeper than
`normalized_allowed_linear_error × length_unit`. Rook leaves
`length_unit` at 1 (one stud read as one meter), so resting contacts are
pushed out past 0.005 studs, where Roblox's rest 0.0176 deep.

Rapier's own guidance is to set `length_unit` to the world's units per
meter. Roblox documents a stud as 0.28 m, and 1 / 0.28 = 3.57 gives
0.0179 studs of allowed depth (Roblox rests at 0.0176) and a sleep speed
of 0.36 studs/s (Roblox's is about 0.33, which Rook sets directly): the
scale Roblox's numbers point at. `length_unit` also scales Rapier's contact
recycling distance, and at 3.57 a resting train with free-turning wheels
slid off its rails on reused contacts; keeping that distance at today's
0.05 studs kept it on (Inference, from one place's train).
