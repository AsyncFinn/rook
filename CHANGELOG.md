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
- Contacts and materials (M3 step 3): every material's density, friction and elasticity (`PhysicalProperties.new(material)`), and `CustomPhysicalProperties`; touching parts mix friction and elasticity by their weights, as measured in Studio. `GetMass` uses the real shape volume (balls, cylinders, wedges).
- `Touched` and `TouchEnded` fire from contacts, on both parts; `CanTouch` turns them off.
- Collision groups: `BasePart.CollisionGroup`, `CanCollideWith` and PhysicsService's group methods (current and deprecated), stored in Workspace's `CollisionGroupData` as Roblox saves it; `TouchesUseCollisionGroups` is honoured.
- Physics conformance cases (`physics/joints`, `physics/contacts`).
- Faster scripting: member lookups are resolved once per class, `inst.Key` and `inst.Key = v` take a dedicated path into Rust, the engine-function shim no longer forwards results through a vararg helper, `GetChildren`/`GetDescendants` build their arrays in Luau, and the script-timeout check reads the clock every 1024th interrupt. A modern core place's 10 s run drops from about 46 s to 32 s, with identical output.
- Command bar: in an interactive terminal, `rook-player` shows a `>` prompt that runs Luau in the open place, like Luau's REPL (expressions print their value, globals persist, unfinished statements continue on `>>` lines, line editing and history, output above the prompt; `:server`/`:client` pick the side); `rook run-headless --repl` does the same without a window. It is the `repl` Cargo feature, on by default; `--no-repl` turns the prompt off.
- Fixed: engine functions that return nothing (`print`, `Destroy`, ...) returned one `nil` since the faster engine-function shim, which changed `select("#", ...)`.
- BodyMovers and constraints (M3 step 4), from Studio measurements: BodyForce, BodyThrust, BodyVelocity, BodyAngularVelocity, BodyPosition, BodyGyro and RocketPropulsion (`Fire`, `Abort`, `ReachedTarget`) push their part's whole assembly every 240 Hz step. BodyVelocity and BodyAngularVelocity are velocity constraints limited only by MaxForce/MaxTorque; BodyPosition and BodyGyro are mass-independent spring-dampers (BodyGyro at a quarter strength, on the sine of the angle, with MaxTorque limiting angular acceleration).
- RotateV and RotateP are driven by their surface's `SurfaceInput`, `ParamA` and `ParamB` (RotateV turns at 31 rad/s per unit; RotateP steps 0.01 rad per step toward its angle; NoInput holds); Rotate stays free.
- HingeConstraint (free, Motor, Servo, limits, `CurrentAngle`), BallSocketConstraint, RopeConstraint and RodConstraint become Rapier joints between attachments; SpringConstraint, VectorForce and Torque apply forces.
- Fixed: the deprecated `Velocity` and `RotVelocity` were stored apart from the velocities physics uses, so scripts launching parts with them (a disaster place's volcano, meteors, tornado) did nothing. They're now views of `AssemblyLinearVelocity`/`AssemblyAngularVelocity` (`Velocity` reads the assembly's velocity at the part), velocities saved in files are adopted at load, and setting any of them sets the whole assembly's. Anchored parts keep their velocity and carry what rests on them, like Roblox's conveyors.
- Physics conformance cases `physics/movers`, `physics/constraints` and `physics/velocity`. `rook conformance record` picks the Studio window whose place name contains `ROOK_STUDIO` when several are open.
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
