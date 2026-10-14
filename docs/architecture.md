# Rook — Architecture Proposal

Status: **draft for review**, 2026-10-04. Nothing here is implemented yet. Background and sources are in [research.md](research.md).

## 1. Goals that shape the architecture

1. **Modern Roblox is the single source of truth**, pinned to one API snapshot. Old content works to the extent modern Roblox runs it. There is no separate legacy mode.
2. **Instance-first.** Everything (files, scripts, physics, rendering, GUI) goes through one runtime DataModel. The pipeline for a Part is `file → instances → properties → hierarchy → change journal → physics / render / GUI → scripts → events`, never `file → cube`.
3. **Every class exists.** Every class in the reflection database can be created, parented, read, written, saved and scripted, even before Rook gives it any behavior.
4. **Verifiable.** Behavior is checked against values recorded from live Studio, and every behavior has a documented source.
5. **Extensions never leak into compatibility.** Rook-specific features sit behind an explicit profile.

## 2. Crate layout

```
crates/
  rook-types        Roblox datatypes (Vector3, CFrame, Color3, UDim2, BrickColor, sequences, enums, Value)
  rook-reflection   Class/property/enum/event database: pinned rbx_reflection_database + Rook overlay
  rook-dom          Runtime instance tree, property storage, change journal, class behavior registry
  rook-formats      rbxl/rbxm/rbxlx/rbxmx <-> rook-dom (via rbx_binary / rbx_xml / rbx_dom_weak)
  rook-scheduler    Frame pipeline, thread queues, wait/task, resumption points, time source
  rook-signal       RBXScriptSignal / connections, Immediate + Deferred dispatch, re-entrancy limit
  rook-lua          mlua/Luau bridge: Instance & datatype userdata, globals, script environments, require
  rook-content      Content URIs, providers (Rook defaults, local Roblox install, packages, asset delivery), cache, decoders
  rook-physics      Assemblies, joints, constraints, collisions, queries — Roblox semantics over Rapier
  rook-humanoid     Humanoid state machine and character controller
  rook-gui          GUI layout, text measurement, hit testing (engine-side; produces draw lists)
  rook-render       wgpu renderer: render world, parts/meshes/materials/lighting/sky/particles/GUI pass
  rook-audio        Sound / SoundGroup / SoundService over kira
  rook-input        Keyboard/mouse/gamepad/touch -> UserInputService / ContextActionService
  rook-net          Replication (in-process loopback first, network transport later)
  rook-engine       Assembles a DataModel: services, systems, run modes, headless host
apps/
  rook-cli          dump, roundtrip, run-headless, corpus-scan, conformance
  rook-player       windowed client (winit)
  rook-server       headless server
  rook-studio       editor (much later)
conformance/         Studio-recorded expectations + the Luau cases that produced them
docs/                research, architecture, behavior notes
```

Dependencies only point downward:

```
rook-types ← rook-reflection ← rook-dom ← rook-signal ← rook-scheduler ← rook-lua
                                    ↑            ↑
              rook-formats ────────┘            │
              rook-content ─────────────────────┤
              rook-physics, rook-humanoid, rook-gui, rook-audio, rook-input, rook-net
                                                 ↑
                                           rook-engine ← apps
                                                 ↓ (render snapshots over a channel)
                                           rook-render
```

`rook-render` never touches the DOM directly (see §8).

## 3. Datatypes (`rook-types`)

- Rook's own types, built on `glam`, with `From`/`TryFrom` conversions to `rbx_types::Variant` at the I/O boundary.
- `Value` is the dynamic property value: `Bool`, `Int`, `Float`, `String`, `Content`, `Vector3`, `CFrame`, `Color3`, `BrickColor`, `UDim`/`UDim2`, `Rect`, `NumberSequence`, `ColorSequence`, `NumberRange`, `PhysicalProperties`, `Font`, `Faces`/`Axes`, `EnumItem(EnumId, u32)`, `Ref(Option<InstanceId>)`, `Attributes`, `Tags`, and so on.
- Math semantics follow Luau (f64 `number`) where scripts can see the result. Engine-internal hot paths may use f32 where that's invisible. CFrame orthonormalisation, `lerp`, `ToEulerAnglesXYZ` and similar functions are verified against Studio values (§9).
- `lune-roblox` (MPL-2.0) is the reference for datatype edge cases. If any file is adapted from it, that file stays MPL.

