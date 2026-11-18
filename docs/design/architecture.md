# Architecture

The current design, first drafted 2026-10-04 and kept up to date as the milestones land. Crates marked *(planned)* or *(unscheduled)* don't exist yet. Background and sources are in [research.md](research.md); the rules every part follows are in [policies](policies.md); the plan is in the [roadmap](../roadmap/README.md); what each milestone found in Studio is in [`docs/behavior/`](../behavior/).

## Goals that shape the architecture

1. **Modern Roblox is the single source of truth**, tracking the live version. Old content works to the extent modern Roblox runs it. There is no separate legacy mode.
2. **Instance-first.** Everything (files, scripts, physics, rendering, GUI) goes through one runtime DataModel. The pipeline for a Part is `file → instances → properties → hierarchy → change journal → physics / render / GUI → scripts → events`, never `file → cube`.
3. **Every class exists.** Every class in the reflection database can be created, parented, read, written, saved and scripted, even before Rook gives it any behavior.
4. **Verifiable.** Behavior is checked against values recorded from live Studio, and every behavior has a documented source.
5. **Extensions never leak into compatibility.** Rook-specific features sit behind an explicit profile.

## Crate layout

```
crates/
  rook-platform     The host machine as the portable crates reach it: files, long-lived threads, parallel work, the network, the clipboard, credentials
  rook-types        Roblox datatypes (Vector3, CFrame, Color3, UDim2, BrickColor, sequences, enums, Value)
  rook-reflection   Class/property/enum/event database: Rook's copy of rbx-dom's, kept current + Rook overlay
  rook-dom          Runtime instance tree, property storage, change journal, class behavior registry
  rook-formats      rbxl/rbxm/rbxlx/rbxmx <-> rook-dom (via rbx_binary / rbx_xml / rbx_dom_weak)
  rook-lua          mlua/Luau bridge: Instance & datatype userdata, globals, script environments, require;
                    under it, Lua-agnostic: signals (connections, Immediate + Deferred dispatch, re-entrancy
                    limit) and the scheduler (thread queues, wait/task, resumption points, time source)
  rook-content      Content URIs, providers (Rook defaults, local Roblox install, packages, asset delivery), cache, decoders
  rook-physics      Assemblies, joints, constraints, collisions, queries; Roblox semantics over Rapier
  rook-humanoid     (planned) Humanoid state machine and character controller
  rook-gui          GUI layout, text measurement, hit testing (engine-side; produces draw lists)
  rook-audio        Mixer, SoundGroups, sound effects; the output device (cpal) behind its `output` feature
  rook-terrain      Terrain voxels (SmoothGrid decoding) and Roblox's surface-nets mesher
  rook-net          Replication (in-process loopback first, network transport later)
  rook-engine       Assembles a DataModel: services, systems, run modes, headless host; input events ->
                    UserInputService / ContextActionService (bridges/input)
  rook-scene         What a DataModel draws: the render scene built from the DOM, kept up to date, and the
                    mesh files and images it loads (DOM → rook-render's input)
  rook-render       wgpu renderer: render world, parts/meshes/materials/lighting/sky/particles/GUI pass,
                    and drawing into a window (WindowSurface)
  rook-client       A client without an OS of its own: the game a player sees (Play and Run), the frame
                    loop and render thread, screenshot/bench/record; its `window` feature is the winit
                    window, with its gamepads, every app that opens one runs
apps/
  rook-cli          rook: opens places in rook-player, dump, roundtrip, run-headless, content
  xtask             cargo xtask: conformance, corpus-scan, drift, package; never shipped
  rook-player       the desktop player: its command line around rook-client, message boxes
  rook-server       dedicated server: a place's server at 60 Hz, no window
  rook-studio       (unscheduled) editor
conformance/         Studio-recorded expectations + the Luau cases that produced them
docs/                guide (users), roadmap (the plan), design (architecture, policies, research),
                     behavior (findings by area and topic)
```

Dependencies only point downward:

```
rook-types ← rook-reflection ← rook-dom ← rook-formats
                                  ↑
      rook-content, rook-terrain, rook-physics, rook-audio, rook-net, rook-gui
      (planned: rook-humanoid, rook-input)
                                  ↑
                              rook-lua
                                  ↑
                             rook-engine ←───────────── rook-cli, rook-server
                                  ↑
             rook-render ←── rook-scene ←── rook-client ←── rook-player (apps that draw)

rook-platform sits below all of them: every crate that touches the host reaches it through rook-platform.
```

