# Humanoid movement and states

Checked against Studio 0.745 on 2026-11-06, the legs and landing on
2026-11-12.

How a Humanoid stands, walks, jumps, climbs and swims, and its state
machine: `rook_physics::humanoid` (the controller) and `rook_lua`'s
`characters::movement` (the Humanoid members, and what the controller's
reports change and fire).

Measured in a Baseplate playtest on the client, which simulates its
character ([ownership](ownership.md)), with the PlayerModule's controls
turned off and `Humanoid:Move` and `Jump` set from a render-step binding
after them (with the controls off, they still clear `MoveDirection` and
`Jump` every frame). Positions and velocities were logged every
Heartbeat. The playtest body (the signed-in user's avatar, not the
[default](rigs.md#r15)) has `HipHeight` 2.0075, `WalkSpeed` 16,
`JumpHeight` 7.2 with `UseJumpPower` off, `MaxSlopeAngle` 89, under
Workspace gravity 196.2. R6 steps were measured on a body from
`Players:CreateHumanoidModelFromDescription`.

## Standing

- The root part floats: R15 stands with its center `HipHeight` plus half
  its height above the floor (3.0075), R6 with its legs' height, half its
  own and `HipHeight` (0): 3. Only the root, the torso parts and the head
  collide; legs and arms don't.
- The floor is felt under the root part's footprint: walking off a ledge,
  it held until the footprint had fully left it.
- Steps up to the root part's bottom (2 studs, on R15 and R6 alike) are
  walked up while Running, rising over about 0.25 s as the footprint
  moves onto them; a 2.5-stud step stops the torso.
- Above standing height the legs don't hold the root up. Gravity takes
  it, and the legs pull it down at least as fast as 24 per second per
  stud above, up to 2.727 times standing height per second: with
  gravity off it comes down at a steady 8.20 for standing height 3.0075
  (HipHeight 2), 13.64 for 5 and 9.55 for 3.5 (a 3-stud root), and from
  0.4 above it reached only 7.8. With gravity, whichever is faster wins:
  off a step the first frame came down as fast at gravity 20 as at 100,
  and after that it gained gravity's pace, unbraked until it reached
  standing height.
- Landing dips the root about 0.0064 s times the landing speed below
  standing height (0.09 at 14 studs/s, 0.14 at 24, 0.38 at 53 from a
  jump, 0.69 at 108), and it comes back up without overshooting, what's
  left of the dip shrinking about 43% a frame.
- Freefall lands within 0.4 per stud of legs (HipHeight on R15), 0.13
  per stud of root height and 0.05 above standing height: 1.11 for the
  default R15 body (measured falling at 1 to 8 studs/s, HipHeight 0.5 to
  4, root heights 1.4 to 3). Faster falls landed a frame either side of
  that. Inference: R6's legs count as HipHeight (2 studs, so 1.11 too),
  and its root as the root.
- A floor that moves carries the character at once, whether it's a moving
  part or an anchored part with a velocity (a conveyor).

## Walking

- The velocity closes on `WalkSpeed` with 2% of the gap left after one
  60 Hz frame, starting and stopping alike (no ramp).
- Up a 30° slope it walks at `WalkSpeed` along the surface: 13.9 across
  and 7.8 up, the root lagging about 0.3 below standing height.
- Past `MaxSlopeAngle` (40, against a 50° wedge) it can't climb: it
  slides back at g·sinθ·cosθ across the ground and tries again, still
  Running.
- `AutoRotate` turns the root toward the way it walks: about 1,000° per
  second at first, then the angle left shrinks about 14% a frame.
- In the air the velocity turns toward the walk at a steady 143 studs/s²
  (2.38 a frame), not at the ground's rate.

## Jumping and falling

- A jump launches the root at √(2 · gravity · JumpHeight) (53.15), or at
  `JumpPower` when `UseJumpPower` is on, and is ballistic after that; its
  first frame rose as if gravity were off for half of it. The apex was
  7.52 above standing height.
- States: Running, then Jumping for one frame, Freefall, Landed for two
  frames when it's back within reach of the floor, then Running.
- Walking off a ledge it stays Running, with FloorMaterial Air, for 0.1 s,
  then goes to Freefall; the same 0.1 s at gravity 50, so it's time, not
  a fall distance.
- FloorMaterial kept the floor's material for the first frames of a jump,
  until about 2.5 studs up.
- With Jumping disabled by `SetStateEnabled`, `Jump = true` does nothing
  and `Jump` goes back to false.

## Climbing and swimming

- Rungs in front of a solid board climb as a ladder when their faces
  stand 0.30 studs or more in front of it, never at 0.28, and sometimes
  at 0.29 (2026-11-10; 0.3-stud rungs 1.44 apart, as in a large place's
  ladder). Climbing began 0.9 to 1.15 studs from the rungs with or
  without the board, so the board isn't simply out of reach. On the same
  ladder, rungs 0.2 studs tall always climbed; 0.1 and 0.05 caught for a
  moment and let go.
- Walking into a TrussPart climbs it: Climbing, at 0.7 × `WalkSpeed`
  (11.2), with the root's center held 1.5 studs from the face. With no
  input it holds still, still Climbing.
- A 2-stud truss lying on the floor is climbed in hops (Observed
  2026-10-27): Climbing from 1.64 studs off its face until the root is
  4.7 up (0.15 s), Running forward at 16 while the legs pull it down, then
  Climbing again from 0.09 studs off the face, where it stays, and Running
  over the top. Climbing draws the root in to 1.5 studs but doesn't push
  it out.
- In terrain water it's Swimming: the root floats about half a stud under
  the surface and swims at about 11.8 (`WalkSpeed` 16). A jump at the
  surface goes Jumping, Freefall, and back to Swimming in the water.

## MoveTo and Move

- `MoveTo` walks to the point across the ground (its height doesn't
  matter), stops about 0.93 studs short and slides to 0.8, and fires
  `MoveToFinished(true)`. Against a wall it fires `MoveToFinished(false)`
  8 s after the call. `WalkToPoint` keeps the point afterwards.
- A `Move` with a direction cancels a `MoveTo`. `MoveDirection` read 0
  throughout a `MoveTo` (with the PlayerModule's controls calling
  `Move(0)` every frame).

## State requests

- `ChangeState(Jumping)` jumps: Jumping, Freefall, Landed, Running.
- `PlatformStand` goes to PlatformStanding and back to Running; `Sit`
  goes to Seated, and a jump from it goes straight to Running with `Sit`
  false.
- Seated leaves the body to physics, with or without a seat, and senses
  nothing: `FloorMaterial` reads Air. `Sit = true` on the floor drops an
  R6 or R15 character from root height 3 to 1 in about 0.25 s, onto what
  collides (R6's Torso, R15's root); it doesn't walk to a `MoveTo` or turn
  to the camera. Left alone it stays up only because that box sits flat:
  leaned 20 degrees it rocks back, and from 40 to 100 degrees R6 ends on
  its side at 90, staying Seated. Set 80 degrees in the air, R15 landed
  upside down and stayed there. Measured 2026-11-06.
- An unanchored 4 x 1 x 4 Seat tilted 20 degrees about X with an R6 or
  R15 sitter falls from y 12 at gravity, lands flat at y 0.5 within about
  0.8 s and stays Seated. About Z with R6 it tips over and rests on its
  edge at y 2.05, tilt 105.9; with R15 it lands flat. A default
  4 x 1.2 x 2 Seat tips over on landing, its sitter's weight off to one
  side. Jumping off one lying over goes to FallingDown
  (Seated, Running, FallingDown, as a body leaning past 45 degrees does);
  from the floor, the root rises to standing height in about 0.1 s.
- `ChangeState(FallingDown)` goes on to GettingUp after 3.0 s, and `Ragdoll`
  after 0.02 s; GettingUp goes on to Running after 0.05 s, or once it has
  risen from upside down (Approximation: Rook also waits for the root to
  reach standing height, a few frames); `Physics` stays until
  another `ChangeState`.

## In Rook

- Each frame `rook_lua` reads the properties of every Humanoid whose root
  this DataModel simulates (players' characters on their clients, the
  rest on the server; the root is `HumanoidRootPart`, or `Torso`) into a
  drive. Every 240 Hz step the controller sets the root body's velocity,
  then after the step feels the floor, a truss and water and moves
  through the states, so a walk takes the same path at any frame rate.
  Done once a frame instead, the steps of a slow frame steered by a floor
  up to a frame old: walking up 0.4-stud stairs, a 20 Hz client's path
  strayed up to 0.41 studs from a 60 Hz one's, so the legs bobbed more
  the slower the frames. Jump and Sit, read once a frame, are used up at
  the step that acts on them. Each driven Humanoid costs about a dozen
  ray casts a step while it walks, five while it stands.
- The floor is five rays down from the root part's bottom, at its center
  and its footprint's corners, to 2.5 studs below standing height; its
  height is the average of the hits. Inference: this matches the ledge,
  the steps and the slope.
- Below standing height the legs are an overdamped spring with no
  gravity (stiffness 2890, damping 119), fitted to the dips, the
  recovery and the 0.3 lag up a slope at once. Above it, each step the
  vertical speed over the floor takes whichever is faster of gravity's
  step and closing on the legs' pull at the ground's rate.
- Approximation: the legs act on a floor up to 2.5 studs below standing
  height. Studio's let go nearer: with gravity off, a root of the default
  body stood 1.9 above standing height was pulled down and 1.95 wasn't
  (HipHeight 1: 1.0 and 1.2; HipHeight 4: 2.9 and 3.8; a 3-stud root:
  1.95 and 2.05), and went to Freefall after 0.1 s.
- While the controller holds a body upright it locks its tilt and applies
  gravity itself; FallingDown, Ragdoll, PlatformStanding, Seated, Physics
  and Dead leave the body to physics, so a vehicle pitches and falls with
  its driver. A body that becomes upright again is stood
  up the same way as any other that leans: it closes on upright at about
  85 per second, at most about 2,000 degrees a second (measured: 20 degrees
  to 5 and 40 to 12 in a 60 Hz frame), about its root part. Seated isn't stood
  up: it leans as its seat does (measured on 30 and 60 degrees).
- Approximation: the same Seat tilted about Z with R6 matches (y 2.06,
  tilt 105.9), but about X it tips over at y 2.03, tilt 113.2, where
  Studio lands flat. Mass and center of mass match Studio's; how much
  spin the edge's bounce gives differs.
- Lean past 45 degrees while Running falls down (measured: it rights itself
  from 45, falls from 46, about X or Z alike). In the air it rights itself
  from 60. Approximation: what leans a body here is a script or a seat, as
  its lock keeps collisions from tipping it.
- Climbing draws the root in to 1.5 studs from the face and never
  pushes it out, so a low truss is climbed in hops as in Studio.
- Climbing: rays ahead at most 0.2 studs apart from the feet to above
  the root, 2 studs long, find a TrussPart, or a ladder (parts hit with a gap
  between them). A gap is a ray that hits nothing or hits 0.29 studs or
  more behind the nearest hit, so rungs on a board climb. Approximation:
  Roblox's sensor isn't documented.
- Swimming starts when the root's center is in a voxel of water (or a
  solid voxel's water), and floats toward 0.45 under the surface.
- Approximations: Flying holds height and steers as in the air;
  RunningNoPhysics and StrafingNoPhysics act as Running.
- Events: `StateChanged(old, new)`; `Jumping`, `FreeFalling`,
  `FallingDown`, `GettingUp`, `Ragdoll`, `PlatformStanding` and `Seated`
  fire true on entering their state and false on leaving it; `Running`,
  `Climbing` and `Swimming` fire their speed on entering and whenever it
  changes by more than 0.1 (Inference from creator-docs' event list; the
  threshold is Rook's). `StateEnabledChanged` fires when
  `SetStateEnabled` changes something. `FloorMaterial` follows the floor.
- `ChangeState` takes effect at the next step, if the state is enabled.
  `GetState` on a Humanoid this DataModel doesn't simulate reads Running,
  or Dead without health (Inference): a client's states don't reach the
  server yet (M10).
- Setting `WalkToPoint` or `WalkToPart` starts a walk as `MoveTo` does,
  so a server's `MoveTo` on a player's character reaches its client
  (Inference), but `MoveToFinished` fires only where the walk is
  simulated (M10).
- Spawned characters take StarterPlayer's `CharacterWalkSpeed`,
  `CharacterJumpPower`, `CharacterJumpHeight`, `CharacterUseJumpPower`,
  `CharacterMaxSlopeAngle` and `AutoJumpEnabled` (creator-docs
  StarterPlayer).
- An anchored part's velocity replicates, so a client simulating its
  character rides a conveyor (see [replication](../replication.md)).
