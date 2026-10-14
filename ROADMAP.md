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

- **Now:** M4 Content: the built-in mesh shapes, asset fetching, file meshes, textures and decals and trusses are done; materials are next, then unions.
- **Next:** M5 Audio.
- **Then:** M6 Terrain, M7 Lights and effects, M8 GUI, M9 Characters and, right after, M10 Networking.

The world comes before the people in it: sound, terrain, lights and GUI
are each in hundreds of archive files, and characters need terrain to walk
on and GUI for their tools and health. Networking follows characters
directly, since a character is what makes playing together worth it.

## Milestones

| | Milestone | Exit criterion |
|---|---|---|
| ✅ | **M0** File formats, datatypes, reflection, CLI | All test fixtures round-trip; 99.3% of the archive loads (v0.1.0) |
| ✅ | **M1** Instance tree, signals, scheduler, Luau, server Scripts, conformance | First conformance batch passes; baseline of archive scripts running (v0.1.0) |
| ✅ | **M2** Renderer v0, client/server split, LocalScripts, basic input | Archived places render recognisably; client scripts run alongside server scripts (v0.2.0) |
| ✅ | **M3** Physics | Physics conformance batch passes; unanchored places settle plausibly |
| ⬜ | **M4** Content: meshes, materials, textures, decals | Archived places render with their meshes, materials, decals and textures |
| ⬜ | **M5** Audio | Archived places play their sounds, positioned in 3D; the audio conformance batch passes |
| ⬜ | **M6** Terrain | Archived places with terrain show it, and parts and queries collide with it |
| ⬜ | **M7** Lights and effects | Local lights light the scene; Fire, Smoke, Sparkles and ParticleEmitters draw as in Studio |
| ⬜ | **M8** GUI | GUI conformance batch passes; archived places' GUIs draw and take input |
| ⬜ | **M9** Characters | Spawn, walk, jump, sit and use tools in an archived place |
| ⬜ | **M10** Networking | Two players on two machines play an archived place together |
| ⬜ | **M11** Avatars and animation | Players pick an avatar (no Roblox account) and see each other's, dressed and animated |
| ⬜ | **M12** Server hosting | Host a place for friends over the internet; its saved data survives a restart |
| ⬜ | **M13** Lighting and post-processing | Screenshots of reference places match Studio's lighting, sky and effects |
| ⬜ | **M14** Scripting completeness | 90% of archive places with server scripts run without an error Rook causes |
| ⬜ | **M15** Physics completeness | Second physics conformance batch passes; MeshParts and unions collide with their real shapes |
| ⬜ | **M16** Every class the archive uses | No class in 5 or more archive files is still inert |
| ⬜ | **M17** Modern places | Reference places built with today's Studio (R15, layered clothing, PBR materials) play as they do in Studio |
| ⬜ | **M18** Offline content | An archived place plays on a machine with no network, from a content pack |
| ⬜ | **M19** Input and platforms | Play with a gamepad; Rook builds and runs on Windows, Linux and macOS |
| ⬜ | **M20** Hosting safety | A public server survives runaway scripts and a hostile client test suite |
| ⬜ | **M21** Performance and scale | The archive's largest places hold 60 FPS on a mid-range PC |
| ⬜ | **M22** Release | Download Rook, pick a place from a library and play, without a terminal |

### M3 Physics

Rapier underneath, Roblox's rules on top, every rule checked in Studio.

1. ✅ Rapier world, anchoring, write-back.
2. ✅ Joints and assemblies (welds, Snaps, Motor6D, legacy surface joints, `MakeJoints`).
3. ✅ Contacts, `Touched`, collision groups, materials.
4. ✅ BodyMovers, driven hinges (RotateV, RotateP), the most-used constraints, `Velocity` and conveyors.
5. ✅ Queries on Rapier, legacy first, by files using them: `FindPartOnRay` (205), `FindPartsInRegion3` (135), `FindPartOnRayWithIgnoreList` (123), `GetTouchingParts` (27), `FindPartOnRayWithWhitelist` (15), `Workspace:Raycast` with `RaycastParams`/`RaycastResult` (15), `FindPartsInRegion3WithIgnoreList`/`WithWhiteList` (17), `GetPartBoundsInBox`, `GetPartsInPart` and `GetPartBoundsInRadius` with `OverlapParams` (12), `Blockcast` and `Spherecast` (8); also Camera `ScreenPointToRay`/`ViewportPointToRay` (51), which scripts pair with raycasts. The mouse and ClickDetectors moved over, and `crates/rook-engine/src/raycast.rs`'s own geometry is gone. Clients got a physics world that only answers queries.