## 4. Reflection (`rook-reflection`)

- Source: pinned `rbx_reflection_database`. It includes the class tree, members, tags (`Deprecated`, `NotCreatable`, `Service`, security levels) and per-class defaults.
- **Overlay data** (Rook-owned, in-repo TOML/JSON): which members Rook has implemented, how each property is stored (§5), behavior notes and provenance IDs, and extension classes/members marked `rook_extension = true`.
- At startup the database is compiled into dense IDs: `ClassId(u16)`, `PropertyId`, `EnumId`. Each class gets a **layout**: its full inherited property list, mapped to slots. Name lookups go through interned atoms and a per-class `(name → member)` cache.
- Updating the pinned snapshot is a deliberate change: bump the crate version, regenerate, review the diff of added and removed members, then run the conformance suite.

## 5. Runtime DOM (`rook-dom`)

### Why not use `rbx_dom_weak` at runtime

`WeakDom` stores each instance's properties as a string-keyed map of `Variant`s. That suits serialization, but the runtime needs fast typed access, change tracking, destroyed state, locked parents, attributes and tags, script identity, and derived properties (`Position` from `CFrame`). Rook converts at the I/O boundary only.

### Structure

- `InstanceId`: a generational arena key (`slotmap`). Stale IDs never alias new instances, and destroyed instances stay valid handles until collected.
- `InstanceData`: `class: ClassId`, `name`, `parent`, ordered `children` (`GetChildren` order), flags (`archivable`, `parent_locked`, `destroyed`), a property store, attributes, tags, and an optional behavior-state slot.
- **Property backing**, chosen per property in the overlay:
  - `Stored(slot)`: the value lives in the instance's slot vector. Unset slots read the class default. This is how every class works before it has behavior.
  - `Native(getter, setter)`: implemented by a behavior module, for derived or hot properties (`BasePart.Position`, `Orientation`, `CFrame`, GUI `AbsolutePosition`).
- **Class behavior registry**: `trait ClassBehavior` with hooks (`on_created`, `on_property_changed`, `on_ancestry_changed`, `on_destroying`) and method implementations. Classes with no registered behavior are **data-only**: still fully creatable, scriptable and serializable. Calling an unimplemented method raises a clear `"<Class>:<Method> is not implemented in Rook"` error and increments a counter that feeds the roadmap.
- **Hierarchy rules** follow Roblox: no cycles; locked parents (`Destroy`, services); `Parent` set last when cloning; `Destroy` locks the parent, disconnects connections and recurses; `Clone` respects `Archivable`; services are found or created by `GetService`.
- **Change journal**: every mutation appends an event (`Created`, `PropertyChanged(id, prop)`, `ChildAdded`, `ChildRemoved`, `AncestryChanged`, `Destroying`). Two kinds of consumers read it:
  1. the signal dispatcher (`Changed`, `GetPropertyChangedSignal`, `ChildAdded`, `DescendantAdded`, `AncestryChanged`, ...), and
  2. engine systems (physics, GUI, render sync, audio). Each keeps its own cursor and processes dirty sets at its phase. Systems never get callbacks in the middle of a mutation.
- **No global state.** Several DataModels can exist in one process (server + client for Play mode, Studio edit + play). Each has its own DOM, Lua VM, scheduler and physics world.

## 6. Signals and scheduler (`rook-signal`, `rook-scheduler`)

### Signals

- `RBXScriptSignal`: `Connect`, `Once`, `Wait`, `ConnectParallel` (stub until Actors exist), plus legacy `connect`/`wait` aliases. `RBXScriptConnection`: `Disconnect`, `Connected`.
- **Immediate** mode (what `SignalBehavior.Default` maps to today, and what old places use): handlers run synchronously at fire time on a new thread, with a re-entrancy depth limit of 10. This is **Confirmed** from creator-docs.
- **Deferred** mode: handlers are queued and run at resumption points. `Disconnect()` drops pending invocations; `Destroy()` disconnects but still runs pending ones. This is **Confirmed** from creator-docs.

