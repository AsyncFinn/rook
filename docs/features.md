# Features

Everything Rook supports today. What's coming next is in [ROADMAP.md](../ROADMAP.md), and every unfinished script member is listed in [behavior/stubs.md](behavior/stubs.md).

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
- Unfinished members are either clear `"is not implemented in Rook yet"` errors or listed, inert stubs. Every stub is documented in [behavior/stubs.md](behavior/stubs.md), and a test fails if one is added without an entry.

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

## Rendering

- A **wgpu** renderer (Vulkan, Metal, DirectX 12) that draws snapshots of the scene and never touches the instance tree.
- Primitive parts (blocks, spheres, cylinders, wedges, corner wedges) with colour and transparency, a sky colour, sun, ambient light and fog.
- The built-in mesh shapes (SpecialMesh types, BlockMesh, CylinderMesh) at the sizes Roblox draws them, including Rook's own version of the classic head.
- `rook-player` runs the place live in a window with a fly camera that drives `Workspace.CurrentCamera`, or renders a single frame to PNG offscreen.

File meshes, textures, decals, terrain and shadows are on the [roadmap](../ROADMAP.md).

## Tooling

- `rook dump`: print a place's instance tree and properties.
- `rook roundtrip`: load, save and reload a file and report every difference.
- `rook corpus-scan`: scan a folder of places for what loads and which classes, properties and script APIs they use.
- `rook run-headless` and `rook corpus-run`: run server scripts without a window and report what works.
- `rook conformance record` / `run`: record behaviour in Roblox Studio and check Rook against it.

Usage for each is in [tools.md](tools.md).