`rook-render` never touches the DOM (see [threads and frames](architecture/threads-and-frames.md#threading-and-data-flow)): `rook-scene` reads the DOM and builds what the renderer takes. Only `rook-client`, and the apps that draw through it, link `rook-scene` and `rook-render`; `rook-engine` and `rook-server` depend on neither. An app that opens a window is its command line or entry point, the platform it installs and a call to `rook_client::window::run`: the frame loop, input and drawing are shared, and the GPU stays inside `rook-render`.

### The seam: portable crates and the host

Everything in `crates/` is portable: it reaches the host machine only through `rook-platform` (`rook_platform::get()`), whose services are traits: `files` (settings, logs, crash reports, the console's history), `workers` (short work spread over cores: culling, particles, terrain meshing, terrain colliders), `threads` (long-lived threads such as the console's reader), `network` (asset delivery's and scripts' web requests, to the URLs `endpoints` picks per Roblox web domain: see [web endpoints](web-endpoints.md)) `clipboard` (a TextBox's copy and paste) and `credentials` (the OS credential store, for the Open Cloud key). **Portable crates get the platform from the app**, and **a server links only `rook-engine`** (and what it pulls in): no window, renderer or audio device.

The desktop implementation lives in `rook-platform` itself, behind a `native` feature (std files and threads, Rayon's global pool, ureq, arboard's clipboard, keyring-core's stores) that only apps turn on, as `rook-audio`'s `output` feature keeps cpal out until an app asks. An app either turns `native` on, which makes it the platform when nothing else was installed, or calls `rook_platform::install` with its own (a host without threads uses `parallel::Serial`). This beats an implementation in each app: `rook-player`, `rook-cli` and `rook-server` share one copy instead of three, crates' own tests get the real thing as a dev-dependency, and the code sits beside the contract it implements; the feature keeps its dependencies out of any build that doesn't ask for them.

Guards keep it that way:

- clippy's `disallowed-methods`/`disallowed-types` (root `clippy.toml`) ban `std::thread::spawn`, `std::thread::Builder` and the `std::fs` functions and `File` in `crates/`. `apps/clippy.toml` lifts the ban for the apps, and `rook-platform/clippy.toml` for the crate that implements the route. Clippy reads only the nearest `clippy.toml`, so `rook-dom/clippy.toml` carries a copy of the bans.
- cargo-deny (`deny.toml`, `cargo deny check bans`) lets only listed crates depend on `winit` (`rook-client`), `wgpu` (`rook-render`), `cpal` (`rook-audio`), `ureq` (`rook-platform`) and `rayon` (`rook-platform`, `rook-cli`, and Rapier and Parry).

Known exceptions, each marked with an `#[expect]` saying why:

- `rook-content` is the loader: its cache, the install lookup, Roblox's client packages (downloaded and extracted into the data folder), local file reads, its decode workers and the cache-trimming thread still use std directly.
- `rook-physics`: Rapier's `parallel` feature runs a step on Rayon's global pool, which `rook-physics` sizes (`size_worker_pool`); `rook-platform`'s native workers share that pool.
- `rook-engine`: the data folder's location (`directories`) and the console's line editor (reedline, behind the `repl` feature), which keeps its own history file.
- Tests and examples set up files on the machine running them.
## Subsystems

Each subsystem's design has its own page in [`architecture/`](architecture/):

- [The data model](architecture/data-model.md): datatypes (`rook-types`), the class database (`rook-reflection`) and the runtime DOM (`rook-dom`)
- [Scripting](architecture/scripting.md): signals, the scheduler and the Luau bridge (`rook-lua`)
- [Threads and frames](architecture/threads-and-frames.md): threading and data flow, copies of state, frame order, per-frame budgets
- [Physics](architecture/physics.md): assemblies, constraints, the timestep, queries, terrain, Humanoids (`rook-physics`)
- [GUI](architecture/gui.md): layout and retained-mode redrawing (`rook-gui`)
- [Rendering](architecture/rendering.md): meshes, images, draws, levels of detail, particles, lighting and shadows (`rook-render`)
- [Content](architecture/content.md): references, providers, the cache and decoders (`rook-content`)
- [Services and replication](architecture/replication.md): the first services, run modes and `rook-net`

## Testing and measurement

1. **Unit tests** per crate.
2. **Format round-trip**: `rojo-rbx/rbx-test-files` plus local fixtures supplied via environment variables (never committed).
3. **Conformance suite** (`conformance/`): small Luau cases that log what they observe (event orderings, error messages, property coercion, scheduler order, and later GUI rects, CFrame math and physics positions). `cargo xtask conformance record` runs each case in a Studio playtest through Studio's built-in MCP server and commits the output as the expectation; `cargo xtask conformance run` runs the same cases in Rook and diffs them. See `conformance/README.md`.
4. **Corpus scanner** (`cargo xtask corpus-scan <dir>`): parses every place (rbx formats) and script (`full_moon`), and reports class, member and enum usage frequency, plus "loads / runs N seconds without error / errors by cause". This drives priorities and is Rook's headline progress metric.
5. **Render golden images** (later): Rook frames compared against Studio screenshots with perceptual tolerance.

## Behavior provenance

Each subsystem has its pages in `docs/behavior/<area>/`, each opening with when it was last checked against Studio. Entries are planned to be numbered, like this one for GUI (planned; its conformance case doesn't exist yet):

```
GUI-012  AnchorPoint offsets position by -AnchorPoint * AbsoluteSize before clipping.
         Source: creator-docs GuiObject.AnchorPoint; conformance/gui/anchor_point.json
         Status: Confirmed
```

Status is one of **Confirmed**, **Inference**, **Approximation** or **Rook extension**. Code that implements an entry cites its ID in a comment. This is both the clean-implementation record and the compatibility map.

## Extensions

- Extension classes and members are marked in the reflection overlay and only exist when the engine runs with the `extensions` profile. The default profile is strict compatibility.
- Engine-side extensions (better lighting, debugging tools, performance options) are configuration flags that are off in strict mode.
- Saving a file that contains extension classes warns, because Studio can't read them back.
- Candidate extension: **restore legacy embedded assets.** Modern Roblox loads `<Content><binary>` data from old XML files as empty (Confirmed in Studio, 2026-10-05). Rook matches that by default, but the legacy upgrade keeps the data, tagged by object and property, so an opt-in extension could show those textures again.


## Open decisions

1. **HopperBin**: see [Unscheduled](../roadmap/unscheduled.md#undecided).
2. **`null` numbers in legacy XML** (3 values in the archive) are read as 0; verify against Studio.

Settled: license is Apache-2.0 with a `NOTICE` file crediting the Rook Contributors. The repository is initialized, with a `.gitignore` that excludes Roblox files, caches, reports and credentials. The methods/events list is committed as a compact generated file inside `rook-reflection`.
