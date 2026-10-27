# Humanoid movement and states

Checked against Studio 0.744 on 2026-10-26.

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
- Landing from a jump dips the root 0.38 below standing height, and it
  springs back over about 0.2 s without overshooting much.
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
- `ChangeState(FallingDown)` goes on to GettingUp, and `Ragdoll` to
  GettingUp and Running, each within a few seconds; `Physics` stays until
  another `ChangeState`.

## In Rook

- Each frame `rook_lua` reads the properties of every Humanoid whose root
  this DataModel simulates (players' characters on their clients, the
  rest on the server; the root is `HumanoidRootPart`, or `Torso`) into a
  drive. The controller sets the root body's velocity every 240 Hz step;
  after the frame's steps it feels the floor, a truss and water, and
  moves through the states. Each driven Humanoid costs about a dozen ray
  casts a frame while it walks, five while it stands.
- The floor is five rays down from the root part's bottom, at its center
  and its footprint's corners, to 2.5 studs below standing height; its
  height is the average of the hits, extrapolated along the slope between
  frames. Inference: this matches the ledge, the steps and the slope.
- The legs are a spring holding standing height, fitted to the landing
  (stiffness 2450, damping 80); it pulls down as well as up, so a
  character walking off a step lower than 2.5 studs follows it down.
- While the controller holds a body upright it locks its tilt and applies
  gravity itself; FallingDown, Ragdoll, PlatformStanding, Physics and Dead
  leave the body to physics. A body that becomes upright again is stood
  up at once (Approximation: Roblox rises over GettingUp).
- Climbing draws the root in to 1.5 studs from the face and never
  pushes it out, so a low truss is climbed in hops as in Studio.
- Climbing: six rays ahead at heights from the feet to above the root,
  2 studs long, find a TrussPart, or a ladder (parts hit with a gap
  between them). Approximation: Roblox's sensor isn't documented.
- Swimming starts when the root's center is in a voxel of water (or a
  solid voxel's water), and floats toward 0.45 under the surface.
- Approximations: Landed is taken within 0.5 studs of standing height
  (Studio landed at 0.23, not at 0.88); FallingDown and Ragdoll last 1 s
  and GettingUp 0.5 s; Seated stands in place until seats exist (M9's
  seats step); Flying holds height and steers as in the air;
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