### Scheduler

- One thread per DataModel. Its Luau, DOM and physics run on it. This matches Roblox's cooperative model, and it is where OpenRBLX went wrong. A server and its client are separate DataModels, so they may run on separate threads (§8).
- Frame pipeline. The order is an **Inference** from the creator-docs diagram, to be confirmed in Studio before it's locked in:

  ```
  input processing      → UserInputService events
  PreRender             → BindToRenderStep, camera  (client)
  [snapshot to renderer]
  replication receive
  legacy resumption     → wait(), spawn(), delay()
  PreAnimation
  animation + Humanoid step
  PreSimulation (Stepped)
  physics substeps      → (240 Hz believed, Unverified)
  PostSimulation
  Heartbeat; task.wait / task.delay resumption
  replication send
  ```

  Deferred-event drains happen at each documented resumption point.
- Queues: a min-heap of waiting threads by resume time; legacy `wait` (with its own minimum and throttling, behavior to be measured); `task.defer`/`task.spawn`; the deferred-event queue.
- **Time source abstraction**: real time (player) or fixed-step simulated time (headless runs and tests), so conformance runs are deterministic.

## 7. Luau bridge (`rook-lua`)

- One Luau VM per DataModel (`mlua` with `luau`, `vendored`; `luau-jit` later). Each `Script`/`LocalScript`/`ModuleScript` runs on its own thread with its own environment table, which falls back to the shared globals. `getfenv`/`setfenv` work as in Luau.
- **Instance userdata** wraps an `InstanceId`. A weak-valued registry keeps **one userdata per instance**, so `==`, `rawequal` and table keys behave like Roblox. `__index` checks properties, then methods, then children by name; each class's members are resolved once into a table by name, so a lookup is one hash probe. `__index` and `__newindex` skip the general engine-function shim (they run more than any other engine code). Destroyed instances still read.
- Datatypes are userdata with Roblox's operators, constructors and immutability.
- **Yielding**: a yielding engine function (`wait`, `task.wait`, `WaitForChild`, `Event:Wait`, `InvokeServer`, async HTTP) registers the current thread with the scheduler, then yields. That way a yield inside a `coroutine.wrap` behaves as on Roblox. The exact mlua mechanism (async functions vs. yield-with-values) gets prototyped in M1, and the observable behavior is pinned by conformance cases.
- **Security identities**: game scripts run at the normal game identity. Members tagged with a higher security in reflection raise the Roblox-style permission error.
- **Errors and output**: chunk names are the instance's full name, so messages read `Workspace.Script:3: ...`. Output goes through `LogService` with Roblox-style message types.
- `require(ModuleScript)`: caching per DataModel, cyclic-require error, and the "Requested module experienced an error while loading" message.

## 8. Engine systems

### Threading and data flow

Each DataModel's Luau, DOM and physics stay on one thread (§6). In singleplayer Play the player runs two DataModels on two threads, so the window waits only on its client and the renderer:

```
   server thread                                   main thread (window)
  ┌──────────────────────────────────┐            ┌─────────────────────────────────────┐
  │ server DataModel                 │  updates,  │ client DataModel                    │
  │  formats → DOM ⇄ Luau (scripts)  │  remotes   │  DOM ⇄ Luau (LocalScripts), input   │
  │   │ change journal               │ ─────────→ │   │ change journal                  │
  │   ├─→ physics (Rapier) ──┐       │            │   ├─→ queries (mouse, raycasts)     │
  │   └─→ replication source │       │ ←───────── │   └─→ scene cache ──→ renderer      │
  │ steps at 60 Hz of real time      │  remotes,  │ steps at 60 Hz; draws as often as   │
  └──────────────────────────│───────┘  clicks    │ the display allows (or uncapped)    │
                     physics workers (half the     └─────────────────────────────────────┘
                     cores: collision detection
                     and the solver, in parallel)
```

