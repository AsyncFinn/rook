# Raven — Architecture Proposal

Status: **draft for review**, 2026-10-04. Nothing here is implemented yet. Background and sources are in [research.md](research.md).

## 1. Goals that shape the architecture

1. **Modern Roblox is the single source of truth**, pinned to one API snapshot. Old content works to the extent modern Roblox runs it. There is no separate legacy mode.
2. **Instance-first.** Everything (files, scripts, physics, rendering, GUI) goes through one runtime DataModel. The pipeline for a Part is `file → instances → properties → hierarchy → change journal → physics / render / GUI → scripts → events`, never `file → cube`.
3. **Every class exists.** Every class in the reflection database can be created, parented, read, written, saved and scripted, even before Raven gives it any behavior.
4. **Verifiable.** Behavior is checked against values recorded from live Studio, and every behavior has a documented source.
5. **Extensions never leak into compatibility.** Raven-specific features sit behind an explicit profile.

## 2. Crate layout

```
crates/
  raven-types        Roblox datatypes (Vector3, CFrame, Color3, UDim2, BrickColor, sequences, enums, Value)
  raven-reflection   Class/property/enum/event database: pinned rbx_reflection_database + Raven overlay
  raven-dom          Runtime instance tree, property storage, change journal, class behavior registry
  raven-formats      rbxl/rbxm/rbxlx/rbxmx <-> raven-dom (via rbx_binary / rbx_xml / rbx_dom_weak)
  raven-scheduler    Frame pipeline, thread queues, wait/task, resumption points, time source
  raven-signal       RBXScriptSignal / connections, Immediate + Deferred dispatch, re-entrancy limit
  raven-lua          mlua/Luau bridge: Instance & datatype userdata, globals, script environments, require
  raven-content      Content URIs, providers (Raven defaults, local Roblox install, packages, asset delivery), cache, decoders
  raven-physics      Assemblies, joints, constraints, collisions, queries — Roblox semantics over Rapier
  raven-humanoid     Humanoid state machine and character controller
  raven-gui          GUI layout, text measurement, hit testing (engine-side; produces draw lists)
  raven-render       wgpu renderer: render world, parts/meshes/materials/lighting/sky/particles/GUI pass
  raven-audio        Sound / SoundGroup / SoundService over kira
  raven-input        Keyboard/mouse/gamepad/touch -> UserInputService / ContextActionService
  raven-net          Replication (in-process loopback first, network transport later)
  raven-engine       Assembles a DataModel: services, systems, run modes, headless host
apps/
  raven-cli          dump, roundtrip, run-headless, corpus-scan, conformance
  raven-player       windowed client (winit)
  raven-server       headless server
  raven-studio       editor (much later)
conformance/         Studio-recorded expectations + the Luau cases that produced them
docs/                research, architecture, behavior notes
```

Dependencies only point downward:

```
raven-types ← raven-reflection ← raven-dom ← raven-signal ← raven-scheduler ← raven-lua
                                    ↑            ↑
              raven-formats ────────┘            │
              raven-content ─────────────────────┤
              raven-physics, raven-humanoid, raven-gui, raven-audio, raven-input, raven-net
                                                 ↑
                                           raven-engine ← apps
                                                 ↓ (render snapshots over a channel)
                                           raven-render
```

`raven-render` never touches the DOM directly (see §8).

## 3. Datatypes (`raven-types`)

- Raven's own types, built on `glam`, with `From`/`TryFrom` conversions to `rbx_types::Variant` at the I/O boundary.
- `Value` is the dynamic property value: `Bool`, `Int`, `Float`, `String`, `Content`, `Vector3`, `CFrame`, `Color3`, `BrickColor`, `UDim`/`UDim2`, `Rect`, `NumberSequence`, `ColorSequence`, `NumberRange`, `PhysicalProperties`, `Font`, `Faces`/`Axes`, `EnumItem(EnumId, u32)`, `Ref(Option<InstanceId>)`, `Attributes`, `Tags`, and so on.
- Math semantics follow Luau (f64 `number`) where scripts can see the result. Engine-internal hot paths may use f32 where that's invisible. CFrame orthonormalisation, `lerp`, `ToEulerAnglesXYZ` and similar functions are verified against Studio values (§9).
- `lune-roblox` (MPL-2.0) is the reference for datatype edge cases. If any file is adapted from it, that file stays MPL.

