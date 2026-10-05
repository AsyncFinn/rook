# Raven

Raven is an open-source, independent reimplementation of the Roblox engine and runtime, written in Rust. It aims to load Roblox places and models, run their scripts, and simulate and render them the way modern Roblox does, including the large body of older archived content that modern Roblox still runs.

Raven is not affiliated with, endorsed by, or sponsored by Roblox Corporation.

## Status

Early development. Milestone M0 (the data foundation) is in progress:

| Crate | What it does |
|---|---|
| `raven-types` | Roblox datatypes (Vector3, CFrame, Color3, UDim2, ...) and the lossless dynamic `Value` |
| `raven-reflection` | Class, property and enum database, compiled from a pinned `rbx_reflection_database` |
| `raven-formats` | Loading and saving `.rbxl`, `.rbxm`, `.rbxlx`, `.rbxmx` |
| `raven-cli` | The `raven` command-line tool |

See [docs/architecture.md](docs/architecture.md) for the design and milestones, and [docs/research.md](docs/research.md) for background.

## Building

Requires a recent stable Rust toolchain.

```sh
cargo build --release
```

## Command-line tool

```sh
raven dump place.rbxl --properties --depth 3
raven roundtrip place.rbxl            # load, save, reload, report differences
raven roundtrip place.rbxl --to xml   # same, converting to XML
raven corpus-scan path/to/places --out reports/corpus
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

Raven is maintained by AsyncFinn. Bug reports are welcome; pull requests are by invitation.
