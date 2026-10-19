<div align="center">

# Rook

**An open-source reimplementation of the Roblox engine, written in Rust.**

An open engine for Roblox places: run them offline, host them yourself, mod them, and keep them alive. Built to match modern Roblox.

[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust 1.97.1+](https://img.shields.io/badge/rust-1.97.1%2B-orange.svg)](https://www.rust-lang.org)
![Status: early development](https://img.shields.io/badge/status-early%20development-yellow.svg)

[Getting started](docs/guide/getting-started.md) · [Features](docs/guide/features.md) · [Tools](docs/guide/tools.md) · [Roadmap](ROADMAP.md) · [Contributing](CONTRIBUTING.md) · [FAQ](#faq)

</div>

> [!NOTE]
> Rook is an independent project. It is not affiliated with, endorsed by, or sponsored by Roblox Corporation. "Roblox" is a trademark of Roblox Corporation. This repository never contains Roblox place files, models or client assets.

## Why Rook?

Millions of places have been built on Roblox since 2006, and today they run in exactly one place. Rook is an open engine in the spirit of Xash3D and OpenMW: bring your own places, and run them on an engine anyone can read, fix and extend.

**What you get**

- **Singleplayer that works offline.** The server runs inside the player, so a place plays on your machine with nothing to connect to.
- **Your own servers** *(planned, M12)*. Host a place yourself and invite your friends.
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

Rook is in **early development**. File formats (M0), scripting (M1), the client/server split with the first renderer (M2) and physics (M3) are done. Content (M4: meshes, textures, decals) is in progress. The full plan, its order and why, is in [ROADMAP.md](ROADMAP.md).

| Measure | Result |
|---|---|
| Archive places that load | **99.3%** of 3,077 unique files (11.5 million instances, 287,000 scripts) |
| Places with server scripts that run 10 s without a script error | **60.4%** (1,236 of 2,048) |
| Places that crash Rook | **0** |
| Studio conformance cases | Instance API, property coercion, error messages, signal order, `wait`/`task` order, tweens |

The numbers come from running an archive of real places through Rook ([how](docs/guide/tools.md#working-with-an-archive)). The failures they list set the priorities.

## Features

- **Files:** loads and saves `.rbxl`, `.rbxm`, `.rbxlx` and `.rbxmx`, including 2006-era XML that other parsers reject, without dropping data.
- **Scripting:** Luau with the Instance API, Roblox's datatypes, signals in both Immediate and Deferred modes, `wait` and `task`, `require` and Roblox's error messages.
- **Client and server:** Play mode as on modern Roblox, with replication, FilteringEnabled, LocalScripts and remotes.
- **Physics:** Rapier at 240 Hz with Roblox's joints, assemblies, materials, BodyMovers, constraints and raycasts.
- **Rendering:** a wgpu renderer for parts, the built-in mesh shapes, mesh files and MeshParts, live in a window or to a PNG.
- **Content:** `rbxasset://`, `rbxassetid://` and legacy asset URLs resolve from your own Roblox install, content folders, a disk cache and Roblox's asset delivery.
- **Tools:** dump, round-trip, run headless, scan an archive, trace where content comes from and check behaviour against Studio.

The full list is in [docs/guide/features.md](docs/guide/features.md).

## Documentation

New here? Start with [Getting started](docs/guide/getting-started.md): build Rook and open your first place in three commands.


| | |
|---|---|
| [Getting started](docs/guide/getting-started.md) | Build Rook, open a place, controls and the command bar |
| [Features](docs/guide/features.md) | Everything Rook supports today |
| [Tools](docs/guide/tools.md) | The `rook-tools` command-line tool: inspect, run headless, scan archives, conformance |
| [Roadmap](ROADMAP.md) | What's next, in what order, and why |
| [Architecture](docs/design/architecture.md) | How Rook is built, and the rules every behaviour follows |
| [Behaviour notes](docs/behavior/) | What Rook found in Studio and the archive, topic by topic |
| [Contributing](CONTRIBUTING.md) | Reporting problems, finding your way around the code, running the checks |

## FAQ

**Can Rook connect to Roblox servers or join Roblox games?**
No. Replication is Rook to Rook. Wire compatibility with Roblox clients or servers isn't a goal. Instead, you'll be able to host your own Rook servers (M12 on the [roadmap](ROADMAP.md)).

**Does Rook use Roblox's code or assets?**
No. Rook is written from public documentation, the public API dump and observation of Studio through its scripting API. Each behaviour cites its source.

**Why do old places look different from how they did in 2010?**
Rook matches what modern Roblox does with old content. For example, modern Roblox loads embedded legacy textures as empty, and so does Rook. Restoring them is a possible opt-in extension.

## Contributing

Rook gets better with every place people throw at it. The most useful thing you can do is run your places through Rook and [report what breaks](CONTRIBUTING.md#reporting-a-problem). Star the repo to follow along.

## License

Rook is licensed under the [Apache License 2.0](LICENSE). See [NOTICE](NOTICE) for attribution.