## 4. Reflection (`raven-reflection`)

- Source: pinned `rbx_reflection_database`. It includes the class tree, members, tags (`Deprecated`, `NotCreatable`, `Service`, security levels) and per-class defaults.
- **Overlay data** (Raven-owned, in-repo TOML/JSON): which members Raven has implemented, how each property is stored (§5), behavior notes and provenance IDs, and extension classes/members marked `raven_extension = true`.
- At startup the database is compiled into dense IDs: `ClassId(u16)`, `PropertyId`, `EnumId`. Each class gets a **layout**: its full inherited property list, mapped to slots. Name lookups go through interned atoms and a per-class `(name → member)` cache.
- Updating the pinned snapshot is a deliberate change: bump the crate version, regenerate, review the diff of added and removed members, then run the conformance suite.

## 5. Runtime DOM (`raven-dom`)

### Why not use `rbx_dom_weak` at runtime

`WeakDom` stores each instance's properties as a string-keyed map of `Variant`s. That suits serialization, but the runtime needs fast typed access, change tracking, destroyed state, locked parents, attributes and tags, script identity, and derived properties (`Position` from `CFrame`). Raven converts at the I/O boundary only.

### Structure

- `InstanceId`: a generational arena key (`slotmap`). Stale IDs never alias new instances, and destroyed instances stay valid handles until collected.
- `InstanceData`: `class: ClassId`, `name`, `parent`, ordered `children` (`GetChildren` order), flags (`archivable`, `parent_locked`, `destroyed`), a property store, attributes, tags, and an optional behavior-state slot.
- **Property backing**, chosen per property in the overlay:
  - `Stored(slot)`: the value lives in the instance's slot vector. Unset slots read the class default. This is how every class works before it has behavior.
  - `Native(getter, setter)`: implemented by a behavior module, for derived or hot properties (`BasePart.Position`, `Orientation`, `CFrame`, GUI `AbsolutePosition`).
- **Class behavior registry**: `trait ClassBehavior` with hooks (`on_created`, `on_property_changed`, `on_ancestry_changed`, `on_destroying`) and method implementations. Classes with no registered behavior are **data-only**: still fully creatable, scriptable and serializable. Calling an unimplemented method raises a clear `"<Class>:<Method> is not implemented in Raven"` error and increments a counter that feeds the roadmap.
- **Hierarchy rules** follow Roblox: no cycles; locked parents (`Destroy`, services); `Parent` set last when cloning; `Destroy` locks the parent, disconnects connections and recurses; `Clone` respects `Archivable`; services are found or created by `GetService`.
- **Change journal**: every mutation appends an event (`Created`, `PropertyChanged(id, prop)`, `ChildAdded`, `ChildRemoved`, `AncestryChanged`, `Destroying`). Two kinds of consumers read it:
  1. the signal dispatcher (`Changed`, `GetPropertyChangedSignal`, `ChildAdded`, `DescendantAdded`, `AncestryChanged`, ...), and
  2. engine systems (physics, GUI, render sync, audio). Each keeps its own cursor and processes dirty sets at its phase. Systems never get callbacks in the middle of a mutation.
- **No global state.** Several DataModels can exist in one process (server + client for Play mode, Studio edit + play). Each has its own DOM, Lua VM, scheduler and physics world.

## 6. Signals and scheduler (`raven-signal`, `raven-scheduler`)

### Signals

- `RBXScriptSignal`: `Connect`, `Once`, `Wait`, `ConnectParallel` (stub until Actors exist), plus legacy `connect`/`wait` aliases. `RBXScriptConnection`: `Disconnect`, `Connected`.
- **Immediate** mode (what `SignalBehavior.Default` maps to today, and what old places use): handlers run synchronously at fire time on a new thread, with a re-entrancy depth limit of 10. This is **Confirmed** from creator-docs.
- **Deferred** mode: handlers are queued and run at resumption points. `Disconnect()` drops pending invocations; `Destroy()` disconnects but still runs pending ones. This is **Confirmed** from creator-docs.

