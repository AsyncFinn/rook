# Changelog

## Versioning

Rook follows [Semantic Versioning](https://semver.org):

- **Minor** (`0.x.0`): a release with new features. Finishing a milestone is a natural time for one, but releases aren't tied to milestones, and each entry below says which milestone, if any, it finishes.
- **Patch** (`0.x.y`): fixes only.
- Before 1.0, a minor release may also break things, as semver allows.

Every crate and app shares the workspace version. Each release is tagged `vX.Y.Z` in git. The pinned Roblox version (currently 0.741.19) is separate and listed under each release.

## [Unreleased]

- `rook-physics`: Workspace parts are simulated with Rapier on the server at 240 Hz. Anchored parts are fixed; each unanchored part is its own body for now. Workspace.Gravity and FallenPartsDestroyHeight apply; CFrame and assembly velocities are written back and replicate.
- Joints and assemblies (M3 step 2): Weld, ManualWeld, Snap, Glue, ManualGlue, Motor, Motor6D and WeldConstraint join parts into assemblies, each simulated as one body. As in Roblox, every anchored part roots its own assembly, which takes along the unanchored parts joined to it; anchored parts joined to each other stay separate. Rotate, RotateP and RotateV are free hinges between assemblies. Motors turn toward `DesiredAngle`, and Motor6D applies `Transform`.
- Scripts: `MakeJoints` and `BreakJoints` (BasePart, Model, Workspace) follow Studio's surface rules. Also added: `GetJoints`, `GetConnectedParts`, `GetRootPart`, `IsGrounded`, `AssemblyRootPart`, `AssemblyMass`, `Motor:SetDesiredAngle` and `getMass`. Setting a part's `CFrame` moves its whole assembly; setting `Position`/`Orientation`/`Rotation` moves only that part and rewrites its side of its joints.
- Places saved before `Workspace.ExplicitAutoJoints` existed have their parts joined by surfaces when they load, as Roblox did then (Inference: such places save almost no joints).
- `rook-dom` indexes reference properties both ways (`Dom::referrers`).
- First physics conformance case (`physics/joints`).
- Faster scripting: member lookups are resolved once per class, `inst.Key` and `inst.Key = v` take a dedicated path into Rust, the engine-function shim no longer forwards results through a vararg helper, `GetChildren`/`GetDescendants` build their arrays in Luau, and the script-timeout check reads the clock every 1024th interrupt. A modern core place's 10 s run drops from about 46 s to 32 s, with identical output.
- Tracy zones per resumed script (`script: <chunk>:<line>`), per `Instance.__index`/`__newindex` (key as zone text), per native Instance method, and around all engine functions (`api`).

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
