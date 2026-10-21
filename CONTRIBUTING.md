# Contributing to Rook

Rook is maintained by [AsyncFinn](https://github.com/AsyncFinn). Rook gets better with every place people throw at it, and bug reports are the most useful contribution there is. Pull requests are by invitation.

## Reporting a problem

The most valuable reports are:

- **Places that fail to load.**
- **Scripts that behave differently from Roblox**: a wrong value, a missing event, a different error message, things happening in a different order.
- **Crashes.** Rook should never crash on a place, so any crash is a bug.

Please include:

- The `rook-tools run-headless place.rbxl` output (see [docs/guide/tools.md](docs/guide/tools.md)).
- If you can, a minimal place that reproduces it. Only share places you have the rights to share.
- For a behaviour difference, what Roblox does and what Rook does.

Calling a member that Rook doesn't support yet gives an `"is not implemented in Rook yet"` error. Reports of which ones your places hit are welcome too: they help decide what comes next. The known ones are listed in [docs/behavior/stubs.md](docs/behavior/stubs.md).

## Finding your way around

Rook is a Cargo workspace of small crates whose dependencies only point downward. Everything goes through one runtime DataModel: `file → instances → properties → change journal → replication / rendering → scripts → events`. The full design is in [docs/design/architecture.md](docs/design/architecture.md), and background and sources are in [docs/design/research.md](docs/design/research.md).

| Crate | What it does |
|---|---|
| [`rook-types`](crates/rook-types) | Roblox datatypes and the lossless dynamic `Value` |
| [`rook-reflection`](crates/rook-reflection) | Class, property and enum database, plus the member list for the pinned Roblox version |
| [`rook-formats`](crates/rook-formats) | Loading and saving `.rbxl`, `.rbxm`, `.rbxlx`, `.rbxmx`, including legacy XML |
| [`rook-dom`](crates/rook-dom) | The runtime instance tree and its change journal |
| [`rook-platform`](crates/rook-platform) | The host machine as the portable crates reach it: files, threads, parallel work, the network (desktop implementation behind `native`) |
| [`rook-lua`](crates/rook-lua) | The Luau bridge: Instance API, datatypes, globals, `require`, and the signals, timers and deferred queue under it |
| [`rook-net`](crates/rook-net) | Server-to-client replication |
| [`rook-content`](crates/rook-content) | Content URLs, the provider chain (Rook defaults, the Roblox install, asset delivery), the cache, and mesh, image and sound decoding |
| [`rook-audio`](crates/rook-audio) | The mixer, SoundGroups, sound effects and the audio output device |
| [`rook-terrain`](crates/rook-terrain) | Terrain's voxels (`SmoothGrid`) and the surface Roblox meshes from them |
| [`rook-physics`](crates/rook-physics) | Assemblies, joints, constraints, contacts and scene queries over Rapier |
| [`rook-gui`](crates/rook-gui) | GUI layout (the `Absolute*` properties), text (fonts, shaping, `TextBounds`) and what ScreenGuis draw |
| [`rook-engine`](crates/rook-engine) | DataModels, services, script start rules, play sessions and the frame loop |
| [`rook-view`](crates/rook-view) | What a DataModel draws: the render scene built from the DOM, mesh files and images loading for it |
| [`rook-render`](crates/rook-render) | The wgpu renderer |
| [`rook-cli`](apps/rook-cli) | The `rook-tools` command-line tool |
| [`rook-player`](apps/rook-player) | The windowed player |

### Where things live

Most features span several crates: the rules in `rook-types` or the feature's own crate, the script side in `rook-lua`, the engine glue in `rook-engine`, and drawing in `rook-view` (DOM to draw list) and `rook-render`.

- **The Luau API:** `rook-lua/src/api/`. A member function (`Instance:FindFirstChild`, `Workspace:Raycast`) is in `api/methods/<topic>.rs` (registered in `methods.rs`); Luau sources such methods load are in `api/luau/`. A property's script-side behaviour is in `api/properties.rs`, member lookup in `api/instance/`, globals in `api/globals.rs`, datatypes (`CFrame`, `Ray`, ...) in `rook-lua/src/types/`.
- **Part shapes:** the shapes and their rules in `rook-types/src/part/` (`shape.rs`, `truss.rs`, `mesh.rs`, `surface.rs`); the draw meshes in `rook-render/src/mesh/` (`shapes.rs`, `head.rs`, `truss.rs`, `files.rs`); collision shapes in `rook-physics/src/shape.rs` and `shape/`; what a part draws in `rook-view/src/look.rs`.
- **Materials:** `rook-types/src/part/material.rs` (the enum, and how each looks in `material/looks.rs`); their textures in `rook-content/src/materials.rs`; MaterialVariants and SurfaceAppearances in `rook-view/src/look/materials.rs`; density and friction in `rook-physics/src/contacts/materials.rs`; terrain's in `rook-terrain/src/material.rs`.
- **Lighting:** the sun and sky in `rook-view/src/lighting.rs`, local lights in `rook-view/src/lighting/local.rs`; what the renderer takes in `rook-render/src/scene/lighting.rs`; drawing in `rook-render/src/renderer/lighting/` (local lights, shadow map), `renderer/tonemap.rs` and `shaders/lighting.wgsl`.
- **Effects** (Fire, Smoke, Sparkles, ParticleEmitter, Beam, Trail, Explosion): reading them in `rook-view/src/effects/`; the simulation in `rook-render/src/effects/`, drawing in `renderer/effects/`; their methods in `rook-lua/src/api/methods/effects.rs`.
- **Sounds:** decoding in `rook-content/src/decode/audio.rs`; the mixer, effects and 3D falloff in `rook-audio`; playback in `rook-engine/src/bridges/sounds/`; Sound's methods in `rook-lua/src/api/methods/sound.rs`, the Audio API's in `api/methods/audio_api.rs` with its shared rules in `rook-lua/src/audio_api.rs`.
- **Terrain:** voxels, meshing, materials and grass in `rook-terrain`; drawing in `rook-view/src/terrain.rs`; collision in `rook-physics/src/shape/terrain.rs`; the Terrain API in `rook-lua/src/api/methods/terrain.rs`.
- **Content** (meshes, images, sounds by URL): parsing references in `rook-content/src/reference.rs`, finding the bytes in `resolver/` (cache, the install, asset delivery, kept content), background loading in `loader.rs`, decoding in `decode/`; the drawing side in `rook-view/src/content/`.
- **GUI:** layout in `rook-gui/src/layout.rs` and `layout/` (UI layouts, constraints, `AutomaticSize`, padding), drawing order, clipping and borders in `rook-gui/src/draw.rs`, what the renderer takes in `rook-types/src/gui.rs`; script reads in `rook-lua/src/api/instance/access.rs`, GUI methods in `rook-lua/src/api/methods/gui.rs`; the GUI pass in `rook-render/src/renderer/gui.rs`.
- **Input:** the window's keys and camera in `rook-player/src/app/` (`keys.rs`, `camera.rs`); clicks and the mouse in `rook-engine/src/bridges/input.rs`; UserInputService in `rook-lua/src/api/methods/input.rs`.
- **Replication:** `rook-net` (`source.rs` on the server, `sink.rs` on the client, `replicator.rs`); the play session in `rook-engine/src/session/host.rs`; remotes in `rook-lua/src/api/methods/remotes.rs` and `rook-lua/src/net.rs`.
- **Physics:** `rook-physics/src/`: `assembly/bodies.rs` (assemblies as Rapier bodies), `sync.rs` (building the world from the DOM and applying the journal), `joints/` (joints, constraints, movers and the drives that power them), `contacts/` (materials, collision groups), `shape/` (collider shapes, unions, terrain), `query.rs` (raycasts and overlaps), `step.rs`.
- **The icon:** `resources/rook.svg`, the only copy; `apps/build.rs` embeds it in both apps and `rook-player/src/app/icon.rs` puts it on the window (see `resources/README.md`).
- **Settings, logs and the data folder:** `rook-engine/src/settings.rs`, `logging.rs` and `data_folder.rs`; the files themselves through `rook-platform`.
- **Findings from Studio and the archive:** `docs/behavior/<topic>.md`. The roadmap keeps only the plan and links there.

Each crate's `lib.rs` holds its module list, re-exports and core type; everything else is one topic per file. What a file covers decides when it's split, not its length: split it when it holds more than one topic (a second type with its own rules, a separate stage of a pipeline), or when someone looking for one thing has to wade through another. A long file that is all one topic, doc comments included, stays whole; a short one mixing two topics is split. Split by topic as `foo.rs` plus a `foo/` folder, never `mod.rs`. A `foo/` folder holds the code for each part of foo, and `foo.rs` says what those parts are, like `lib.rs` does for a crate: its module list and re-exports, plus only what the parts share (the module's core types and its entry point; `rook-render`'s `mesh.rs` keeps `Vertex`, `MeshData` and the `build` that picks a builder, while the ball, truss, head and mesh files are built in `mesh/`). Each part's implementation lives in its own file in the folder, never in `foo.rs`. A module that is one topic is a plain `foo.rs` with its tests inline at the end (`#[cfg(test)] mod tests { ... }`); once foo has parts, `foo/tests.rs` sits beside them. No folder holds only tests. Keep files at most three folders below a crate's `src/` (`src/a/b/c/file.rs`), and aim for two: deeper usually means a topic deserves its own top-level folder in the crate (`rook-tools drift` enforces three). `scripts/check` points out files over 1,000 lines of code (comments and blank lines not counted) as worth a look, not as a rule. Group files that share a topic in a folder too, before any of them gets big: a `src/` with many top-level files is hard to find your way around, and a crate is not one topic (`rook-engine` and `rook-player` are glue between many). Don't make a folder for a lone file, unless the topic is known to hold several kinds (the roadmap lists them, as with adornments): then the folder comes with the first.

### Errors

Failures keep their reason until something reports it. Return typed errors (`thiserror`) that say what went wrong, and log them with `tracing` where the context is known (an instance's path, a file's name); a content load that fails is logged at `debug` with its instance, reference and reason, and summed up after loading (see [data and logging](docs/design/data-and-logging.md)).

`.ok()`, `let _ =` and `unwrap_or_default()` on a `Result` are only for where something being absent is normal (an optional file that isn't there, a best-effort clean-up), never to hide a failure. When dropping an error is right, say why in a comment. The content-loading paths follow this; other places get fixed as they're touched.

### Comments

Comments say why: a Roblox behaviour, a measurement, a non-obvious reason, or an Observed/Measured/Inference tag, never what the next line already says. Findings have one home, `docs/behavior/<topic>.md`: a module's header says what it does and links there, and only one-line tagged rules stay beside the code that implements them.

### Keeping it clean

`scripts/check` enforces these, so drift fails the check instead of waiting for someone to notice:

- Two functions in different files with the same signature and body fail it: move the helper down a layer and keep one.
- A comment block over 15 lines outside tests is pointed out: findings belong in `docs/behavior`.
- Only `rook-engine` and the apps may read environment variables: settings are resolved in `rook-engine`'s `settings` module and passed down as plain values.
- Outside `rook-dom`, properties are read with `Dom`'s typed readers (`float_property`, `enum_item_property`, ...), not `property_by_name` (clippy's `disallowed-methods`); a raw read needs an `#[expect]` saying why.
- Crates in `crates/` reach the host only through `rook-platform`: clippy bans `std::fs`, `std::thread::spawn` and `std::thread::Builder` there (`apps/clippy.toml` lifts the ban for the apps), and cargo-deny (`deny.toml`) lets only the listed crates depend on `winit`, `cpal`, `ureq` and `rayon`. The known exceptions carry an `#[expect]` with the reason.
- Workspace lints include `unreachable_pub`, `needless_pass_by_value`, `match_same_arms`, `unnecessary_wraps`, `redundant_closure_for_method_calls` and `needless_pass_by_ref_mut`; where one is wrong, `#[expect]` it with the reason.

### Behaviour provenance

Every behaviour Rook implements is recorded with its source and one of four statuses: **Confirmed** (creator-docs or a Studio recording), **Inference**, **Approximation** or **Rook extension**. This is both the record of a clean implementation and a map of how compatible Rook is. Rook-specific extensions sit behind an explicit profile and are off by default. See [docs/design/architecture.md](docs/design/architecture.md#10-behavior-provenance).

## Development

Run the full check (format, lints, tests and dependency bans; it needs [cargo-deny](https://github.com/EmbarkStudios/cargo-deny), `cargo install cargo-deny`):

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

Behaviour changes are checked against Roblox Studio with the conformance suite; see [conformance/README.md](conformance/README.md).