### Scheduler

- Single engine thread. Luau, the DOM and physics run on it. This matches Roblox's cooperative model, and it is where OpenRBLX went wrong.
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

## 7. Luau bridge (`raven-lua`)

- One Luau VM per DataModel (`mlua` with `luau`, `vendored`; `luau-jit` later). Each `Script`/`LocalScript`/`ModuleScript` runs on its own thread with its own environment table, which falls back to the shared globals. `getfenv`/`setfenv` work as in Luau.
- **Instance userdata** wraps an `InstanceId`. A weak-valued registry keeps **one userdata per instance**, so `==`, `rawequal` and table keys behave like Roblox. `__index` checks properties, then methods, then children by name. Destroyed instances still read.
- Datatypes are userdata with Roblox's operators, constructors and immutability.
- **Yielding**: a yielding engine function (`wait`, `task.wait`, `WaitForChild`, `Event:Wait`, `InvokeServer`, async HTTP) registers the current thread with the scheduler, then yields. That way a yield inside a `coroutine.wrap` behaves as on Roblox. The exact mlua mechanism (async functions vs. yield-with-values) gets prototyped in M1, and the observable behavior is pinned by conformance cases.
- **Security identities**: game scripts run at the normal game identity. Members tagged with a higher security in reflection raise the Roblox-style permission error.
- **Errors and output**: chunk names are the instance's full name, so messages read `Workspace.Script:3: ...`. Output goes through `LogService` with Roblox-style message types.
- `require(ModuleScript)`: caching per DataModel, cyclic-require error, and the "Requested module experienced an error while loading" message.

## 8. Engine systems

### Threading and data flow

```
            engine thread (per DataModel)                         render thread
  ┌───────────────────────────────────────────────────┐      ┌──────────────────────┐
  │ formats → DOM ⇄ Luau (scripts)                    │      │ render world         │
  │            │ change journal                       │      │  (proxies, meshes,   │
  │            ├─→ physics (assemblies, Rapier)       │ ───→ │   materials, lights, │
  │            ├─→ humanoid                           │ snap │   GUI draw lists)    │
  │            ├─→ GUI layout + text measurement      │ shot │ wgpu                 │
  │            ├─→ audio commands                     │      └──────────────────────┘
  │            └─→ render sync (builds snapshot delta)│
  └───────────────────────────────────────────────────┘
```

Scripts read some derived values synchronously: `AbsolutePosition`, `AbsoluteSize`, `TextBounds`, `TextFits`, raycasts, `GetPartsInPart`. So **GUI layout, text measurement and physics queries live on the engine thread**. The renderer only consumes snapshots, so the tree is never shared between threads behind a lock.

### Physics (`raven-physics`, `raven-humanoid`)

- **Assemblies**: parts connected by rigid joints (`Weld`, `WeldConstraint`, legacy surface joints, and `Motor6D` with its `Transform` applied as a kinematic offset) form one assembly. Each assembly maps to one Rapier rigid body with compound colliders. Anchored assemblies are fixed. Root-part selection follows Roblox's documented rules. Assemblies are rebuilt incrementally from the journal.
- Non-rigid constraints (Hinge, Prismatic, BallSocket, Rope, Spring, AlignPosition, ...) map to Rapier joints or force controllers. Legacy BodyMovers map to force controllers.
- Shapes: Block → cuboid; Ball → sphere; Cylinder → cylinder along the part's **X** axis; Wedge/CornerWedge → convex hulls; MeshPart → box, hull or decomposition according to `CollisionFidelity`.
- `CanCollide`/`CanTouch`/`CanQuery`, collision groups (Rapier interaction groups), `Touched`/`TouchEnded` from contact events, and `WorldRoot` queries (Raycast, Blockcast, Spherecast, GetPartsInPart, GetPartBoundsInBox).
- **Humanoid** is its own system: state machine, floor detection, HipHeight, WalkSpeed, JumpPower/JumpHeight, climbing, seats, health and death, and joint breaking. It drives the root assembly. Every behavior is tagged Approximation until checked against Studio recordings.
- Rapier is the foundation, not the specification. Where Roblox's solver differs (friction combine, elasticity, joint stiffness, sleeping), Raven's semantics layer adjusts, and each difference gets a provenance note.

