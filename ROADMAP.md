# Roadmap

This is the one place Rook's plan lives. The README, `docs/design/architecture.md`
and `docs/behavior/stubs.md` refer to milestones by name and link here.
What each step found in Studio and the archive is in
[`docs/behavior/`](docs/behavior/), linked from the step.

## How the order is decided

1. **Visible progress on archived places first.** Rook's first job is to
   open an archived place and show it as Roblox would.
2. **Then by what the archive uses.** Counts below come from the corpus
   scan (`reports/corpus/`): 3,056 place and model files that load.
3. **Dependencies.** A milestone that needs another's work comes after it
   (characters need physics and the head mesh).

Each milestone is a vertical slice with a measurable exit criterion, and
each release says which milestone it finishes (see `CHANGELOG.md`).

## Now, next, later

- **Now:** M4 Content: the built-in mesh shapes, asset fetching, file meshes, textures and decals and trusses are done; unions are next.
- **Next:** M5 Characters.
- **Later:** M6 GUI, M7 Audio and M8 Networking.

### Rook extensions (unscheduled)

Opt-in features beyond what Roblox allows, behind an explicit profile and off by default.
None are scheduled; each comes after the milestone it depends on.

- WebSockets available to any script (after M8 Networking)
- EditableImage and EditableMesh without platform gating (after M4 Content)
- Restoring embedded legacy textures that modern Roblox loads as empty

## Milestones

| | Milestone | Exit criterion |
|---|---|---|
| ✅ | **M0** File formats, datatypes, reflection, CLI | All test fixtures round-trip; 99.3% of the archive loads (v0.1.0) |
| ✅ | **M1** Instance tree, signals, scheduler, Luau, server Scripts, conformance | First conformance batch passes; baseline of archive scripts running (v0.1.0) |
| ✅ | **M2** Renderer v0, client/server split, LocalScripts, basic input | Archived places render recognisably; client scripts run alongside server scripts (v0.2.0) |
| ✅ | **M3** Physics | Physics conformance batch passes; unanchored places settle plausibly |
| ⬜ | **M4** Content: meshes, textures, decals | Archived places render with their meshes, decals and textures |
| ⬜ | **M5** Characters | Walk around and use tools in an archived place |
| ⬜ | **M6** GUI | GUI conformance batch passes |
| ⬜ | **M7** Audio | Archived places play their sounds |
| ⬜ | **M8** Networking: transport, headless server, multiple clients | Two clients play together |
| ⬜ | **M9+** Terrain, animation, effects, advanced lighting, Studio tooling | — |

### M3 Physics

Rapier underneath, Roblox's rules on top, every rule checked in Studio.

1. ✅ Rapier world, anchoring, write-back.
2. ✅ Joints and assemblies (welds, Snaps, Motor6D, legacy surface joints, `MakeJoints`).
3. ✅ Contacts, `Touched`, collision groups, materials.
4. ✅ BodyMovers, driven hinges (RotateV, RotateP), the most-used constraints, `Velocity` and conveyors.
5. ✅ Queries on Rapier, legacy first, by files using them: `FindPartOnRay` (205), `FindPartsInRegion3` (135), `FindPartOnRayWithIgnoreList` (123), `GetTouchingParts` (27), `FindPartOnRayWithWhitelist` (15), `Workspace:Raycast` with `RaycastParams`/`RaycastResult` (15), `FindPartsInRegion3WithIgnoreList`/`WithWhiteList` (17), `GetPartBoundsInBox`, `GetPartsInPart` and `GetPartBoundsInRadius` with `OverlapParams` (12), `Blockcast` and `Spherecast` (8); also Camera `ScreenPointToRay`/`ViewportPointToRay` (51), which scripts pair with raycasts. The mouse and ClickDetectors moved over, and `crates/rook-engine/src/raycast.rs`'s own geometry is gone. Clients got a physics world that only answers queries.

M3's exit criterion is met: the physics conformance batch (`physics/*`, six cases) passes, and unanchored places settle plausibly.

### M4 Content: meshes, textures, decals

The most visible gap: **SpecialMesh is in 65% of files** (1,988 files,
494k instances), CylinderMesh in 723 and BlockMesh in 574. Today the
renderer draws all of these as their part's plain box.