- The ends are `rook-engine`'s `ServerHost` and `ClientHost`, which only exchange messages (`host::ToClient`, `host::ToServer`) with Rook-to-Rook replication: the server's replication `Source` describes each change as a self-contained update (its current values, a new subtree in full) and the client's `Sink` maps server ids to its own instances and applies it. Remote events, invokes and ClickDetector clicks travel the same way, with instance ids translated by the client.
- The server's DataModel is made on its thread and never leaves it (a Luau VM isn't `Send`). It steps by itself at 60 Hz in live play, and only when told to for `--seconds`, `--bench` and screenshots, which keep the client in lockstep so results are the same every run. `Session` runs both ends on one thread, delivering every message at once (headless runs, tests, conformance).
- Physics parallelises inside a step on a shared pool of worker threads, half the logical cores. Rapier lays the work out the same whatever the pool's size, so results don't depend on it.
- Run mode (no player) keeps the server on the main thread, since the window draws it.

Scripts read some derived values synchronously: `AbsolutePosition`, `AbsoluteSize`, `TextBounds`, `TextFits`, raycasts, `GetPartsInPart`. So **GUI layout, text measurement and physics queries live on their DataModel's thread**. The renderer only consumes the client's scene, so no tree is ever shared between threads behind a lock.

### Physics (`rook-physics`, `rook-humanoid`)