### GUI (`raven-gui`)

Built from creator-docs and Studio observation only. Covers LayerCollectors (ScreenGui, SurfaceGui, BillboardGui), `UDim2` + `AnchorPoint` + `AutomaticSize`, the UI layouts, padding and constraints, `ZIndexBehavior`, `ClipsDescendants`, ScrollingFrame, `TextScaled`/`TextWrapped`/`RichText` (cosmic-text), `ImageRectOffset`/`ScaleType`/`SliceCenter`, input hit-testing and GUI events. The output is draw lists for the renderer.

### Rendering (`raven-render`)

- Render proxies per visual instance (parts, MeshParts, SpecialMeshes, Decals/Textures, lights, particles, beams), built from snapshot deltas.
- Primitive meshes generated in code. Mesh and texture caches are fed by `raven-content`.
- Materials: `Enum.Material` → Raven's default material set, or optionally the user's Roblox content.
- Lighting: Lighting service properties (ClockTime → sun, Ambient/OutdoorAmbient, Brightness, Atmosphere, Sky, post effects). Start with a forward renderer and shadow maps that approximate ShadowMap/Voxel. Future (clustered) lighting comes later.
- Every lighting and material formula is documented with its source and confidence.

### Content (`raven-content`)

- URI resolution: `rbxasset://`, `rbxassetid://`, legacy `http(s)://www.roblox.com/asset/?id=`, `rbxthumb://`.
- Providers tried in a configurable order: Raven defaults → user's Roblox install → downloaded content packages (hash-verified, cached) → asset delivery (anonymous; Open Cloud key or opt-in cookie for restricted assets) → local overrides.
- Fetching is async. Completion is delivered on the engine thread, so `ContentProvider` events and `IsLoaded`/`PreloadAsync` behave correctly.

### Services (first wave)

DataModel, Workspace, Players, Lighting, ReplicatedStorage, ReplicatedFirst, ServerScriptService, ServerStorage, StarterGui, StarterPack, StarterPlayer, Teams, RunService, UserInputService, ContextActionService, TweenService, Debris, CollectionService, HttpService (opt-in), ContentProvider, TextService, PhysicsService, LogService, SoundService, InsertService (limited). DataStoreService is backed by local files. That is an Approximation, and it's documented as one.

### Run modes and replication (`raven-net`)

- Modes: **Edit** (no scripts), **Run** (server simulation, no player), **Play** (server + client), **Server** (headless).
- Modern Play runs **separate server and client DataModels**. Raven does the same from M2 on, connected by an **in-process loopback replicator**. That makes `LocalScript` vs `Script`, RemoteEvents/RemoteFunctions and replication filtering correct early. A real network transport (BrikNet, quinn or renet, to be evaluated) plugs in later behind the same interface.
- Replication is Raven-to-Raven. Wire compatibility with real Roblox clients is not a goal.

## 9. Testing and measurement

1. **Unit tests** per crate.
2. **Format round-trip**: `rojo-rbx/rbx-test-files` plus local fixtures supplied via environment variables (never committed).
3. **Conformance suite** (`conformance/`): small Luau cases that print JSON (property values after operations, event orderings with timestamps, GUI absolute rects, CFrame math, physics positions after N steps). Each case is run once in Studio via a recorder plugin, and the output is committed as the expectation. `raven-cli conformance` runs the same cases headless with the fixed time source and compares with tolerances.
4. **Corpus scanner** (`raven-cli corpus-scan <dir>`): parses every place (rbx formats) and script (`full_moon`), and reports class, member and enum usage frequency, plus "loads / runs N seconds without error / errors by cause". This drives priorities and is Raven's headline progress metric.
5. **Render golden images** (later): Raven frames compared against Studio screenshots with perceptual tolerance.

