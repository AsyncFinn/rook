# Changelog

## Versioning

Rook follows [Semantic Versioning](https://semver.org):

- **Minor** (`0.x.0`): a release with new features. Finishing a milestone is a natural time for one, but releases aren't tied to milestones, and each entry below says which milestone, if any, it finishes.
- **Patch** (`0.x.y`): fixes only.
- Before 1.0, a minor release may also break things, as semver allows.

Every crate and app shares the workspace version. Each release is tagged `vX.Y.Z` in git. The pinned Roblox version (currently 0.741.19) is separate and listed under each release.

## [Unreleased]

- README: a new pitch (offline singleplayer, self-hosting, preservation, modding, opt-in extensions), credits moved up as "Standing on the shoulders of giants" with Rapier added, and the status brought up to M3. ROADMAP lists the planned Rook extensions (unscheduled).
- Docs split: the README is now a short overview (287 to 108 lines) that links to `docs/getting-started.md` (building, playing, controls, command bar), `docs/tools.md` (the `rook` CLI), `docs/features.md` (the full list, now with physics) and `CONTRIBUTING.md` (reporting problems, crate map, development).
- `rook-physics`: Workspace parts are simulated with Rapier on the server at 240 Hz. Anchored parts are fixed; each unanchored part is its own body for now. Workspace.Gravity and FallenPartsDestroyHeight apply; CFrame and assembly velocities are written back and replicate.
- Joints and assemblies (M3 step 2): Weld, ManualWeld, Snap, Glue, ManualGlue, Motor, Motor6D and WeldConstraint join parts into assemblies, each simulated as one body. As in Roblox, every anchored part roots its own assembly, which takes along the unanchored parts joined to it; anchored parts joined to each other stay separate. Rotate, RotateP and RotateV are free hinges between assemblies. Motors turn toward `DesiredAngle`, and Motor6D applies `Transform`.
- Scripts: `MakeJoints` and `BreakJoints` (BasePart, Model, Workspace) follow Studio's surface rules. Also added: `GetJoints`, `GetConnectedParts`, `GetRootPart`, `IsGrounded`, `AssemblyRootPart`, `AssemblyMass`, `Motor:SetDesiredAngle` and `getMass`. Setting a part's `CFrame` moves its whole assembly; setting `Position`/`Orientation`/`Rotation` moves only that part and rewrites its side of its joints.
- Places saved before `Workspace.ExplicitAutoJoints` existed have their parts joined by surfaces when they load, as Roblox did then (Inference: such places save almost no joints).
- `rook-dom` indexes reference properties both ways (`Dom::referrers`).
- Contacts and materials (M3 step 3): every material's density, friction and elasticity (`PhysicalProperties.new(material)`), and `CustomPhysicalProperties`; touching parts mix friction and elasticity by their weights, as measured in Studio. `GetMass` uses the real shape volume (balls, cylinders, wedges).
- `Touched` and `TouchEnded` fire from contacts, on both parts; `CanTouch` turns them off.
- Collision groups: `BasePart.CollisionGroup`, `CanCollideWith` and PhysicsService's group methods (current and deprecated), stored in Workspace's `CollisionGroupData` as Roblox saves it; `TouchesUseCollisionGroups` is honoured.
- Physics conformance cases (`physics/joints`, `physics/contacts`).
- Faster scripting: member lookups are resolved once per class, `inst.Key` and `inst.Key = v` take a dedicated path into Rust, the engine-function shim no longer forwards results through a vararg helper, `GetChildren`/`GetDescendants` build their arrays in Luau, and the script-timeout check reads the clock every 1024th interrupt. A large modern place's 10 s run drops from about 46 s to 32 s, with identical output.
- Command bar: in an interactive terminal, `rook-player` shows a `>` prompt that runs Luau in the open place, like Luau's REPL (expressions print their value, globals persist, unfinished statements continue on `>>` lines, line editing and history, output above the prompt; `:server`/`:client` pick the side); `rook run-headless --repl` does the same without a window. It is the `repl` Cargo feature, on by default; `--no-repl` turns the prompt off.
- Fixed: engine functions that return nothing (`print`, `Destroy`, ...) returned one `nil` since the faster engine-function shim, which changed `select("#", ...)`.
- BodyMovers and constraints (M3 step 4), from Studio measurements: BodyForce, BodyThrust, BodyVelocity, BodyAngularVelocity, BodyPosition, BodyGyro and RocketPropulsion (`Fire`, `Abort`, `ReachedTarget`) push their part's whole assembly every 240 Hz step. BodyVelocity and BodyAngularVelocity are velocity constraints limited only by MaxForce/MaxTorque; BodyPosition and BodyGyro are mass-independent spring-dampers (BodyGyro at a quarter strength, on the sine of the angle, with MaxTorque limiting angular acceleration).
- RotateV and RotateP are driven by their surface's `SurfaceInput`, `ParamA` and `ParamB` (RotateV turns at 31 rad/s per unit; RotateP steps 0.01 rad per step toward its angle; NoInput holds); Rotate stays free.
- HingeConstraint (free, Motor, Servo, limits, `CurrentAngle`), BallSocketConstraint, RopeConstraint and RodConstraint become Rapier joints between attachments; SpringConstraint, VectorForce and Torque apply forces.
- Fixed: the deprecated `Velocity` and `RotVelocity` were stored apart from the velocities physics uses, so scripts launching parts with them (a small place's volcano, meteors, tornado) did nothing. They're now views of `AssemblyLinearVelocity`/`AssemblyAngularVelocity` (`Velocity` reads the assembly's velocity at the part), velocities saved in files are adopted at load, and setting any of them sets the whole assembly's. Anchored parts keep their velocity and carry what rests on them, like Roblox's conveyors.
- Fixed: `MakeJoints` treated wedges as boxes, so parts resting on a wedge's slope never joined, and a small place's barn lost its roof as soon as the map was unanchored. A wedge's slope is now a face and its box's top and front aren't, following Studio: a wedge calling MakeJoints joins what's on its slope only through its own Weld surfaces (TopSurface, then FrontSurface); a part calling it sees the wedge's FrontSurface there; rotating surfaces on a slope make nothing.
- Physics conformance cases `physics/movers`, `physics/constraints` and `physics/velocity`. `rook conformance record` picks the Studio window whose place name contains `ROOK_STUDIO` when several are open.
- `ROADMAP.md`: the one place for the plan. Content (meshes, textures, decals) moves ahead of characters, which need the head mesh; then GUI, audio and networking. Other docs name milestones instead of numbering them.
- Fixed: RotateV had unlimited torque and Rapier capped every body's spin at a quarter turn per step, so a helicopter rotor (a small place's fort) slammed to full speed in one step and flung the whole helicopter. RotateV's torque is now limited by the smaller part's size, as measured in Studio, so heavy rotors spin up over seconds, and parts can spin as fast as Roblox lets them (1,550 rad/s at ParamB 50).
- Tracy zones per resumed script (`script: <chunk>:<line>`), per `Instance.__index`/`__newindex` (key as zone text), per native Instance method, and around all engine functions (`api`).
- Scene queries (M3 step 5), on Rapier's query pipeline and measured in Studio: `FindPartOnRay` (and `findPartOnRay`), `FindPartOnRayWithIgnoreList`, `FindPartOnRayWithWhitelist`, `FindPartsInRegion3` (and `findPartsInRegion3`), `FindPartsInRegion3WithIgnoreList`, `FindPartsInRegion3WithWhiteList`, `IsRegion3Empty`, `IsRegion3EmptyWithIgnoreList`, `Raycast`, `Blockcast`, `Spherecast`, `GetPartBoundsInBox`, `GetPartBoundsInRadius`, `GetPartsInPart`, `BasePart:GetTouchingParts` and `GetNoCollisionConstraints`. Rays reach 15,000 studs and ignore parts they start in; `CanQuery` only counts with `CanCollide` off; legacy rays act as the `Default` collision group and regions ignore groups; regions and `GetPartBounds*` test each part's `Size` box, `GetPartsInPart` and `GetTouchingParts` real shapes. Queries see parts created or moved earlier in the same frame.
- New datatypes `RaycastParams`, `OverlapParams` and `RaycastResult` (`FilterType` with the old Whitelist/Blacklist names, nested filter tables, `AddToFilter`, `CollisionGroup`, `RespectCanCollide`, `MaxParts`), with Studio's messages, which reach `pcall` as plain strings.
- Camera: `ViewportPointToRay`, `ScreenPointToRay`, `WorldToViewportPoint` and `WorldToScreenPoint`; the client's camera gets the window size as `ViewportSize`.
- Clients have a physics world too, built on their first query and never stepped, so LocalScripts can raycast. The mouse (`Mouse.Hit`, `Target`, `TargetFilter`) and ClickDetectors use the same queries, and `rook-engine`'s separate ray geometry (`raycast.rs`) is gone: part shapes are defined once, in `rook-physics`.
- Physics conformance case `physics/queries`.
- Built-in mesh shapes (M4 step 1), measured in Studio: parts with a SpecialMesh, BlockMesh or CylinderMesh draw the mesh's shape instead of their own. Brick, Torso and BlockMesh draw a block, Wedge and CornerWedge their wedges, Sphere a ball stretched on every axis, SpecialMesh Cylinder a cylinder along X and CylinderMesh one standing along Y; the deprecated Prism, Pyramid, ParallelRamp and RightAngleRamp draw nothing. `Scale` multiplies the part's Size before each type's rule, and `Offset` moves the mesh in the part's axes. Of several meshes, the last child counts; a file mesh with no `MeshId` is a 1-stud box times `Scale`. Physics and queries keep the part's own shape. The size rules live once in `rook-types` (`MeshKind`, `MeshLook`).
- Faster physics rebuilds and scripting, measured on a 2017 place's sequence ( forced through the place's own round code): server+client step early in the sequence 28.2 ms → 13.3 ms per frame, late in it 22.0 ms → 18.2 ms; the windowed player runs the sequence at 0.88-0.98 game seconds per real second instead of about 0.68; the 70 s before it run in 2.1 s instead of 3.2 s. Every change except the in-place collider updates below leaves output identical (console logs, a per-10-second digest of every part's position, velocity and material, and a mid-sequence screenshot); every conformance case passes. A large modern place's 10 s run drops from 26.7 s to 25.5 s, with identical output.
  - A physics rebuild walks each cluster of unanchored parts once. It used to walk the whole cluster once per joint into it from every anchored part, so recolouring one part of a welded map cost a whole-map walk per joint (50-85 ms each, up to 580 ms in a frame).
  - Scripts cache Instance methods per class in Luau, so `part:FindFirstChild(...)` no longer crosses into Rust to fetch the method; reads of keys that are no class's method skip that cache. `inst.Key` reads its key without copying it, fetches the engine context once, and its shim runs `type()` on Luau's fast path (an environment marked `safeenv` that holds only the builtins it uses).
  - Changing a part's `Material`, `CustomPhysicalProperties` or `CanTouch` updates its collider in place instead of rebuilding its assembly, and wakes the part and what touches it (observed in Studio 0.742: a part at rest on a 30° ramp slides as soon as its friction drops). Unlike a rebuild, this keeps the simulation's contact state, so long runs differ from before in the details of how piles of parts settle.
  - Engine-internal maps keyed by ids and names (reflection lookups, instance properties, defaults, referrers, signal connections, member tables) use a fast deterministic hash (`rustc-hash`).
- The Head MeshType draws Rook's own head mesh, built in code to match the vertex layout measured from the user's Roblox install (36 segments, rounded rims in four steps, smooth shading), fitted to the part as Roblox fits it. No Roblox file or data is included.
- Changing a mesh (its type, `Scale` or any property, or adding, removing or moving one) redraws its part.
- Asset fetching (M4 step 2), new crate `rook-content`. Content references are parsed once (`ContentRef`) by the rules Studio follows, checked form by form: `rbxasset://` paths, `rbxassetid://` ids and legacy asset URLs on any host, with their case, slash, spacing and `version=` variants; anything else is no content. They resolve through a configurable chain: Rook's defaults, the user's Roblox install (found automatically, newest first, or `ROOK_ROBLOX_INSTALL`), content folders (`ROOK_CONTENT_DIRS`) and Roblox's asset delivery (anonymous; an Open Cloud key from `ROOK_OPEN_CLOUD_API_KEY` for assets refused anonymously; `ROOK_ASSET_DELIVERY=off` turns the network off). Each answer says where it came from.
- 262 old `rbxasset://` paths of the 2009 client (`fonts/sword.mesh`, `sounds/collide.wav`, `textures/SpawnLocation.png`), which archived places use and modern installs lack, resolve to the public assets Roblox uploaded for them, as they do in Studio.
- A content-addressed, hash-checked disk cache outside the repository (`ROOK_CACHE_DIR`): a second run needs no network, damaged files are fetched again, and "not found" or "forbidden" answers are remembered for a day.
- `ContentProvider:PreloadAsync` and `RequestQueueSize`, from Studio observations: on a client, entries resolve on worker threads and callbacks run on the engine thread as each finishes, then the call returns; on a server it never returns once there is something to fetch, as in Studio. Conformance case `content/preload`.
- `rook content <reference>` shows how a reference resolves (provider, file or URL, size, hash, file type; for meshes, version, triangles and size); `rook content --place <file>` does it for every content property in a place and totals the results by provider and failure. `rook corpus-scan` reports content properties, URL shapes and what `rook-content` makes of them.
- The player draws the Head MeshType with the `head.mesh` from the user's Roblox install when there is one (decoded by `rook-content` through the `rbx_mesh` crate, which reads mesh versions 1.00 to 5.00), and Rook's own head otherwise.
- Fixed: places store their content in the `Content` properties (`MeshContent`, `TextureContent`, `AudioContent`, ...), so `MeshId`, `Texture`, `SoundId` and the other string properties read empty in scripts, and every file mesh from a file drew as a 1-stud box. They're now views of the `Content` properties, and setting one turns backslashes in `rbxasset://` paths into slashes, as Studio does.
- Rook now needs Rust 1.94 or later (for `rbx_mesh`; the code already needed 1.88 for `if let` chains, though 1.85 was declared).
- Rendering uses reverse Z with an infinitely far plane, so distant surfaces no longer z-fight from depth-buffer imprecision (with the old 0.1–10,000 stud range, depth resolved only to about 0.15 studs at 500 studs away).

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
