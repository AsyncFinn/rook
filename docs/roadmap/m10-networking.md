# M10 Networking

Straight after characters: once there's a character, the next thing is
playing together. Replication already runs in-process between a server
and its client; this puts a real transport behind it.

1. A transport (UDP, reliable and unreliable channels) behind the
   replicator's interface; joining, leaving, timeouts.
2. Several clients, each with its own player and character.
3. Network ownership: a player's client simulates its character and what
   it owns (`SetNetworkOwner`).
4. RemoteEvents, RemoteFunctions and UnreliableRemoteEvents across
   machines (114, 73 and 4 files), with Roblox's limits and ordering.
5. Bandwidth: deltas and interest management, so large archived places
   stay playable.
