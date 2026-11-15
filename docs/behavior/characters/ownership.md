# Character ownership: who simulates a character

Checked against Studio 0.744 on 2026-10-24.

On Roblox a player's client owns its character's assembly: the client
steps its physics and sends the result to the server (creator-docs
"Network ownership"). `rook_lua`'s `characters::ownership`,
`rook_physics::Authority`, and the session host's `Pose` message.

## In Studio

Measured in a Baseplate playtest (one player; server and client in one
process, so no real network delay):

- `HumanoidRootPart:GetNetworkOwner()` is the player, and
  `GetNetworkOwnershipAuto()` is true.
- A client moving its character with `CFrame` each Heartbeat: the server
  saw each position about 0.28 s later, and about one in four of the
  client's frames never reached it (the server got the moves at a lower
  rate, in order).
- The server's `Touched` fires for the character's parts against an
  anchored part, with the server's view of their positions at the time.
- A server script setting the character's root `CFrame` moves it on the
  owning client (it saw the move 35 ms later) and the client goes on
  simulating from there.

## In Rook

- A client simulates only its own character's assembly (the parts in its
  player's `Character`, and what's rigidly joined to them); everything
  else in its world follows replication. A client with no character
  steps nothing, and its world only answers queries.
- A joint (a hinge surface, a constraint) is in a world's physics only
  when a part on one side is simulated there: one between parts that
  world only follows can't move anything, but Rapier solved it every
  step all the same, as it does any joint with a moving kinematic side
  (Jolt leaves such constraints out of its solve). Measured walking 60
  s in a place with 228 server-driven contraptions and 246 joints: the
  client's step went from 9.9 to 5.1 ms on average, its slowest from 42
  to 16 ms.
- The server simulates everything but its clients' characters, which it
  moves where their clients say. They are kinematic bodies there, so the
  server's Touched fires for them against anchored parts and other
  characters.
- After each step a client sends its character's root part's CFrame and
  its assembly's velocities when they changed; the server moves the
  assembly there before its next frame and replicates it to the other
  clients (Approximation: Roblox sends at a lower rate and the server
  sees it later). The sender isn't sent its own move back, but the
  server scripts' moves of its character do reach it. The server's
  physics takes a move up by placing the assembly again around the part
  that was moved, which writes the same CFrames and so changes nothing;
  placing it from its root again rounded the CFrames a little
  differently while the character turned, and that second write went
  back to the client a frame stale, snapping its walk back about every
  other frame.
- Only the player's own character is taken from its client: a pose for
  any other part is ignored, and so is one for the character while the
  server has it anchored (an anchored assembly has no network owner, as
  `GetNetworkOwner` says). A pose sent just before the client heard of
  the anchoring would otherwise land after the server's moves and undo
  them: a lift that anchors its riders and carries them down kept them,
  on the server, where they boarded.
- The Humanoid controller runs where the character is simulated: on its
  client ([movement](movement.md)).
- `GetNetworkOwner` answers the player for its character's assemblies
  and nil (the server) for everything else; it and
  `GetNetworkOwnershipAuto` raise on a client and for parts outside
  Workspace (Inference from Roblox's messages for `SetNetworkOwner`).
  `SetNetworkOwner` and automatic ownership of other parts are M10's.
