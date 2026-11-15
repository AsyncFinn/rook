# Network ownership: who simulates an assembly

Checked against Studio 0.747 on 2026-11-15.

On Roblox each unanchored assembly is simulated by one machine, its network
owner: a player's client steps its physics and sends the result to the
server, or the server does (creator-docs "Network ownership"). A player's
client owns its character, and server scripts can hand any other assembly
to a player with `BasePart:SetNetworkOwner`, as games' draggers do so that
a force the client puts on the part moves it. `rook_lua`'s
`characters::ownership`, `rook_physics::ownership` and
`rook_physics::Authority`, and the session host's `Owned` and `Pose`
messages.

## In Studio

Measured in a Baseplate playtest (one player; server and client in one
process, so no real network delay).

The character:

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

The API (Studio 0.747, 2026-11-15):

- `SetNetworkOwner(player)` makes that player the owner of the part's
  whole assembly: every part of it answers the player, and
  `GetNetworkOwnershipAuto()` turns false. `SetNetworkOwner(nil)`,
  `SetNetworkOwner()` and an Instance that isn't a Player (`workspace`)
  all give it to the server, also with automatic ownership off; a value
  that isn't an Instance is `Unable to cast value to Object`.
- `SetNetworkOwnershipAuto()` turns automatic ownership back on.
  Automatic ownership gives an unanchored assembly near a player's
  character to that player: one 6 studs away was the player's within half
  a second, one 400 studs away stayed the server's. A character's root set
  to the server and then back to automatic answered nil at once and the
  player half a second later.
- The setting belongs to the assembly as it was: welding another part on,
  breaking one of its welds, or anchoring a part of it and unanchoring it
  again puts the whole of it back to automatic at once. Moving the part
  out of Workspace and back keeps it.
- Errors, checked in this order: on a client every method raises
  `Network Ownership API can only be called from the Server.`; outside
  Workspace, `Can only call Network Ownership API on a part that is
  descendent of Workspace`; for an anchored part or one welded to one,
  `Network Ownership API cannot be called on Anchored parts or parts
  welded to Anchored parts.`, `GetNetworkOwner` and
  `GetNetworkOwnershipAuto` included. `CanSetNetworkOwnership()` returns
  true, or false and that message, without raising.

## In Rook

- Who owns what is physics' to keep, as the owner of assemblies
  (`rook_physics::ownership`): a script's setting holds the assembly's
  parts as they were when it was made. The journal entries that change
  assemblies (a joint's `Part0`, `Part1` or `Enabled`, a joint added or
  removed, `Anchored`) put the assembly back to automatic, read before
  every question about ownership, so a script sees the reset at once. A
  player that leaves loses what it owned (Inference: nothing is left to
  simulate it).
- Automatic ownership is the character rule alone: a player's client owns
  its character's assembly, and the server everything else. Roblox's
  handing of loose parts to nearby players is still to come (M10 step 3),
  so `GetNetworkOwner` answers nil for them, which is who simulates them
  in Rook. A character's ownership turned back to automatic is its
  player's at once, where Studio waits a moment.
- A client simulates only what it owns: its character (unless a script
  gave it away) and the assemblies of the parts the server told it about;
  everything else in its world follows replication, so a force the client
  puts on a part it doesn't own moves nothing. A client with nothing to
  simulate steps nothing, and its world only answers queries.
- The server sends each client what it owns, by the server's ids, when
  that changes (`Owned`); the client translates them as the parts arrive,
  so a part made and given away in one server frame is the client's once
  it replicates.
- A joint (a hinge surface, a constraint) is in a world's physics only
  when a part on one side is simulated there: one between parts that
  world only follows can't move anything, but Rapier solved it every
  step all the same, as it does any joint with a moving kinematic side
  (Jolt leaves such constraints out of its solve). Measured walking 60
  s in a place with 228 server-driven contraptions and 246 joints: the
  client's step went from 9.9 to 5.1 ms on average, its slowest from 42
  to 16 ms.
- The server simulates everything its clients don't own, and moves what
  they own where they say. Those are kinematic bodies there, so the
  server's Touched fires for them against anchored parts and other
  characters. When ownership comes back to the server the assembly picks
  up from the last pose and velocities its owner sent.
- After each step a client sends, for each assembly it owns, its root
  part's CFrame and velocities when they changed; the server moves the
  assembly there before its next frame and replicates it to the other
  clients (Approximation: Roblox sends at a lower rate and the server
  sees it later). The sender isn't sent its own move back, but the
  server scripts' moves of what it owns do reach it. The server's
  physics takes a move up by placing the assembly again around the part
  that was moved, which writes the same CFrames and so changes nothing;
  placing it from its root again rounded the CFrames a little
  differently while a character turned, and that second write went back
  to the client a frame stale, snapping its walk back about every other
  frame.
- A pose is taken only from the client that owns the assembly, and not
  while the server has it anchored (an anchored assembly has no owner).
  A pose sent just before the client heard of the anchoring would
  otherwise land after the server's moves and undo them: a lift that
  anchors its riders and carries them down kept them, on the server,
  where they boarded.
- The Humanoid controller runs where the character is simulated: on its
  client ([movement](../characters/movement.md)).