- **Assemblies**: parts connected by rigid joints (`Weld`, `WeldConstraint`, legacy surface joints, and `Motor6D` with its `Transform` applied as a kinematic offset) form one assembly. Each assembly maps to one Rapier rigid body with compound colliders. Each anchored part roots its own assembly (a fixed body), so anchored parts joined to each other stay separate. Root-part selection follows Roblox's documented rules. Assemblies are rebuilt incrementally from the journal. A rebuild finds the assemblies of all the parts it touches in one pass, walking each cluster of unanchored parts once (`assembly::Clusters`): a welded map joined to its anchored parts through dozens of joints is one cluster, and walking it once per joint made one changed part cost a whole-map walk per joint. A change to a part's `Material`, `CustomPhysicalProperties` or `CanTouch` doesn't rebuild anything: the part's collider gets its new mass, friction, elasticity and touchability in place, and its body and what touches it wake up, since a resting part reacts at once in Studio. `CanCollide` and `CollisionGroup` still rebuild, because they change which colliders touch and so which touch events fire. The joint and assembly rules read only the DOM (finding a part's joints through its reverse references), so what scripts see happens synchronously: a `CFrame` write moves the whole assembly, and `GetConnectedParts` and `AssemblyRootPart` are always current.
- Non-rigid constraints map to Rapier joints or force controllers. Joints: legacy hinges (Rotate, RotateV, RotateP) and HingeConstraint are revolute joints whose motors are set once per frame from their surface inputs or actuator; BallSocket is a spherical joint; Rope and Rod are distance limits. Force controllers (`movers.rs`): the legacy BodyMovers, RocketPropulsion, SpringConstraint, VectorForce and Torque are read from the DOM when they or their attachments change (and after any rebuild), then set each body's forces before every 240 Hz step from its current velocity, so stiff controllers (BodyPosition's defaults) are solved implicitly and stay stable. Rocket `Fire`/`Abort` reach physics as commands queued on the scripting world.
- Shapes: Block → cuboid; Ball → sphere; Cylinder → cylinder along the part's **X** axis; Wedge/CornerWedge → convex hulls; MeshPart → box, hull or decomposition according to `CollisionFidelity`.
- `CanCollide`/`CanTouch`/`CanQuery`, collision groups (Rapier solver groups, so groups that don't collide still touch unless `TouchesUseCollisionGroups` is on), and `Touched`/`TouchEnded` from contact events. Friction and elasticity mix by their weights in a Rapier contact hook, as measured in Studio.
- **Queries** (legacy `FindPartOnRay`/`FindPartsInRegion3` and their variants, `Raycast`, `Blockcast`, `Spherecast`, `GetPartBoundsInBox`/`InRadius`, `GetPartsInPart`, `GetTouchingParts`, and the mouse) run on Rapier's query pipeline over the simulation's own colliders, so part shapes are defined once (`rook-physics/src/shape.rs`). Scripts often create or move a part and query in the same frame, so each query first syncs the world with the DOM's journal and puts the colliders of bodies moved since the last step in place, broad phase included; filters (`CanQuery`, ignore lists, groups) are read from the DOM per candidate. The physics world therefore lives in the scripting `World`, next to the DOM. Every DataModel has one: servers step it; a client's is built on its first query and never steps (it answers LocalScripts' and the mouse's queries from replicated positions) until network ownership arrives.
- **Humanoid** is its own system: state machine, floor detection, HipHeight, WalkSpeed, JumpPower/JumpHeight, climbing, seats, health and death, and joint breaking. It drives the root assembly. Every behavior is tagged Approximation until checked against Studio recordings.
- Rapier is the foundation, not the specification. Where Roblox's solver differs (friction combine, elasticity, joint stiffness, sleeping), Rook's semantics layer adjusts, and each difference gets a provenance note.

### GUI (`rook-gui`)

Built from creator-docs and Studio observation only. Covers LayerCollectors (ScreenGui, SurfaceGui, BillboardGui), `UDim2` + `AnchorPoint` + `AutomaticSize`, the UI layouts, padding and constraints, `ZIndexBehavior`, `ClipsDescendants`, ScrollingFrame, `TextScaled`/`TextWrapped`/`RichText` (cosmic-text), `ImageRectOffset`/`ScaleType`/`SliceCenter`, input hit-testing and GUI events. The output is draw lists for the renderer.

### Rendering (`rook-render`)

- Render proxies per visual instance (parts, MeshParts, SpecialMeshes, Decals/Textures, lights, particles, beams), built from snapshot deltas.
- Primitive meshes generated in code. Mesh and texture caches are fed by `rook-content`.
- Built-in mesh shapes (SpecialMesh `MeshType`, BlockMesh, CylinderMesh) reuse the part shapes: `rook-types` decides what a part's mesh draws and at what size (`MeshKind::look_on`, which also says which parts ignore mesh children), and the snapshot turns that into a draw shape (`DrawShape`: a part shape, the head or a mesh file), a size and, for the upright CylinderMesh, a turned rotation. The only new geometry is Rook's own head mesh. Meshes change only how a part looks; physics keeps the part's shape.
- File meshes (FileMesh, SpecialMesh FileMesh, MeshPart): `rook-types` gives their scale (`MeshLook::File`, `mesh_part_scale`), and the mesh's own origin sits at the part. The player's scene builder (`apps/rook-player/src/files.rs`) asks a `rook-content` `Loader` for a file the first time a part needs it (`Want::Mesh`, so the worker threads also decode it), polls finished files once per frame and redraws only the parts waiting for them. A frame never waits; a screenshot does. While a file loads, or after it fails, a part keeps what its mesh drew before (the old mesh, or the empty-`MeshId` box) and a new mesh draws nothing, as observed in Studio. The scene carries each mesh by the BLAKE3 hash of its bytes (`MeshHash`); the renderer uploads each file once and draws every part using it in one instanced batch, like the primitive shapes. No loader starts for a place without file meshes.
- **Trusses**: `rook-types` (`truss.rs`) holds the rules: how Size snaps (`truss_size`, used by `rook-lua`'s Size setter), the drawn shape (`TrussShape`: a Style and a segment count, a one-segment truss counting as NoSupports so it shares one mesh) and how it sits on the part (`TrussLook`: the turn onto the long axis and the offset to the box's low end). The snapshot gives the part `DrawShape::Truss` with that turn and offset in its `Pose`, so moving it only moves its instance; `rook-render` builds a shape's bars (`mesh::truss`) the first time a truss of that shape is drawn and draws every such truss in one instanced batch, so a truss costs one instance, whatever its length. Physics keeps the box. Files save `Style` as `style`, a separate property in the database; scripts' `Style` is a view of it (`SAME_AS` in `rook-lua`).
- **Images** (mesh `TextureId`, MeshPart `TextureID`, Decals, Textures) load the same way as mesh files, through the same `Files` and `Loader` (`Want::Image`: the workers decode to RGBA8 and build the mips). The scene carries each image by the BLAKE3 hash of its bytes (`ImageHash`, in `Scene::images`). A mesh texture is a property of its part's scene entry (`PartInstance::texture`, multiplied by `VertexColor`); each Decal and Texture is a scene entry of its own (`Layer::Image`), at its part's transform, kept in its part's list of layer indices so that parts and their images update in place together. What goes where follows `rook-types`: the face frames, the dominant-axis rule for round shapes and the tiling (`NormalId`, `Tiling`), which meshes take textures and follow UVs (`MeshKind::takes_texture`, `decals_follow_uvs`, `shows_surfaces`) and materials (`MaterialLook`); the shader reads the face frames from a uniform filled from `NormalId`. While an image loads a Decal draws nothing and a mesh is black; after a failure the Decal draws nothing and the mesh keeps the part's colour, as observed in Studio.
- **GPU cache** (`rook-render/src/cache.rs`): one scheme for mesh files and images, `GpuCache` keyed by content hash. The first entry that needs a file queues it; uploads go up at the start of a frame within a 24 MB budget, so hundreds of images arriving at once spread over frames, and entries waiting for an upload draw nothing until it is done (a screenshot uploads everything first). Files are kept for the renderer's life. Opaque parts are batched by (shape, image), images on parts by (`ZIndex`, shape, image) in a second set of batches, both with the in-place slots of `update_parts`; untextured draws bind a 1 × 1 white image. A place without images makes no image batches, requests and uploads nothing, and pays only the larger instance (128 bytes, from 80) and a branch in the shader.
- **Image layers** draw after the opaque parts and before the transparent ones, blended, testing depth with `GreaterEqual` and writing none: they reuse the part's own mesh and vertex maths (the clip position is `@invariant`), so they lie exactly on it. A layer projected onto a face keeps only the points whose part-space normal is closest to its face's axis and lie inside the face's box; a mesh-UV layer covers the whole mesh. Layers of a partly transparent part are sorted with the transparent parts, just after their own. Layers never enter the shadow pass.
- **Materials and surfaces:** `MaterialLook` decides: Plastic draws its faces' surface patterns (studs, the weld/glue lattice, inlets, universal), Neon is drawn unlit, the rest plain, Reflectance mixes towards the sky colour (Approximation). The patterns are a four-layer texture array: from the user's install (`textures/studs.dds`, handed over by the player at startup like `head.mesh`) or Rook's own drawn patterns. The 2022 materials' textures aren't in the install (Studio downloads them), so Rook draws those materials plain; nothing from the install is ever committed.
- Lighting: Lighting service properties (ClockTime → sun, Ambient/OutdoorAmbient, Brightness, Atmosphere, Sky, post effects). A forward renderer; Future (clustered) lighting comes later. The sun's direction follows `Lighting:GetSunDirection()` as measured in Studio (`sun_direction` in the player's snapshot).
- Sun shadows (`rook-render/src/shadow.rs`): before the main pass, a depth-only pass draws the opaque batches' existing instance buffers from the sun into one 2048² `Depth32Float` map, reverse Z like the main pass (compare Greater, clear to 0). It uses the main shader's own vertex maths (`world_position`, shared by `vs_main` and the `vs_shadow` entry point) and a pipeline layout without the shadow map's bind group, which the pass is writing. The orthographic box holds the smallest sphere around the camera's view out to 300 studs, so its size changes only with the field of view and aspect, and its centre moves in whole texels across the sun's view, so a texel's footprint stays put as the camera moves; it reaches 500 studs further towards the sun for tall casters off screen. Against acne the casters get a slope-scaled depth bias and receivers look up the map 1.5 texels out along their normal plus 0.05 studs towards the sun. The main pass multiplies only the sun term by 3×3 linear-filtered comparison samples, fading to lit over the last fifth of the distance. Each instance carries a cast flag (`BasePart.CastShadow`) that the shadow vertex shader uses to clip non-casters, so batches and their slots are unchanged; transparent parts live outside the opaque batches and never cast. The snapshot sets `Lighting::global_shadows`; with it off, or the sun below the horizon, no shadow pass is encoded.
- Every lighting and material formula is documented with its source and confidence.

