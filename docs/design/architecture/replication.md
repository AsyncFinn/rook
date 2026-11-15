# Services and replication

Part of the [architecture](../architecture.md).

## Services (first wave)

DataModel, Workspace, Players, Lighting, ReplicatedStorage, ReplicatedFirst, ServerScriptService, ServerStorage, StarterGui, StarterPack, StarterPlayer, Teams, RunService, UserInputService, ContextActionService, TweenService, Debris, CollectionService, HttpService (opt-in), ContentProvider, TextService, PhysicsService, LogService, SoundService, InsertService (limited). DataStoreService is backed by local files. That is an Approximation, and it's documented as one.

## Run modes and replication (`rook-net`)

- Modes: **Edit** (no scripts), **Run** (server simulation, no player), **Play** (server + client), **Server** (headless).
- Modern Play runs **separate server and client DataModels**. Rook does the same from M2 on, connected by an **in-process loopback replicator**. That makes `LocalScript` vs `Script`, RemoteEvents/RemoteFunctions and replication filtering correct early. A real network transport (BrikNet, quinn or renet, to be evaluated) plugs in later behind the same interface.
- Replication is Rook-to-Rook. Wire compatibility with real Roblox clients is not a goal.