M3's exit criterion is met: the physics conformance batch (`physics/*`, six cases) passes, and unanchored places settle plausibly.

### M4 Content: meshes, materials, textures, decals

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
5. **Materials**: every material drawn as modern Roblox draws it; today
   only Plastic (its surface patterns), Neon and Reflectance are, and the
   rest are their plain colour. By files using them: Wood 531, Slate 502,
   Concrete 366, Grass 358, Ice 353, DiamondPlate 351, CorrodedMetal 252,
   SmoothPlastic 201, Foil 177, Brick 158, Sand 149, Granite 127, Marble
   125, Metal 110 (703,662 parts), Pebble 105, Fabric 101, WoodPlanks 51,
   Cobblestone 39, Glass 23, ForceField 18, and a few others. First find
   where Studio keeps the material textures it downloads at runtime (they
   aren't in the install), so Rook can load the user's copy the way it
   loads `studs.dds`; without them Rook draws its own, never Roblox's
   files committed. Measure in Studio for each material: texture scale in
   studs, how Color tints it, normal and roughness, how much it reflects
   (Metal, Foil, DiamondPlate), SmoothPlastic's lack of pattern, Glass
   (transparency and reflection) and ForceField's animated look, and which
   materials show surfaces (so far only Plastic does). MaterialVariant and
   MaterialService (10 files) come last.
6. **Unions**: draw UnionOperations with their own geometry instead of
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
   fidelity (M15).

Physics picks up the real shapes of MeshParts and unions in M15.

### M5 Audio

Sound is in 1,284 files (113,199 instances); today it only keeps its
state. It's self-contained, and every place sounds wrong without it.

1. Decoding (OGG, MP3, WAV) and playback through the content loader.
2. `Play`, `Stop`, `Pause`, `Resume`, `TimePosition`, `Looped`,
   `PlaybackSpeed`, `Volume`, and `Played`, `Ended`, `Loaded`, `IsLoaded`.
3. 3D sound: a Sound under a part or Attachment is positioned, with
   `RollOffMode` and its distances measured in Studio.
4. SoundService, SoundGroups (5 files) and sound effects, by archive use.

### M6 Terrain

977 files have a Terrain instance. First count how many hold voxels
(`SmoothGrid`) or legacy `ClusterGrid` blocks, and check what modern
Roblox does with the legacy data before deciding how Rook handles it.
Terrain comes before characters, who walk on it.

1. Decode `SmoothGrid` voxels (material and occupancy) and mesh them as
   Roblox does, measured against Studio.
2. Terrain materials and water (swimming comes with characters).
3. Collision and queries: parts land on terrain, rays hit it.
4. The Terrain API scripts use, by archive use (`FillBlock`,
   `ReadVoxels`, `WriteVoxels`, `FillBall`, ...).

### M7 Lights and effects

Lights are in hundreds of files and in great numbers: PointLight 228
files (35,041 instances), SpotLight 212 (50,397), SurfaceLight 65
(7,949). Effects: Sparkles 303 files, Fire 256, Smoke 251,
ParticleEmitter 86, Trail 17, Beam 15, plus the `Explosion`s scripts
create.

1. Local lights: Brightness, Range, Color, Shadows, SpotLight Angle and
   Face, SurfaceLight; many lights per frame.
2. Fire, Smoke and Sparkles, drawn as Studio draws them today.
3. ParticleEmitter, Beam and Trail.
4. Explosion: its blast (physics, joints breaking) and its look.

### M8 GUI

ScreenGui 600 files, SurfaceGui 140, BillboardGui 229; Frame 558,
TextLabel 550, ImageLabel 458, TextButton 456; Hint 145 and Message 26.
It comes before characters: tools, health bars and leaderboards are GUI.

1. Layout: `UDim2`, `AnchorPoint`, `ZIndex`, `ClipsDescendants`,
   `AutomaticSize`, the UI layouts and constraints.
2. Text (cosmic-text): fonts, sizing, wrapping, `TextScaled`.
3. Images, 9-slice, ScrollingFrame.
4. Input: buttons, `TextBox`, GUI taking input before the 3D world.
5. BillboardGui and SurfaceGui in the world; Hint and Message.
6. The GUI Roblox draws itself (player list, chat, backpack), from
   Core-Scripts (Apache-2.0) where they fit.

### M9 Characters

Humanoid 905 files, Tool 1,152, HopperBin 1,107, SpawnLocation 1,509,
Seat 642, VehicleSeat 230. Needs physics, the head mesh, terrain and GUI.

1. Spawning at SpawnLocations, the default R6 character, respawning.
2. Humanoid: state machine, walking, jumping, climbing, swimming, health
   and death, joints breaking.
3. Default camera and control scripts (Core-Scripts, Apache-2.0).
4. Tools and the backpack; HopperBin as decided below.
5. Seats and VehicleSeats.
6. Playing animations, enough for Rook's own Animate (Animation is in 270
   files; the rest of animation is M11).