### Content (`rook-content`)

- **References** are parsed once, into `ContentRef` (`rook-content/src/reference.rs`): `rbxasset://<path>` (a client file), an asset id with an optional version (`rbxassetid://`, and the legacy `http://www.roblox.com/asset/?id=` URLs in all the shapes Studio accepts), an asset version id, and `rbxthumb://` (parsed, not resolved). Anything else, including the empty string, is no content rather than an error. The accepted shapes follow Studio, checked form by form (ROADMAP M4 step 2). Files store content in the `Content`-typed properties (`MeshContent`, `TextureContent`, `AudioContent`); the legacy string properties scripts use (`MeshId`, `Texture`, `SoundId`) are views of them in `rook-lua`, and consumers read the `Content` ones (`Value::content_url`).
- **Providers**, tried in an order `ROOK_CONTENT_ORDER` can change (default `defaults,install,folders,delivery`):
  1. *Rook defaults* (`defaults.rs`): no files yet, and a table of the 2009 client's `rbxasset://` paths that now stand for public assets (`fonts/sword.mesh` → 12221720). Rook's head mesh is generated in `rook-render`, not shipped here.
  2. *The Roblox install*: the newest Studio or player version folder under `%LOCALAPPDATA%\Roblox\Versions` (and `Program Files`, and the macOS app bundles), or `ROOK_ROBLOX_INSTALL`; `rbxasset://` paths are looked up in its `content`, `ExtraContent` and `PlatformContent/pc` folders, ignoring case.
  3. *Content folders* (`ROOK_CONTENT_DIRS`): downloaded content packages or local overrides, laid out like `content`.
  4. *Asset delivery* (`delivery.rs`) for asset ids, through the cache: anonymous `assetdelivery.roblox.com/v2/assetId/<id>` requests, then, for assets refused anonymously, the Open Cloud endpoint with `ROOK_OPEN_CLOUD_API_KEY` if the user set one. Nothing else is ever sent anywhere, and no browser cookie or credential is read. `ROOK_ASSET_DELIVERY=on|off` controls the network; the player and `rook content` default to on, headless runs and tests to off.
