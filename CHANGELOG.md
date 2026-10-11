# Changelog

## Versioning

Rook follows [Semantic Versioning](https://semver.org). Until 1.0:

- **Minor** (`0.x.0`) — a milestone is finished. The minor number is the milestone: 0.2.0 is M2, 0.3.0 will be M3.
- **Patch** (`0.x.y`) — fixes and additions between milestones.

Every crate and app shares the workspace version. Each release is tagged `vX.Y.Z` in git. The pinned Roblox version (currently 0.741.19) is separate and listed under each release.

## [Unreleased]

## [0.2.0] — 2026-10-11 — M2: client side, renderer, input

Pinned Roblox version: 0.741.19.

- `rook-render` and `rook-player`: wgpu renderer for parts and primitive meshes, camera, basic lighting and sky; fly camera; `--screenshot`, `--bench`.
- Client/server split: separate DataModels joined by the in-process replicator (`rook-net`); `Player`, `Backpack`, `PlayerGui`, `PlayerScripts`; LocalScripts.
- RemoteEvent, UnreliableRemoteEvent and RemoteFunction.
- TweenService, Tween and the legacy GUI tweens.
- Input: UserInputService, `Mouse`, ClickDetector, mouse ray queries.
- Build profiles (`dev`, `release`, `profiling`) and Tracy zones; incremental scene updates.
- Console output in Roblox Studio's format, coloured by message type.
- Every inert or partial API is listed in `docs/behavior/stubs.md`.

## [0.1.0] — 2026-10-11 — M0 and M1: formats, DOM and scripting

Pinned Roblox version: 0.741.19.

- M0: workspace, datatypes, class database, binary and XML place formats, the legacy XML upgrade (99.3% of the archive loads), `rook dump/roundtrip/corpus-scan`.
- M1: `rook-dom`, `rook-signal`, `rook-scheduler`, the Luau bridge (`rook-lua`), the core Instance API and the members list for 0.741.19; headless server Scripts (`run-headless`, `corpus-run`); conformance through Studio's MCP server.
- Renamed from Raven to Rook.

[Unreleased]: https://github.com/AsyncFinn/rook/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/AsyncFinn/rook/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/AsyncFinn/rook/releases/tag/v0.1.0
