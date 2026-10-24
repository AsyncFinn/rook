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
- The server simulates everything but its clients' characters, which it
  moves where their clients say. They are kinematic bodies there, so the
  server's Touched fires for them against anchored parts and other
  characters.
- After each step a client sends its character's root part's CFrame and
  its assembly's velocities when they changed; the server moves the
  assembly there before its next frame and replicates it to the other
  clients (Approximation: Roblox sends at a lower rate and the server
  sees it later). The sender isn't sent its own move back, but the
  server scripts' moves of its character do reach it.
- Only the player's own character is taken from its client: a pose for
  any other part is ignored.
- Until the Humanoid controller (step 3), a client holds its character
  upright at standing height over the floor below it (Approximation;
  `rook_physics::Physics::stand`), so it stands on its feet.
- `GetNetworkOwner` answers the player for its character's assemblies
  and nil (the server) for everything else; it and
  `GetNetworkOwnershipAuto` raise on a client and for parts outside
  Workspace (Inference from Roblox's messages for `SetNetworkOwner`).
  `SetNetworkOwner` and automatic ownership of other parts are M10's.