## 10. Behavior provenance

Each subsystem has `docs/behavior/<subsystem>.md` with numbered entries:

```
GUI-012  AnchorPoint offsets position by -AnchorPoint * AbsoluteSize before clipping.
         Source: creator-docs GuiObject.AnchorPoint; conformance/gui/anchor_point.json
         Status: Confirmed
```

Status is one of **Confirmed**, **Inference**, **Approximation** or **Raven extension**. Code that implements an entry cites its ID in a comment. This is both the clean-implementation record and the compatibility map.

## 11. Extensions

- Extension classes and members are marked in the reflection overlay and only exist when the engine runs with the `extensions` profile. The default profile is strict compatibility.
- Engine-side extensions (better lighting, debugging tools, performance options) are configuration flags that are off in strict mode.
- Saving a file that contains extension classes warns, because Studio can't read them back.
- Candidate extension: **restore legacy embedded assets.** Modern Roblox loads `<Content><binary>` data from old XML files as empty (Confirmed in Studio, 2026-10-05). Raven matches that by default, but the legacy upgrade keeps the data, tagged by object and property, so an opt-in extension could show those textures again.

## 12. Milestones

Each milestone is a vertical slice with a measurable exit criterion. The order was revised after the first corpus scan (see research.md §9).

| # | Slice | Exit criterion |
|---|---|---|
| M0 ✅ | Workspace, `raven-types`, `raven-reflection`, `raven-formats` (incl. the legacy XML upgrade), `raven dump/roundtrip/corpus-scan` | All rbx-test-files fixtures round-trip; corpus report produced. Achieved: 99.3% of the archive loads. |
| M1 | `raven-dom`, signals, scheduler, Luau bridge, core Instance API, the methods/events list, headless runs of **server Scripts**, Studio conformance plugin | First conformance batch (Instance API, signals, `wait`/`task` ordering) passes; baseline % of archive scripts running N seconds without error |
| M2 | Renderer v0 (parts, primitive meshes, camera, basic lighting, sky) in `raven-player`; **client/server split** with the in-process replicator; a `Player` without a character; **LocalScripts** (PlayerScripts, ReplicatedFirst, PlayerGui as data); basic input (`UserInputService`, `Mouse`) | Archived places render recognisably; client-side scripts run alongside server scripts |
| M3 | Physics v0: assemblies, anchoring, collisions, welds/Motor6D/legacy surface joints, BodyMovers, Touched, queries | Physics conformance batch; unanchored places settle plausibly |
| M4 | Characters: spawning, Humanoid, tools, default camera/control scripts (Core-Scripts, Apache-2.0) and Raven's own Animate | Walk around and use tools in an archived place |
| M5 | GUI (layout, text, images, GUI input) | GUI conformance batch |
| M6 | Content: MeshParts, SpecialMesh, textures, decals, materials; audio | Corpus places render with meshes and sound |
| M7 | Network transport, headless server, multi-client | Two clients play together |
| M8+ | Terrain (incl. legacy `ClusterGrid` data), animation, particles, advanced lighting, Studio tooling, extensions | — |

Why this order: the client/server split moves forward to M2 so LocalScripts run as soon as there's a window. Characters stay in M4 because they need physics (M3) first. Physics comes before GUI because the corpus is dominated by physical content (Parts in 1,227 files, BodyMovers in about 200, welds and Snaps in hundreds).

## 13. Open decisions

1. **HopperBin** (229 files): modern Roblox no longer runs it. Decide before M4 whether to follow modern behavior or offer it as an opt-in extension.
2. **`null` numbers in legacy XML** (3 values in the archive) are read as 0; verify against Studio.

Settled: license is Apache-2.0 with a `NOTICE` file crediting AsyncFinn. The repository is initialised, with a `.gitignore` that excludes Roblox files, caches, reports and credentials. The methods/events list is committed as a compact generated file inside `raven-reflection`.
