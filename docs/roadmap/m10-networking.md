# M10 Networking

RemoteEvent 1,929 files, RemoteFunction 1,541, UnreliableRemoteEvent 6;
scripts call `FireServer` in 1,866 files, `FireClient` in 1,442,
`InvokeServer` in 1,328, `FireAllClients` in 1,002, `InvokeClient` in 821,
`SetNetworkOwner` in 764 and `Kick` in 1,237.

Straight after characters: once there's a character, the next thing is
playing together. Replication already runs in two halves, `rook-net`'s
Source (server changes as self-contained updates) and Sink (applying them
to a client), joined in-process, and remotes copy their arguments as
`NetValue`s (`rook-lua/src/net.rs`). M9 adds the client's half for the
character it owns. This milestone puts all of them on the wire.

1. **Wire and transport**: an encoding for snapshots, updates, owned
   physics and remote calls, and a transport with reliable ordered and
   unreliable channels.
   - *Scope*: the encoding (versioned, with a handshake that checks the
     Rook build and the place); joining, leaving, timeouts,
     `PlayerRemoving`; `Kick` (1,237 files) with its message shown by
     M8's error prompt; joining by address from `rook-player`, one
     machine hosting while playing. Clients fetch content themselves, as
     they do now.
   - *Evidence first*: pick the transport. The
     [architecture](../design/architecture.md#run-modes-and-replication-rook-net)
     names BrikNet (the earlier experiment), quinn and renet. Compare
     them on channels, encryption, NAT traversal later (M12), pure Rust,
     and Windows, Linux and macOS. Measure the encoded snapshot of the
     archive's largest places to see whether compression is needed.
   - *Studio questions*: the order things arrive in on joining
     (ReplicatedFirst, then what?), when LocalScripts start against that
     order, and when `game.Loaded` and `IsLoaded` (475 files) turn true.
     Scripts depend on it through `WaitForChild` (2,587 files) and
     `RemoveDefaultLoadingScreen` (297).
   - *How*: a `Link` in `rook-net` with an in-process implementation
     (with optional delay and loss, for tests) and a UDP one, carrying
     the existing Source and Sink messages.
   - *Done when*: a client on another machine joins, plays and leaves an
     archive place, and the in-process link with 100 ms delay and 2%
     loss passes the replication tests.
2. **Several clients**, each with its own player, character and
   PlayerGui; `GetPlayers` order; what one player sees of another's
   character (positions interpolated between updates).
   - *Studio questions*: how far behind the owner another client sees a
     character, and whether `Workspace.InterpolationThrottling` changes
     it. Studio's multi-client test runs several clients; first check
     whether the MCP reaches each of them.
   - *Done when*: four clients, in one process and over the network,
     play an archive place and see each other move smoothly.
3. **Network ownership** beyond characters: `SetNetworkOwner`,
   `GetNetworkOwner` (130 files), `SetNetworkOwnershipAuto` (113),
   `CanSetNetworkOwnership` (123), automatic ownership by distance to
   characters, and hand-offs between owners. Anchored parts always stay
   with the server.
   - *Studio questions*: the automatic ownership radius and its
     hysteresis, what happens to an assembly mid-hand-off, and what
     `Workspace.AuthorityMode` (in the pinned API) does, with whether
     any archive place saves it.
   - *Done when*: an unanchored pile pushed by two players moves
     consistently for both, and ownership cases match Studio.
4. **Remotes across machines**: RemoteEvents, RemoteFunctions and
   UnreliableRemoteEvents over the link.
   - *Scope*: ordering against property changes made before the fire;
     events queued until a handler connects; instances the receiver
     can't see arriving as nil; size limits and throttling;
     UnreliableRemoteEvent drops and its size limit; `InvokeClient`
     erroring when the client leaves.
   - *Studio questions*: each of the above as a conformance case:
     the queue's length and warning, what is dropped, the size limits'
     numbers and errors.
   - *Done when*: the remote cases pass with clients on the network
     link, including its delay and loss.
5. **Bandwidth and streaming**: large archive places stay playable.
   - *Scope*: invisible savings first (deltas, quantized CFrames for
     moving parts, update rates per client); then instance streaming as
     Roblox defines it, only for places that turn on `StreamingEnabled`
     (`StreamingMinRadius`, `StreamingTargetRadius`, `ModelStreamingMode`,
     `RequestStreamAroundAsync` in 23 files, `ReplicationFocus`). A place
     without streaming gets every replicated instance, since scripts see
     what is there.
   - *Evidence*: how many archive places set `StreamingEnabled` (the
     corpus scan doesn't keep property values yet; add it when the scan
     next runs, or sample).
   - *Done when*: the archive's largest places join over a home
     connection in a time set once step 1's measurements exist, and
     steady play stays under a per-client rate set then too.