### M10 Networking

Straight after characters: once there's a character, the next thing is
playing together. Replication already runs in-process between a server
and its client; this puts a real transport behind it.

1. A transport (UDP, reliable and unreliable channels) behind the
   replicator's interface; joining, leaving, timeouts.
2. Several clients, each with its own player and character.
3. Network ownership: a player's client simulates its character and what
   it owns (`SetNetworkOwner`).
4. RemoteEvents and RemoteFunctions across machines (114 and 73 files),
   with Roblox's limits and ordering.
5. Bandwidth: deltas and interest management, so large archived places
   stay playable.

### M11 Avatars and animation

How players look and move. BodyColors 359 files, Shirt 330, Hat 323,
Pants 282, CharacterMesh 168, ShirtGraphic 89, Accessory 42.

1. A local avatar (body colours, clothing, hats and faces from content
   folders or the user's install), set up without a Roblox account and
   sent to the server on joining.
2. Clothing and ShirtGraphics on R6 bodies, CharacterMeshes, hats and
   accessories, as Studio draws them.
3. Animation in full: Animator, AnimationController, KeyframeSequence,
   blending and priorities, replicated between players.

### M12 Server hosting

Rook's self-hosting promise: anyone can host a place for friends.

1. `rook-server`: a headless server for one place, with a config file
   (port, player limit, admins).
2. Joining by address, and finding servers on the local network.
3. DataStores saved to disk, so a server keeps its data between runs.
4. Teams (482 files) and team spawns, if characters didn't need them
   sooner.
5. Basic moderation: kicking, banning, server messages.

### M13 Lighting and post-processing

Make archived places look the way Studio shows them today.

1. The sun, moon and stars, `SkyboxOrientation`, Atmosphere (16 files)
   and Clouds (8).
2. Post-processing: Bloom (49 files), ColorCorrection (44), SunRays (20),
   Blur (13), DepthOfField (11).
3. Lighting tuned against Studio screenshots of reference places (it's an
   Approximation until then).
4. Antialiasing (MSAA by quality level, measured in
   [`docs/behavior/antialiasing.md`](docs/behavior/antialiasing.md)).

### M14 Scripting completeness

Driven by the corpus run (`reports/run/`): today 60.4% of places with
server scripts run 10 seconds without a script error. Many errors are the
scripts' own (old syntax modern Roblox rejects too, missing children);
the target counts only errors Rook causes, checked against Studio.

1. Classify the corpus run's errors: Rook's (unimplemented members,
   wrong behaviour) against the scripts' own, with a sample of each
   checked in Studio.
2. The unimplemented members by places affected, starting with
   `InsertService:LoadAsset` (54 places) and `require(assetId)` (17).
3. Services that talk to Roblox's website, answered locally and
   offline: MarketplaceService, BadgeService, GroupService, Players'
   web lookups.
4. TeleportService between places in one folder (multi-place games).
5. HttpService, opt-in per place.
6. Client scripts in the corpus run too, once characters and GUI exist.

### M15 Physics completeness

1. MeshPart and union collision with their real shapes (collision
   fidelity, `PhysicsData`), replacing boxes.
2. The remaining constraints by archive use (springs, ropes, rods,
   prismatic, the Align and Linear/AngularVelocity movers).
3. Sleeping and adaptive timestepping as Roblox does them.
4. A second physics conformance batch, recorded in Studio.

### M16 Every class the archive uses

Every class exists in Rook already; this makes the ones archived places
use actually do something, ranked by the corpus scan against
`docs/behavior/stubs.md`: SelectionBox and the other adornments (76
files), Flag and FlagStand (64), Dialog and DialogChoice (55),
LocalizationTable (20), SelectionPartLasso (11), CustomEvent and
CustomEventReceiver (10), SkateboardPlatform (8), ViewportFrame (5), and
whatever the scan ranks next once characters and GUI exist.

1. A corpus scan report of classes that are present but inert, by files.
2. The classes in order, each checked in Studio as before.
3. The HopperBin decision, carried out if it wasn't with characters.

### M17 Modern places

The archive is mostly older places, but modern Roblox is the target, and
places built in today's Studio use features old ones don't. Count each in
a set of modern reference places first.

1. R15 characters, HumanoidDescription (15 archive files) and modern
   accessories (42).
2. Layered clothing (WrapLayer, WrapTarget).
3. SurfaceAppearance (13 files) and MaterialVariant (10): PBR textures.
4. Highlight, modern GUI features, and whatever else the reference
   places use that the earlier milestones didn't cover.

### M18 Offline content

Preservation: a place should keep working after the assets it names
stop being downloadable.

1. Content packs: `rook content` exports everything a place or a folder
   of places needs into one pack, and Rook loads from packs before the
   network.
2. A pack format that's documented and checked (hashes, versions), so
   archives can share packs.
3. Reports of what a place still can't find, offline.

### M19 Input and platforms

1. Gamepads, and ContextActionService and UserInputService in full.
2. Rebindable controls and the keyboard and mouse settings Roblox has.
3. Linux and macOS builds, checked in CI.
4. Touch, if a platform that needs it is chosen.

### M20 Hosting safety

Self-hosted servers run places and players their host doesn't control.

1. Script limits on servers: memory, run time per frame, instance counts,
   with clear errors instead of a hung server.
2. Server authority: what a client may change, as Roblox's
   FilteringEnabled rules allow, tested against a hostile client.
3. HttpService and other outward features off unless the host turns
   them on.
4. Logs and crash reports a host can act on.

### M21 Performance and scale

1. A benchmark set: the archive's largest and heaviest places, timed in
   CI with `rook-player --bench`.
2. Level of detail and culling for huge part counts.
3. StreamingEnabled as Roblox does it, server and client.
4. Script performance where the corpus run shows it matters.

### M22 Release

Rook for people who just want to play, once everything above is in.

1. A launcher: a library of places from a folder, with thumbnails, and
   joining servers.
2. The in-game menu: settings (graphics quality, volume, controls),
   reset character, leave.
3. Packaged releases for each platform, and updating.

## Unscheduled

Good ideas without a place in the order yet. Each can only come after the
milestones it needs.

- **Studio**: `rook-studio`, a place editor (Explorer and Properties from
  the reflection database; moving, scaling and rotating parts; play
  testing; saving files Roblox Studio opens unchanged).
- **Rook extensions**: opt-in features beyond what Roblox allows, behind
  the explicit profile from the architecture (§11), off by default:
  - WebSockets available to any script (needs M10).
  - EditableImage and EditableMesh without platform gating.
  - Restoring embedded legacy textures that modern Roblox loads as empty.

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
  before M9 Characters whether to follow modern behaviour or offer an extension.
- **Wire compatibility with real Roblox clients** is not a goal;
  replication is Rook to Rook.

What's partly done or accepted but ignored today is listed member by member
in [`docs/behavior/stubs.md`](docs/behavior/stubs.md).