1. ✅ **Built-in mesh shapes**, no downloads needed: SpecialMesh's `MeshType`
   shapes, BlockMesh and CylinderMesh, with `Scale` and `Offset`
   (`VertexColor` waits for textures). Every type but Head reuses a shape
   Rook already has (see [meshes](docs/behavior/meshes.md#built-in-shapes-step-1)); the size rules are defined once
   in `rook-types` (`MeshKind`, `MeshLook`). Head is a mesh file in Roblox,
   so Rook draws its own head mesh, built in `rook-render` from
   measurements of the user's copy, which is never committed (see "Content
   and assets" below); from step 2 on, the user's install can supply the
   real file instead.
2. ✅ **Asset fetching**: `rbxasset://`, `rbxassetid://` and legacy asset
   URLs through the provider chain (Rook defaults, the user's Roblox
   install, content folders, asset delivery), cached and hash-checked, in
   the new `rook-content` crate; `ContentProvider:PreloadAsync`; `rook
   content`. Findings and corpus counts: [content.md](docs/behavior/content.md).
3. ✅ **File meshes**: SpecialMesh `MeshId`, FileMesh and MeshPart, Roblox
   mesh versions 1.00–5.00, loaded in the background and drawn at Studio's
   sizes. Counts and findings: [meshes.md](docs/behavior/meshes.md#file-meshes-step-3).
4. ✅ **Textures and decals**: images decoded in the background (PNG, JPEG,
   BMP, DDS), Decals and Textures on every part shape and mesh, mesh
   `TextureId` and MeshPart `TextureID` with `VertexColor`, Plastic's surface
   patterns, Neon and Reflectance, drawn as Studio draws them. Counts and
   findings: [textures-decals.md](docs/behavior/textures-decals.md).
5. **Unions**: draw UnionOperations with their own geometry instead of
   their bounding box (`UnionOperation.Content`, the union's geometry
   asset, is set on 70,944 unions in 99 files; `NegateOperation` 6 in 2).
   The `rbx_mesh` crate Rook already uses decodes union geometry too: its
   `union_graphics` module reads the `MeshData` a union's asset holds
   (versions 2, 4 and 5, plus the old `CSGK` stub), and `union_physics`
   reads `PhysicsData` (versions 3 to 8; version 8 needs zstd, for which
   the crate offers the pure-Rust `ruzstd`). Turning on those features
   adds no new dependency. First count which forms the archive uses
   (geometry in the file or behind an asset id, and whether those assets
   load anonymously), then decode through `rook_content::mesh` and draw
   through step 3's mesh loader and GPU cache. Check in Studio how a
   union's geometry is sized against its Size (unions can be resized),
   `UsePartColor`, and what a union draws while loading or after failing.
   Unions' collision hulls from `PhysicsData` go with MeshPart collision
   fidelity (below).

Physics is to pick up the real shapes and volumes of MeshParts
(collision fidelity) and unions (`PhysicsData`), replacing today's boxes;
it wasn't done in step 3.

### M5 Characters

Needs M3 (physics) and M4 (the head and body meshes). Humanoid 903 files,
Tool 1,150, SpawnLocation 1,507, Seat 640.

- Spawning, Humanoid (state machine, walking, jumping, climbing, seats,
  health and death, joint breaking), tools.
- Default camera and control scripts (Core-Scripts, Apache-2.0) and Rook's
  own Animate.
- Network ownership of a player's character, so its client simulates it.

### M6 GUI

ScreenGui 598 files, SurfaceGui 138, BillboardGui 227. Layout (`UDim2`,
`AnchorPoint`, `AutomaticSize`, the UI layouts and constraints), text
(cosmic-text), images, `ClipsDescendants`, ScrollingFrame and GUI input.

### M7 Audio

Sound is in 1,282 files. Playback, `TimePosition`, `Ended`/`Loaded`,
3D sound, SoundService and SoundGroups. Today Sound only keeps its state.

### M8 Networking

A real transport behind the in-process replicator's interface, a headless
server and several clients.

### M9+ Later

Terrain (including legacy `ClusterGrid` data; 975 files), animation
(Animation 269 files), effects (Fire 255, Smoke 249, Sparkles 303,
ParticleEmitter 84), lights (PointLight 226, SpotLight 210) and advanced
lighting, antialiasing (MSAA by quality level, measured in
[`docs/behavior/antialiasing.md`](docs/behavior/antialiasing.md)), Studio
tooling, and opt-in extensions.

## Content and assets

Rook ships its own defaults. Files from Roblox's client (meshes, textures,
shaders) are never committed. They're used two ways: as a reference that
someone measures, with only the facts written down and Rook's own version
built from them; or loaded at runtime from the user's own Roblox install
or downloaded content packages.

## Out of scope or undecided

- **Behaviour modern Roblox no longer has** is out of scope unless added
  later as an opt-in extension. Old content works as far as modern Roblox
  still runs it.
- **HopperBin** (1,107 files): modern Roblox no longer runs it. Decide
  before M5 whether to follow modern behaviour or offer an extension.
- **Wire compatibility with real Roblox clients** is not a goal;
  replication is Rook to Rook.

What's partly done or accepted but ignored today is listed member by member
in [`docs/behavior/stubs.md`](docs/behavior/stubs.md).
