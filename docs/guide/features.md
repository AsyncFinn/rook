# Features

Everything Rook supports today. What's coming next is in [ROADMAP.md](../../ROADMAP.md), and every unfinished script member is listed in [behavior/stubs.md](../behavior/stubs.md).

## File formats

- Loads and saves **`.rbxl`, `.rbxm`, `.rbxlx` and `.rbxmx`**, binary and XML.
- **Legacy XML upgrade** for roughly 2006 to 2010 files that standard parsers reject: dropped `<External>` entries, `<hash>` content, control characters, MSVC float values like `1.#QNAN`. Files that already load normally are never rewritten.
- Legacy property names (`size` → `Size`) and the database's migrations are applied on load, the way Studio upgrades old files.
- Unknown properties are kept, so saving never silently drops data, and `rook roundtrip` proves it.
- A corrupt file produces an error, never a crash.

## Data model

- All the Roblox datatypes: `Vector2`, `Vector3`, `CFrame`, `Color3`, `BrickColor`, `UDim`, `UDim2`, `Rect`, `NumberSequence`, `ColorSequence`, `NumberRange`, `PhysicalProperties`, `RaycastParams`, `OverlapParams`, `RaycastResult`, `Font`, `Faces`, `Axes`, enums, attributes and tags.
- A class, property and enum database compiled to dense numeric IDs, with inherited property layouts and per-class defaults.
- A runtime instance tree with generational IDs, Roblox's hierarchy rules (locked parents, cycles, `Destroy`, `Clone` and `Archivable`) and a change journal that signals, replication and rendering each read at their own pace.
- Several DataModels in one process, each with its own tree, Luau VM and scheduler, so a server and its clients run side by side.

## Scripting

- **Luau** through `mlua`, one VM per DataModel, each script on its own thread with its own environment (`getfenv`/`setfenv` work).
- The Instance API: properties, methods, events, children by name, `GetService`, `FindFirstChild`, `WaitForChild` and the rest. One userdata per instance, so `==` and table keys behave as on Roblox.
- Roblox datatypes as userdata with their constructors, operators and methods.
- `RBXScriptSignal` with `Connect`, `Once`, `Wait` and the legacy lowercase aliases, in both **Immediate** and **Deferred** signal modes.
- A scheduler for `wait`, `delay`, `spawn` and the `task` library, with deterministic simulated time for headless runs and tests.
- `require` for ModuleScripts, with per-DataModel caching and Roblox's cyclic-require and load-error messages.
- Roblox-style error messages and chunk names (`Workspace.Script:3: ...`), output through `LogService`.
- Security levels: members tagged above game identity raise Roblox's permission error.
- Services including `RunService`, `TweenService` (and the legacy GUI tweens), `Players`, `Debris`, `PhysicsService`, `DataStoreService` (in memory for now) and `HttpService` (JSON and GUIDs; requests off).
- Unfinished members are either clear `"is not implemented in Rook yet"` errors or listed, inert stubs. Every stub is documented in [behavior/stubs.md](../behavior/stubs.md), and a test fails if one is added without an entry.

## Client and server

- **Play mode as on modern Roblox**: a server DataModel and separate client DataModels, connected by an in-process replicator.
- Replication follows Roblox's rules: `NotReplicated` classes and properties stay on the server, a `PlayerGui` reaches only its player, server `Script` source is withheld, references are remapped, attributes and tags replicate, and client changes stay on the client (FilteringEnabled).
- Joining creates the `Player`, `Backpack` and `PlayerGui` from `StarterPack` and `StarterGui`, fires `PlayerAdded`, sets `Players.LocalPlayer` and starts **LocalScripts** where Roblox allows them.
- `RemoteEvent`, `UnreliableRemoteEvent` and `RemoteFunction`: arguments are copied between VMs the way Roblox copies them, and invokes yield until the reply.

## Physics