- **Cache** (`cache.rs`), outside the repository, where the system keeps caches so the system may clear it too (`ROOK_CACHE_DIR`, else `%TEMP%\Rook\cache` on Windows, `~/Library/Caches/Rook`, `$XDG_CACHE_HOME/rook`): content-addressed blobs (BLAKE3) plus one small file per reference giving its blob, or the failure. Rook keeps it under `ROOK_CACHE_MAX` (2 GB) itself: reading a blob marks it used (its modified time, at most hourly), and each resolver opening the cache trims the least recently used blobs, and the references they leave, on a background thread. `rook cache` shows, prunes by age or clears it. A cache in the old Windows place (`%LOCALAPPDATA%\Rook\cache`) is moved once, file by file when both exist, leaving behind anything that fails to move for the next try. Files are written to a temporary name and renamed; blobs are checked against their size and hash when read and refetched if damaged. "Not found" and "forbidden" answers are kept for a day, so a place with hundreds of dead ids asks the network once a day at most; network errors are only remembered for the run. A second run needs no network.
- **Off the engine thread**: a `Loader` (one per DataModel, made on the first request) runs resolutions on four worker threads and shares one resolution between duplicate requests. The DataModel takes scripts' new requests and delivers finished ones once per frame, just before `wait()` resumption, so `PreloadAsync` callbacks run and calls return on the engine thread. A place that never asks for content starts no threads and pays nothing per frame. No async runtime: plain threads and channels.
- **Decoders** live with the content: `mesh.rs` reads Roblox mesh files through the `rbx_mesh` crate (versions 1.00 to 5.00) into one vertex and index layout, keeping only the highest level of detail; `image.rs` reads PNG, JPEG, BMP and DDS (DXT1/3/5; the install's `.tex` files are DDS) through the `image` crate, recognised by their bytes, into RGBA8 with a full mip chain (alpha-weighted, at most 1024 pixels a side). A `Loader` request can ask for a mesh or an image (`Want::Mesh`, `Want::Image`): the worker decodes it into `Fetched::mesh` or `Fetched::image`, and bytes that don't decode fail as `Invalid`. `Want::for_property` says what a content property must hold; PreloadAsync asks this way (a sound in a Decal's `Texture` fails, as in Studio), `rook content --place` totals the results, and the player asks for everything it draws. The player loads the install's `head.mesh` and `textures/studs.dds` at startup, from disk only, and hands them to `rook-render`, which otherwise draws its own.

### Services (first wave)

DataModel, Workspace, Players, Lighting, ReplicatedStorage, ReplicatedFirst, ServerScriptService, ServerStorage, StarterGui, StarterPack, StarterPlayer, Teams, RunService, UserInputService, ContextActionService, TweenService, Debris, CollectionService, HttpService (opt-in), ContentProvider, TextService, PhysicsService, LogService, SoundService, InsertService (limited). DataStoreService is backed by local files. That is an Approximation, and it's documented as one.

### Run modes and replication (`rook-net`)

- Modes: **Edit** (no scripts), **Run** (server simulation, no player), **Play** (server + client), **Server** (headless).
- Modern Play runs **separate server and client DataModels**. Rook does the same from M2 on, connected by an **in-process loopback replicator**. That makes `LocalScript` vs `Script`, RemoteEvents/RemoteFunctions and replication filtering correct early. A real network transport (BrikNet, quinn or renet, to be evaluated) plugs in later behind the same interface.
- Replication is Rook-to-Rook. Wire compatibility with real Roblox clients is not a goal.

## 9. Testing and measurement

1. **Unit tests** per crate.
2. **Format round-trip**: `rojo-rbx/rbx-test-files` plus local fixtures supplied via environment variables (never committed).
3. **Conformance suite** (`conformance/`): small Luau cases that log what they observe (event orderings, error messages, property coercion, scheduler order, and later GUI rects, CFrame math and physics positions). `rook conformance record` runs each case in a Studio playtest through Studio's built-in MCP server and commits the output as the expectation; `rook conformance run` runs the same cases in Rook and diffs them. See `conformance/README.md`.
4. **Corpus scanner** (`rook-cli corpus-scan <dir>`): parses every place (rbx formats) and script (`full_moon`), and reports class, member and enum usage frequency, plus "loads / runs N seconds without error / errors by cause". This drives priorities and is Rook's headline progress metric.
5. **Render golden images** (later): Rook frames compared against Studio screenshots with perceptual tolerance.

## 10. Behavior provenance

Each subsystem has `docs/behavior/<subsystem>.md` with numbered entries:

```
GUI-012  AnchorPoint offsets position by -AnchorPoint * AbsoluteSize before clipping.
         Source: creator-docs GuiObject.AnchorPoint; conformance/gui/anchor_point.json
         Status: Confirmed
```

Status is one of **Confirmed**, **Inference**, **Approximation** or **Rook extension**. Code that implements an entry cites its ID in a comment. This is both the clean-implementation record and the compatibility map.

## 11. Extensions

- Extension classes and members are marked in the reflection overlay and only exist when the engine runs with the `extensions` profile. The default profile is strict compatibility.
- Engine-side extensions (better lighting, debugging tools, performance options) are configuration flags that are off in strict mode.
- Saving a file that contains extension classes warns, because Studio can't read them back.
- Candidate extension: **restore legacy embedded assets.** Modern Roblox loads `<Content><binary>` data from old XML files as empty (Confirmed in Studio, 2026-10-05). Rook matches that by default, but the legacy upgrade keeps the data, tagged by object and property, so an opt-in extension could show those textures again.

## 12. Milestones

The milestones, their order and exit criteria are in [ROADMAP.md](../ROADMAP.md).

## 13. Open decisions

1. **HopperBin** (1,107 files): modern Roblox no longer runs it. Decide before Characters whether to follow modern behavior or offer it as an opt-in extension.
2. **`null` numbers in legacy XML** (3 values in the archive) are read as 0; verify against Studio.

Settled: license is Apache-2.0 with a `NOTICE` file crediting AsyncFinn. The repository is initialised, with a `.gitignore` that excludes Roblox files, caches, reports and credentials. The methods/events list is committed as a compact generated file inside `rook-reflection`.
