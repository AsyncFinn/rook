# Replication

Checked against Studio 0.743 on 2026-10-14.

What a client DataModel receives from the server, as `rook-net` copies it.

## What replicates

From creator-docs "Client-server model" and "Replication":

- Instances under services that replicate. Classes marked NotReplicated
  in the API dump (ServerScriptService, ServerStorage, Camera,
  PlayerScripts, ...) and everything under them never reach clients.
- A player's PlayerGui reaches only that player (PlayerReplicated).
- Properties except those marked NotReplicated. Server `Script` source
  is withheld (Inference: Roblox doesn't send server code to clients);
  LocalScript and ModuleScript source is sent, and so is a `Script`'s
  whose `RunContext` is Client.
- Attributes and tags.

Changes made on the client stay on the client (FilteringEnabled, which
modern Roblox always uses).

## NotReplicated properties clients still see

Some properties are NotReplicated in the database because Roblox sends
them through a hidden twin Rook doesn't store, so Rook sends the property
itself (`rook_net::source`). Found by listing every saved, NotReplicated
property in the database (the rest are server- or Studio-only):

- a replicated twin named `<name>Replicator`, `<name>Replicate` or
  `<name>Replicating`: Sound's `Playing` and `TimePosition`, VideoFrame's,
  BasePart's `CollisionGroup`, MeshPart's `RenderFidelity`, Humanoid's
  `Jump`, Player's `AccountAge` and `MembershipType`, and Instance's
  `Attributes` (which Rook sends on their own anyway);
- an `Xml` property whose replicated twin drops the suffix
  (BallSocketConstraint's `MaxFrictionTorqueXml`);
- pairs with nothing in their names to go by: Sound's `PlaybackSpeed`
  (replicated as `Pitch`), Player's `Team` (Roblox clients read their
  team), Workspace's `CollisionGroups` (clients know the registered
  groups), Terrain's `SmoothGrid` (Roblox sends voxels to clients
  its own way, see [terrain](#terrain); clients see the terrain) and TextSource's `UserId`
  (Roblox's chat on clients finds the local player's TextSource by it).

Confirmed for Sound's three (the server's sounds are heard on clients,
at their speed); Inference from the database for the rest.

Parts' `AssemblyLinearVelocity` and `AssemblyAngularVelocity` reach
clients too: Roblox sends them with parts' physics. A client-simulated
character rides an anchored conveyor in Studio (measured 2026-10-26;
[movement](characters/movement.md)), and a client reads 10 for a part the
server moves at 10 studs/s (measured 2026-10-31).

## Physics

Measured 2026-10-31 in a Baseplate playtest over Studio's MCP: a
server-owned part pushed at a steady 10 studs/s by a LinearVelocity, read
by a client script every RenderStepped.

- The client shows it moving every frame by its speed times the frame
  (0.15 to 0.19 studs in frames of 16 to 18 ms), never in the steps of the
  updates, which reach it about 30 times a second (a server attribute set
  every Heartbeat changed 61 times in 120 client frames).
- It's shown behind the newest update the client has: 0.127 to 0.164 s,
  median 0.146 (the server stamped its position on the part every
  Heartbeat; the client compared each stamp with the part's position).
- It interpolates: turned around by the server at x 222.66, the part went
  no further than 222.48 on the client, and its velocity there eased from
  10 to -10 over about 5 frames, about 0.17 s after the server turned it.
- Client scripts read it where it's shown (`CFrame`, the velocities).

In Rook (`rook_physics`'s `remote.rs`): each server frame's updates carry
its server time, and a client keeps the poses and velocities of the
assemblies it doesn't simulate as they arrive, played back 0.146 s behind
the newest by cubic Hermite interpolation between them (the standard
snapshot interpolation), as each 240 Hz step's kinematic target. So what
it draws, what its scripts read (by whole steps, as for its own parts),
contacts and a floor's velocity under its character all follow the same
smooth motion: a character rides a platform the server moves. An
assembly a frame's updates don't move held still. Past the server's
newest frame it goes on at its velocity for at most 0.146 s (Inference:
the delay covers late updates). The playback clock closes on its target
over about a second, so updates arriving unevenly (a server falling behind
and catching up) don't jerk it, and jumps when more than half a second
off. Another player's character is one of these assemblies.
Before, a client moved such an assembly when updates arrived: a 144 Hz
client saw a moving part stand still for frames and jump four 240 Hz steps
at once, and a character on a server's platform rode 0 of its 6 studs.
A local script moving one puts it there at once, until the next update
(Inference).
Only the played assemblies near something the client simulates (its
character, its own loose parts) take every step's target: those whose
broad-phase box is within how far the two can close in the frame, plus
4 studs for a Humanoid's floor and climbing rays, of a simulated body's.
The rest sleep in Rapier and are put where playback has them once, after
the frame's steps, moving at what they covered over its last step, so
their velocity, where they're drawn between the last two steps and where
scripts and queries find them after the frame are what moving them at
every step gives, without the steps nothing feels: on a 2018-era place
with 227 moving server-owned assemblies, each step of the client's had
been about half a millisecond spent moving them and finding no contact.

An assembly that moves whole sends one pose, its root part's CFrame and
velocities, not one per part: the client places the rest by its own copy of
the joints, as playback does anyway. For that, a WeldConstraint keeps where
it holds its parts in its hidden `CFrame0` (Part1's pose in Part0's frame,
as saved files show: it equals `Part0.CFrame:Inverse() * Part1.CFrame` in
each one read), which replicates with it; Rook used to read the parts'
poses again, which a client that hears only the root can't do. It's taken
when a script sets Part0, Part1 or Enabled, or moves one of the parts on
its own by Position (Inference). A part a motor turns while its root holds
still is sent itself.

## Terrain

Measured 2026-11-03 in a Baseplate playtest (Studio 0.743, streaming on):
the server wrote 128 by 16 by 128 voxels of random materials and
occupancies, then a server script toggled a `FillBall` between Grass and
Air every 0.5 s; the client's `Stats.Network` replicator items were read
while it ran (`DataReceiveKbps` reads 0 in a local playtest).

- Each edit reaches the client as one "cluster" packet (Received Cluster
  Packets about 2 a second, one per edit), and the client reads every
  flip with `ReadVoxels`.
- A packet holds the block of terrain around the edit, never the whole
  grid: a radius-1.5 ball costs 441 bytes in empty air but 4.9 KB inside
  the random voxels (its neighbors go with it), a radius-6 one 2.4 KB in
  air and 6.3 KB inside; a radius-24 ball in air, all full voxels, 272
  bytes. The random grid alone is hundreds of KB. From 4.9 KB of random
  voxels, the block is about 16 voxels on a side (Inference).

In Rook (`rook_net::source`): an edit's region (`Change::Voxels`) goes to
each client that sees the Terrain as `Update::Voxels`, the chunks holding
the region as the server has them, which the client takes in place of
its own (`Dom::write_voxels`). Rook's chunks are its storage's 32 voxels
on a side, larger than Roblox's block. A joining client gets the voxels
whole, as `SmoothGrid`, and so does a script's whole write of it.

## From clients

Changes made on a client stay there, except what Roblox takes from
clients: the moves of a player's own character, which its client
simulates (network ownership; see
[who simulates a character](characters/ownership.md)), and the Tools it
equips. Remote events and ClickDetector clicks also reach the server.