- Workspace is simulated with **Rapier** on the server at 240 Hz, with `Workspace.Gravity` and `FallenPartsDestroyHeight`. Positions and velocities replicate to clients.
- **Joints and assemblies** as in Roblox: welds, glues, snaps, motors (`Motor`, `Motor6D`), `WeldConstraint` and the legacy hinges. `MakeJoints` and `BreakJoints` follow Studio's surface rules, and old places that saved no joints are joined by surfaces when they load.
- Every material's density, friction and elasticity, `CustomPhysicalProperties`, `Touched` and `TouchEnded`, and collision groups.
- The legacy BodyMovers (`BodyVelocity`, `BodyPosition`, `BodyGyro`, `RocketPropulsion` and the rest) and constraints (`HingeConstraint`, `BallSocketConstraint`, ropes, rods, springs, `VectorForce`, `Torque`), tuned from Studio measurements.
- **Scene queries**: `Raycast`, `Blockcast`, `Spherecast`, the overlap queries, the legacy `FindPartOnRay` and `FindPartsInRegion3` families, and the camera's ray and screen-point methods. LocalScripts can query too, and the mouse and ClickDetectors use the same queries.
- **Unions** collide and are hit by rays, the mouse and ClickDetectors as the shape Studio saved for them, not their box, so a button inside a union's doorway can be clicked. MeshParts still collide as their box.

## Rendering

