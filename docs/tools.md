# The `rook` command-line tool

`rook` is built alongside `rook-player` (see [getting-started.md](getting-started.md)).

| Command | What it does |
|---|---|
| `rook dump` | Print a place's instance tree and properties |
| `rook roundtrip` | Load, save and reload a file and report every difference |
| `rook run-headless` | Run server scripts without a window and print their output |
| `rook corpus-scan` | Scan a folder of places for what loads and which classes, properties and script APIs they use |
| `rook corpus-run` | Run every place in a folder headless and rank the most common script errors |
| `rook content` | Show how content references resolve: from the Roblox install, content folders, the cache or asset delivery, and why any fail |
| `rook conformance record` / `run` | Record behaviour in Roblox Studio and check Rook against it |

## Inspecting and testing files

```sh
rook dump place.rbxl --properties --depth 3
rook roundtrip place.rbxl                  # load, save, reload, report differences
rook roundtrip place.rbxl --to xml         # same, converting to XML
rook run-headless place.rbxl --seconds 10  # run server scripts, print output
rook run-headless place.rbxl --repl        # then open a command bar (see getting-started.md)
```

## Content and assets

```sh
rook content rbxassetid://12221462               # what it is, where it comes from, size, hash
rook content --place place.rbxl                  # every reference in a place: what resolves, from where, what fails and why
rook content --place place.rbxl --offline        # only the install, folders and cache
```

Rook finds your Roblox install by itself. These environment variables change what it uses (for `rook`, `rook-player` and headless runs alike):

| Variable | Effect |
|---|---|
| `ROOK_ROBLOX_INSTALL` | A Roblox version folder or `content` folder to use instead, or `off` |
| `ROOK_CONTENT_DIRS` | Extra folders laid out like `content` (content packages, local overrides) |
| `ROOK_CONTENT_ORDER` | Provider order, default `defaults,install,folders,delivery` |
| `ROOK_ASSET_DELIVERY` | `on` or `off`. On by default in `rook-player` and `rook content`, off in headless runs |
| `ROOK_OPEN_CLOUD_API_KEY` | An Open Cloud key, tried for assets refused anonymously (needs the `legacy-asset:manage` scope) |
| `ROOK_CACHE_DIR` | Where downloads are cached (default `%LOCALAPPDATA%\Rook\cache` or the platform cache folder) |

Asset delivery only talks to Roblox's asset endpoints. Rook never reads browser cookies or stored credentials.

## Working with an archive

```sh
rook corpus-scan path/to/places --out reports/corpus
rook corpus-run path/to/places
```

`corpus-scan` skips duplicates and writes `corpus.json` and `summary.md`: what loads, what fails and why, and which classes, properties and script APIs the files use. `corpus-run` runs every place headless and ranks the most common script errors. These two reports produce the numbers in the README's status table, and the failures they list set Rook's priorities.

## Conformance against Roblox Studio

```sh
rook conformance record   # run the cases in a Studio playtest and save the output
rook conformance run      # run the same cases in Rook and diff them
```

Recording uses Roblox Studio's built-in MCP server, so Studio is only observed through its public scripting API. See [conformance/README.md](../conformance/README.md).
