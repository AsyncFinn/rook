# Rook

Rook is an open-source, independent reimplementation of the Roblox engine and runtime, written in Rust. It aims to load Roblox places and models, run their scripts, and simulate and render them the way modern Roblox does, including the large body of older archived content that modern Roblox still runs.

Rook is not affiliated with, endorsed by, or sponsored by Roblox Corporation.

## Status

Early development. M0 (file formats and data) is done, M1 (scripting) is nearly done, and a first renderer is in place.

- 99.3% of a 3,000-place archive loads.
- 60% of archived places with server scripts run 10 seconds without a script error.
- Behaviour is checked against Roblox Studio with recorded conformance cases (`conformance/`).
- `rook-player` runs a place's server scripts and draws its parts live in a window, like Studio's Run mode.

| Crate | What it does |
|---|---|
| `rook-types` | Roblox datatypes (Vector3, CFrame, Color3, UDim2, ...) and the lossless dynamic `Value` |
| `rook-reflection` | Class, property and enum database, plus the member list for the pinned Roblox version |
| `rook-formats` | Loading and saving `.rbxl`, `.rbxm`, `.rbxlx`, `.rbxmx`, including legacy XML |
| `rook-dom` | The runtime instance tree and its change journal |
| `rook-signal`, `rook-scheduler` | Signals, connections, timers and the deferred queue |
| `rook-lua` | The Luau bridge: Instance API, datatypes, globals, `require` |
| `rook-engine` | A server DataModel: services, script start rules, the frame loop |
| `rook-render` | The wgpu renderer |
| `rook-cli` | The `rook` command-line tool |
| `rook-player` | The windowed player: runs a place and draws it |

See [docs/architecture.md](docs/architecture.md) for the design and milestones, and [docs/research.md](docs/research.md) for background.

## Building

Requires a recent stable Rust toolchain.

```sh
cargo build --release
```

## Viewing a place

```sh
rook-player place.rbxl                                  # open a window
rook-player place.rbxl --screenshot out.png             # render one frame
rook-player place.rbxl --camera 100,80,100 --look-at 0,0,0 --screenshot out.png
rook-player place.rbxl --seconds 5 --screenshot out.png  # run scripts first
```

The player runs the place's server scripts while you watch, as Studio's Run mode does, and the camera moves `Workspace.CurrentCamera`.

Hold the right mouse button to look around, WASD to move, Q/E for down/up, Shift to go faster, and the mouse wheel to change speed.

## Command-line tool

```sh
rook dump place.rbxl --properties --depth 3
rook roundtrip place.rbxl            # load, save, reload, report differences
rook roundtrip place.rbxl --to xml   # same, converting to XML
rook corpus-scan path/to/places --out reports/corpus
rook run-headless place.rbxl --seconds 10  # run server scripts, print output
rook corpus-run path/to/places             # run every place, report what works
rook conformance record                    # record cases in Roblox Studio
rook conformance run                       # check Rook against the recordings
```

`corpus-scan` walks a folder of places and models, skips duplicates, and writes `corpus.json` and `summary.md`: what loads, what fails and why, and which classes, properties and script APIs the files use.

## Tests

```sh
./scripts/check.sh      # or scripts\check.ps1 on Windows
```

Format tests use [rojo-rbx/rbx-test-files](https://github.com/rojo-rbx/rbx-test-files), which is not committed:

```sh
git clone --depth 1 https://github.com/rojo-rbx/rbx-test-files fixtures/rbx-test-files
```

This repository never contains Roblox place files, models or client assets.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

Rook is maintained by AsyncFinn. Bug reports are welcome; pull requests are by invitation.