- A **wgpu** renderer (Vulkan, Metal, DirectX 12) that draws snapshots of the scene and never touches the instance tree.
- Primitive parts (blocks, spheres, cylinders, wedges, corner wedges) with colour and transparency, the place's skybox (or the default sky from your Roblox install), sun, ambient light and fog.
- **Sun shadows** from opaque parts, out to 300 studs from the camera, with `Lighting.GlobalShadows` and `BasePart.CastShadow`. The sun's position matches Studio's for every time and latitude.
- The built-in mesh shapes (SpecialMesh types, BlockMesh, CylinderMesh) at the sizes Roblox draws them. The classic head comes from your Roblox install when there is one, and is Rook's own version otherwise.
- **Mesh files**: SpecialMesh and FileMesh `MeshId`s and MeshParts, in Roblox mesh versions 1.00 to 5.00, at the sizes Studio draws them, with their `TextureId` and `VertexColor`. They load in the background and appear when they arrive, as in Roblox; parts sharing a mesh share one copy on the GPU.
- **Trusses** as Studio draws them: AlternatingSupports, BridgeStyleSupports and NoSupports at any length and along any axis, each truss one instanced draw of a mesh shared by every truss of the same length and Style. A script setting a truss's Size gets Studio's snapped size.
- **Decals and Textures** on every part shape and mesh, placed as Studio places them: per face with `Color3`, `Transparency` and `ZIndex`, tiled by `StudsPerTile` and offsets, through a mesh's own UVs on file meshes and the classic head. Images (PNG, JPEG, BMP, DDS) load in the background; parts sharing one share one GPU texture.
- **Surfaces and materials**: studs, inlets, universal and weld surfaces on Plastic (from your Roblox install, or Rook's own), Neon and Reflectance. Every material is drawn with its textures, tinted by the part's Color at Studio's scale, from the 2022 set or the legacy set as the place's `MaterialService.Use2022Materials` says: Roblox's own textures, fetched like any other asset (no Studio needed). MaterialVariants (a part's `MaterialVariant`, or MaterialService's overrides) and SurfaceAppearance on MeshParts draw their own maps. Materials shade with their roughness and metalness: sun highlights, and the sky reflected as far as `Lighting.EnvironmentSpecularScale` says. Glass filters what's behind it, ForceField glows over it, and Neon glows past its colour.
- **Unions** with their own geometry, from the union's asset: sized to the union's Size, in their parts' colours or the union's Color by `UsePartColor`, with Decals, Textures and materials laid as Studio lays them. Union assets of 2015-2016 places load anonymously; newer ones need an account, and draw nothing without one.
- `rook-player` runs the place live in a window with a fly camera that drives `Workspace.CurrentCamera`, or renders a single frame to PNG offscreen.

- **Terrain**: Workspace's smooth terrain (`SmoothGrid`) is drawn with Roblox's own surface, the plain rounded shape of Grass, Sand or Snow, Brick and WoodPlanks as blocks, and Concrete and Pavement faceted, each material in its flat `Terrain.MaterialColors` colour (textures come next), water see-through in `WaterColor`. Legacy terrain (`ClusterGrid`) shows nothing, as in today's Studio. Nothing collides with terrain yet.

The sun and moon in the sky, terrain materials and other lighting (point and spot lights, Future lighting) are on the [roadmap](../../ROADMAP.md).

## Content

- Content references in every form places use, parsed by Studio's rules: `rbxasset://` paths, `rbxassetid://` ids and the legacy `http://www.roblox.com/asset/?id=` URLs with their case, slash and spacing variants.
- They resolve through a chain of providers: **your own Roblox install** (found automatically, newest version first), folders you point Rook at, and Roblox's **asset delivery** for asset ids, anonymous and easy to turn off, with an Open Cloud API key of your own (`ROOK_OPEN_CLOUD_API_KEY`) for assets that need an account. Old 2009 client paths such as `rbxasset://fonts/sword.mesh` resolve to the assets Roblox uploaded for them.
- Downloads go to a hash-checked disk cache in your system's cache folder (`%TEMP%` on Windows), so a second run needs no network, and missing or forbidden assets are remembered for a day. Rook keeps it under 2 GB by dropping what was used least recently (`ROOK_CACHE_MAX`), and `rook cache` shows or cleans it.
- `ContentProvider:PreloadAsync` and `RequestQueueSize`, resolved off the engine thread; a mesh's `MeshId` must hold a mesh and an image property an image to succeed, as in Studio.

## Audio

- Sounds play: WAV, Ogg Vorbis and MP3 files load through the same content providers as everything else and decode in the background, `IsLoaded` and `TimeLength` read as in Studio, and `TimePosition` moves while a sound plays, on servers and clients alike.
- The Sound API behaves as Studio's: `Play`, `Stop`, `Pause` and `Resume` in every state, where `Play` starts from, seeking, ends and loops, `PlaybackSpeed`, `Volume` and their limits, and the `Played`, `Paused`, `Resumed`, `Stopped`, `Ended`, `Loaded` and `DidLoop` events; `PlaybackLoudness` on clients and `PlayOnRemove` too.
- `rook-player` mixes every playing sound to your default output device (`ROOK_AUDIO=off` silences it); headless runs, `--bench` and `--screenshot` never open one. Sounds under a part or Attachment are heard from there, fading by their `RollOffMode` and panned left and right as Studio pans them, from the camera or the listener `SoundService:SetListener` chose.
- SoundGroups and sound effects: a Sound is heard through its `SoundGroup`, nested groups included, with each group's `Volume` and effects; the equalizer, distortion, reverb and pitch-shift effects play in the order their `Priority` gives, matched to Studio's levels, and `PlaybackLoudness` reads them as Studio does. `SoundService.AmbientReverb` adds its reverb to everything, and `SoundService:PlayLocalSound` plays a sound for one client.
- The audio API: AudioPlayers wired through Audio effects (fader, equalizer, distortion, pitch shifter, reverb, chorus, echo, compressor) to an AudioDeviceOutput are heard, and AudioEmitters to AudioListeners by Studio's distance and angle curves, with the camera's default listener; `Wire.Connected`, `GetAudibilityFor` and AudioAnalyzer's levels and spectrum read as in Studio.

## Tooling

- `rook dump`: print a place's instance tree and properties.
- `rook roundtrip`: load, save and reload a file and report every difference.
- `rook corpus-scan`: scan a folder of places for what loads and which classes, properties and script APIs they use.
- `rook run-headless` and `rook corpus-run`: run server scripts without a window and report what works.
- `rook content`: show where a content reference, or every reference in a place, comes from and why any fail.
- `rook conformance record` / `run`: record behaviour in Roblox Studio and check Rook against it.

Usage for each is in [tools.md](tools.md).
