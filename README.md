<div align="center">

# Rook

**An open-source reimplementation of the Roblox engine, written in Rust.**

An open engine for Roblox places: run them offline, host them yourself, mod them, and keep them alive. Built to match modern Roblox.

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
![Status: early development](https://img.shields.io/badge/status-early%20development-yellow.svg)

[Features](#features) · [Quick start](#quick-start) · [Usage](#usage) · [Architecture](#architecture) · [Roadmap](#roadmap) · [FAQ](#faq)

</div>

> [!NOTE]
> Rook is an independent project. It is not affiliated with, endorsed by, or sponsored by Roblox Corporation. "Roblox" is a trademark of Roblox Corporation. This repository never contains Roblox place files, models or client assets.

## Why Rook?

Millions of places have been built on Roblox since 2006, and today they run in exactly one place. Rook is an open engine in the spirit of Xash3D and OpenMW: bring your own places, and run them on an engine anyone can read, fix and extend.

**What you get**

- **Singleplayer that works offline.** The server runs inside the player, so a place plays on your machine with nothing to connect to.
- **Your own servers** *(planned, M8)*. Host a place yourself and invite your friends.
- **Preservation.** Old places keep running as long as someone has the file, with no account and nothing that can be switched off.
- **Modding.** The whole engine is open, so any behaviour can be inspected, patched and extended.
- **Fewer restrictions** *(planned)*. [Opt-in extensions](ROADMAP.md#rook-extensions-unscheduled) such as WebSockets for any script, and EditableImage and EditableMesh with no platform gating. They're off by default, so modern Roblox stays the reference.
- **A platform you already know.** The same Luau, Instance API and services. Scripts written for Roblox are scripts for Rook, and places saved from Studio open as they are.

**Why you can trust it**

- **Modern Roblox is the reference.** Pinned to API 0.741.19, with no separate legacy mode. Old content works as far as modern Roblox runs it.
- **Checked against Studio.** Behaviour is diffed against real Studio recordings, and each behaviour cites its source.
- **Measured on real places.** Progress is tracked on an archive of about 3,000 places, not hand-picked demos.

## Standing on the shoulders of giants

Rook doesn't reinvent what the community has already solved. It builds on projects proven on years of real Roblox files and puts its own effort into the engine between them.

- [**rbx-dom**](https://github.com/rojo-rbx/rbx-dom): file formats and the reflection database, the same foundation Rojo is built on
- [**rbx-test-files**](https://github.com/rojo-rbx/rbx-test-files): format test fixtures
- [**Luau**](https://luau.org) and [**mlua**](https://github.com/mlua-rs/mlua): Roblox's own open-source language, so scripts run on the real VM, not an imitation
- [**Rapier**](https://rapier.rs): physics
- [**wgpu**](https://wgpu.rs) and [**winit**](https://github.com/rust-windowing/winit): rendering on Vulkan, Metal and DirectX 12, and windowing
- [**full_moon**](https://github.com/Kampfkarren/full-moon): Luau parsing for the corpus scanner
- [**glam**](https://github.com/bitshifter/glam-rs): vector maths

What Rook adds is the engine itself: the DataModel, the scheduler, replication and Studio-checked behaviour.

## Status

Rook is in **early development**. File formats (M0), scripting (M1), the client/server split with the first renderer (M2) and physics (M3) are done. Content (M4: meshes, textures, decals) is in progress.

| Measure | Result |
|---|---|
| Archive places that load | **99.3%** of 3,077 unique files (11.5 million instances, 287,000 scripts) |
| Places with server scripts that run 10 s without a script error | **60.4%** (1,236 of 2,048) |
| Places that crash Rook | **0** |
| Studio conformance cases | Instance API, property coercion, error messages, signal order, `wait`/`task` order, tweens |

The archive is scanned with `rook corpus-scan` and run with `rook corpus-run`. Both reports list exactly what fails and why, and that list sets the priorities.

## Features

### File formats

- Loads and saves **`.rbxl`, `.rbxm`, `.rbxlx` and `.rbxmx`**, binary and XML.
- **Legacy XML upgrade** for roughly 2006–2010 files that standard parsers reject: dropped `<External>` entries, `<hash>` content, control characters, MSVC float values like `1.#QNAN`. Files that already load normally are never rewritten.
- Legacy property names (`size` → `Size`) and the database's migrations are applied on load, the way Studio upgrades old files.
- Unknown properties are kept, so saving never silently drops data, and `rook roundtrip` proves it.
- A corrupt file produces an error, never a crash.

### Data model

- All the Roblox datatypes: `Vector2`, `Vector3`, `CFrame`, `Color3`, `BrickColor`, `UDim`, `UDim2`, `Rect`, `NumberSequence`, `ColorSequence`, `NumberRange`, `PhysicalProperties`, `RaycastParams`, `OverlapParams`, `RaycastResult`, `Font`, `Faces`, `Axes`, enums, attributes and tags.
- A class, property and enum database compiled to dense numeric IDs, with inherited property layouts and per-class defaults.
- A runtime instance tree with generational IDs, Roblox's hierarchy rules (locked parents, cycles, `Destroy`, `Clone` and `Archivable`) and a change journal that signals, replication and rendering each read at their own pace.
- Several DataModels in one process, each with its own tree, Luau VM and scheduler, so a server and its clients run side by side.

### Scripting

- **Luau** through `mlua`, one VM per DataModel, each script on its own thread with its own environment (`getfenv`/`setfenv` work).
- The Instance API: properties, methods, events, children by name, `GetService`, `FindFirstChild`, `WaitForChild` and the rest. One userdata per instance, so `==` and table keys behave as on Roblox.
- Roblox datatypes as userdata with their constructors, operators and methods.
- `RBXScriptSignal` with `Connect`, `Once`, `Wait` and the legacy lowercase aliases, in both **Immediate** and **Deferred** signal modes.
- A scheduler for `wait`, `delay`, `spawn` and the `task` library, with deterministic simulated time for headless runs and tests.
- `require` for ModuleScripts, with per-DataModel caching and Roblox's cyclic-require and load-error messages.
- Roblox-style error messages and chunk names (`Workspace.Script:3: ...`), output through `LogService`.
- Security levels: members tagged above game identity raise Roblox's permission error.
- Services including `RunService`, `TweenService` (and the legacy GUI tweens), `Players`, `Debris`, `DataStoreService` (in memory for now) and `HttpService` (JSON and GUIDs; requests off).
- Unfinished members are either clear `"is not implemented in Rook yet"` errors or listed, inert stubs. Every stub is documented in [docs/behavior/stubs.md](docs/behavior/stubs.md), and a test fails if one is added without an entry.

### Client and server

- **Play mode as on modern Roblox**: a server DataModel and separate client DataModels, connected by an in-process replicator.
- Replication follows Roblox's rules: `NotReplicated` classes and properties stay on the server, a `PlayerGui` reaches only its player, server `Script` source is withheld, references are remapped, attributes and tags replicate, and client changes stay on the client (FilteringEnabled).
- Joining creates the `Player`, `Backpack` and `PlayerGui` from `StarterPack` and `StarterGui`, fires `PlayerAdded`, sets `Players.LocalPlayer` and starts **LocalScripts** where Roblox allows them.
- `RemoteEvent`, `UnreliableRemoteEvent` and `RemoteFunction`: arguments are copied between VMs the way Roblox copies them, and invokes yield until the reply.

### Rendering

- A **wgpu** renderer (Vulkan, Metal, DirectX 12) that draws snapshots of the scene and never touches the instance tree.
- Primitive parts (blocks, spheres, cylinders, wedges, corner wedges) with colour and transparency, a sky colour, sun, ambient light and fog.
- The built-in mesh shapes (SpecialMesh types, BlockMesh, CylinderMesh) at the sizes Roblox draws them, including Rook's own version of the classic head.
- `rook-player` runs the place live in a window with a fly camera that drives `Workspace.CurrentCamera`, or renders a single frame to PNG offscreen.

File meshes, textures, decals, terrain and shadows are on the [roadmap](#roadmap).

### Tooling

- `rook dump`: print a place's instance tree and properties.
- `rook roundtrip`: load, save and reload a file and report every difference.
- `rook corpus-scan`: scan a folder of places for what loads and which classes, properties and script APIs they use.
- `rook run-headless` and `rook corpus-run`: run server scripts without a window and report what works.
- `rook conformance record` / `run`: record behaviour in Roblox Studio and check Rook against it.

## Quick start

Rook needs a recent stable Rust toolchain (1.85 or later).

```sh
git clone https://github.com/AsyncFinn/rook
cd rook
cargo build --release
```

This builds two programs in `target/release`: `rook-player` (the windowed player) and `rook` (the command-line tool). Then open a place you have saved from Studio:

```sh
rook-player place.rbxl
```

## Usage

### Playing a place

```sh
rook-player place.rbxl                                    # play as a local player
rook-player place.rbxl --run                              # show the server alone (Studio's Run mode)
rook-player place.rbxl --screenshot out.png               # render one frame to a PNG
rook-player place.rbxl --camera 100,80,100 --look-at 0,0,0 --screenshot out.png
rook-player place.rbxl --seconds 5 --screenshot out.png   # run scripts for 5 s first
rook-player place.rbxl --script extra.luau                # also run a Luau file as a server script
```

By default the player starts a session with one local player and draws what that player's client sees, as Studio's Play mode does. Output is tagged by the side (server or client) that printed it.

| Control | Action |
|---|---|
| Right mouse button (hold) | Look around |
| W A S D | Move |
| Q / E | Down / up |
| Shift | Move faster |
| Mouse wheel | Change speed |

### Command bar

When the player runs in an interactive terminal, that terminal shows a `>` prompt, like Studio's command bar with the feel of Luau's own REPL. Each entry runs as Luau in the open place; an expression prints its value, and globals you set stay set. An unfinished statement continues on a `>>` line. The arrow keys edit the line and recall earlier entries, and output prints above the prompt without disturbing what you're typing.

```text
> workspace.Gravity
196.2
> for _, p in workspace:GetChildren() do
>>     if p:IsA("BasePart") then p.Anchored = false end
>> end
> :client
```

`:server` and `:client` choose where entries run (the server by default), and `:help` lists the commands. Ctrl+C clears the line; on an empty line it, or Ctrl+D, ends input. `rook run-headless place.rbxl --repl` gives the same prompt without a window, after `--seconds`, until input ends. `--no-repl` turns the prompt off.

The prompt is the `repl` Cargo feature, on by default. Build with `--no-default-features` to leave it out of a build you hand to others. It reads only the local terminal, never the network.

### Inspecting and testing files

```sh
rook dump place.rbxl --properties --depth 3
rook roundtrip place.rbxl                  # load, save, reload, report differences
rook roundtrip place.rbxl --to xml         # same, converting to XML
rook run-headless place.rbxl --seconds 10  # run server scripts, print output
```

### Working with an archive

```sh
rook corpus-scan path/to/places --out reports/corpus
rook corpus-run path/to/places
```

`corpus-scan` skips duplicates and writes `corpus.json` and `summary.md`: what loads, what fails and why, and which classes, properties and script APIs the files use. `corpus-run` runs every place headless and ranks the most common script errors.

### Conformance against Roblox Studio

```sh
rook conformance record   # run the cases in a Studio playtest and save the output
rook conformance run      # run the same cases in Rook and diff them
```

Recording uses Roblox Studio's built-in MCP server, so Studio is only observed through its public scripting API. See [conformance/README.md](conformance/README.md).

## Architecture

Rook is a Cargo workspace of small crates whose dependencies only point downward. Everything goes through one runtime DataModel: `file → instances → properties → change journal → replication / rendering → scripts → events`.

```
rook-types ← rook-reflection ← rook-dom ← rook-signal ← rook-scheduler ← rook-lua
                                   ↑                                        ↑
                    rook-formats ──┘                       rook-net ────────┤
                                                                            │
                                                          rook-engine ──────┘ ← apps
                                                               │
                                                               └─ scene snapshots → rook-render
```

| Crate | What it does |
|---|---|
| [`rook-types`](crates/rook-types) | Roblox datatypes and the lossless dynamic `Value` |
| [`rook-reflection`](crates/rook-reflection) | Class, property and enum database, plus the member list for the pinned Roblox version |
| [`rook-formats`](crates/rook-formats) | Loading and saving `.rbxl`, `.rbxm`, `.rbxlx`, `.rbxmx`, including legacy XML |
| [`rook-dom`](crates/rook-dom) | The runtime instance tree and its change journal |
| [`rook-signal`](crates/rook-signal), [`rook-scheduler`](crates/rook-scheduler) | Signals, connections, timers and the deferred queue |
| [`rook-lua`](crates/rook-lua) | The Luau bridge: Instance API, datatypes, globals, `require` |
| [`rook-net`](crates/rook-net) | Server-to-client replication |
| [`rook-engine`](crates/rook-engine) | DataModels, services, script start rules, play sessions and the frame loop |
| [`rook-render`](crates/rook-render) | The wgpu renderer |
| [`rook-cli`](apps/rook-cli) | The `rook` command-line tool |
| [`rook-player`](apps/rook-player) | The windowed player |

Read [docs/architecture.md](docs/architecture.md) for the full design and [docs/research.md](docs/research.md) for background and sources.

### Behaviour provenance

Every behaviour Rook implements is recorded with its source and one of four statuses: **Confirmed** (creator-docs or a Studio recording), **Inference**, **Approximation** or **Rook extension**. This is both the record of a clean implementation and a map of how compatible Rook is. Rook-specific extensions sit behind an explicit profile and are off by default.

## Roadmap

Physics (M3) is done; content (meshes, textures, decals) is next, then characters. The full plan, its order and why, is in [ROADMAP.md](ROADMAP.md).

## Development

Run the full check (format, lints and tests):

```sh
./scripts/check.sh
```

On Windows, use `scripts\check.ps1`.

Format tests use [rojo-rbx/rbx-test-files](https://github.com/rojo-rbx/rbx-test-files), which isn't committed:

```sh
git clone --depth 1 https://github.com/rojo-rbx/rbx-test-files fixtures/rbx-test-files
```

Build profiles:

| Command | Use |
|---|---|
| `cargo build` | Development. Rook's code is lightly optimised; dependencies are fully optimised so Luau and wgpu run at speed |
| `cargo build --release` | What players use |
| `cargo build --profile profiling` | Release speed with full debug info. Add `--features tracy` to `rook-player` for a live Tracy timeline |

## FAQ

**Can Rook connect to Roblox servers or join Roblox games?**
No. Replication is Rook to Rook. Wire compatibility with Roblox clients or servers isn't a goal. Instead, you'll be able to host your own Rook servers (M8 on the [roadmap](ROADMAP.md)).

**Where do I get places to open?**
Use places you made or have the rights to, for example saved from Roblox Studio with *File → Save to File*. Rook ships no Roblox content.

**Does Rook use Roblox's code or assets?**
No. Rook is written from public documentation, the public API dump and observation of Studio through its scripting API. Each behaviour cites its source.

**Why do old places look different from how they did in 2010?**
Rook matches what modern Roblox does with old content. For example, modern Roblox loads embedded legacy textures as empty, and so does Rook. Restoring them is a possible opt-in extension.

## Contributing

Rook gets better with every place people throw at it. The most useful thing you can do is run your places through Rook and report what breaks: places that fail to load, and scripts that behave differently from Roblox. Please include the `rook run-headless` output and, if you can, a minimal place that reproduces it. Star the repo to follow along.

Rook is maintained by [AsyncFinn](https://github.com/AsyncFinn). Pull requests are by invitation.

## License

Rook is licensed under the [Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution.
