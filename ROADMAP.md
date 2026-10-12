# Roadmap

This is the one place Rook's plan lives. The README, `docs/architecture.md`
and `docs/behavior/stubs.md` refer to milestones by name and link here.

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

- **Now:** M4 Content, starting with the built-in mesh shapes.
- **Next:** M5 Characters.
- **Later:** M6 GUI, M7 Audio and M8 Networking.

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

1. **Built-in mesh shapes**, no downloads needed: SpecialMesh's `MeshType`
   shapes (Brick, Sphere, Cylinder, Wedge, Head, Torso, ...), BlockMesh
   and CylinderMesh, with `Scale`, `Offset` and `VertexColor`. Rook
   generates the shapes itself. The head is modelled to match Roblox's from
   measurements of an exported reference, which is never committed (see
   "Content and assets" below).
2. **Asset fetching**: `rbxasset://`, `rbxassetid://` and legacy asset URLs
   through the provider chain (Rook defaults, the user's Roblox install,
   downloaded content packages, asset delivery), cached and hash-checked.
3. **File meshes**: SpecialMesh `MeshId`, FileMesh, MeshPart (78 files,
   126k instances), Roblox mesh versions 1–5.
4. **Textures and decals**: `TextureId`, Decal (1,756 files), Texture (459),
   part materials.

Physics picks up the real shapes and volumes of MeshParts here too
(collision fidelity), replacing today's boxes.

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
lighting, Studio tooling, and opt-in extensions.

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
