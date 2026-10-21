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
  groups) and Terrain's `SmoothGrid` (Roblox streams voxels to clients
  its own way; clients see the terrain).

Confirmed for Sound's three (the server's sounds are heard on clients,
at their speed); Inference from the database for the rest.
