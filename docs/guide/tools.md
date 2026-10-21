# The `rook-tools` command-line tool

`rook-tools` is built alongside `rook-player` (see [getting-started.md](getting-started.md)).

| Command | What it does |
|---|---|
| `rook-tools dump` | Print a place's instance tree and properties |
| `rook-tools roundtrip` | Load, save and reload a file and report every difference |
| `rook-tools run-headless` | Run server scripts without a window and print their output |
| `rook-tools corpus-scan` | Scan a folder of places for what loads and which classes, properties and script APIs they use |
| `rook-tools corpus-run` | Run every place in a folder headless and rank the most common script errors |
| `rook-tools terrain` | Show what a file's Terrain holds (smooth voxels by material, or legacy blocks) and how long it takes to mesh; `--scan <folder>` counts both across a folder |
| `rook-tools content` | Show how content references resolve: from the Roblox install, content folders, the cache or asset delivery, and why any fail |
| `rook-tools cache` | Show the download cache's folder and size; `prune --older-than <days>`, `trim` or `clear` it |
| `rook-tools logs` | Print the newest log file's path; `--open` opens the logs folder |
| `rook-tools conformance record` / `run` | Record behavior in Roblox Studio and check Rook against it |

## Inspecting and testing files

```sh
rook-tools dump place.rbxl --properties --depth 3
rook-tools roundtrip place.rbxl                  # load, save, reload, report differences
rook-tools roundtrip place.rbxl --to xml         # same, converting to XML
rook-tools run-headless place.rbxl --seconds 10  # run server scripts, print output
rook-tools run-headless place.rbxl --repl        # then open a command bar (see getting-started.md)
```

## Content and assets

```sh
rook-tools content rbxassetid://12221462               # what it is, where it comes from, size, hash
rook-tools content place.rbxl                          # every reference in a place and its outcome: what resolves, from where, what fails and why
rook-tools content --place place.rbxl --offline        # only the install, folders and cache
                                                 # (--place also totals mesh and image references: which decode, by version or format, and why others fail)
rook-tools cache                                       # where downloads are cached and how much
rook-tools cache prune --older-than 30                 # delete what hasn't been used for 30 days
```

Rook finds your Roblox install by itself. Every setting is in `settings.json` in the data folder (`%LOCALAPPDATA%\Rook`), written the first time Rook runs with each key at its default and a comment saying what it does. An environment variable overrides the file (for `rook-tools`, `rook-player` and headless runs alike), and a command-line flag overrides both:

| Key | Environment variable | Flag | Default | What it does |
|---|---|---|---|---|
| `contentOrder` | `ROOK_CONTENT_ORDER` (comma-separated) | | `defaults, install, folders, delivery` | Where content comes from, in order |
| `contentFolders` | `ROOK_CONTENT_DIRS` (separated like `PATH`) | | none | Folders laid out like the install's `content` (content packages, local overrides) |
| `assetDelivery` | `ROOK_ASSET_DELIVERY` (`on`/`off`) | `rook-tools content --offline` | on in `rook-player` and `rook-tools content`, off in headless runs | Whether asset delivery may use the network |
| `robloxInstall` | `ROOK_ROBLOX_INSTALL` | | the newest install found | A Roblox version or `content` folder to use instead, or `"off"` |
| `cacheFolder` | `ROOK_CACHE_DIR` | | `%TEMP%\Rook\cache` on Windows, `~/Library/Caches/Rook` on macOS, `$XDG_CACHE_HOME/rook` elsewhere | Where downloads are cached |
| `cacheMaxSize` | `ROOK_CACHE_MAX` | | `2GB` | The cache's size limit (`500MB`, `5GB`, ...); Rook drops what was used least recently |
| `openCloudKeyFile` | `ROOK_OPEN_CLOUD_API_KEY` (the key itself) | | none | A file holding an Open Cloud key, tried for assets refused anonymously (needs the `legacy-asset:manage` scope). The key never goes in `settings.json` |
| `threads` | `ROOK_THREADS` | | the machine's logical cores | The cores Rook plans background threads for |
| `audioDevice` | `ROOK_AUDIO` | | `default` | `default`, `off` (`rook-player` never opens a device), or an output device's name |
| `masterVolume` | | `--volume` | `1` | Everything heard, 0 to 1, like Roblox's in-game volume |
| `uncapped` | | `--uncapped` | `false` | Draw as fast as possible instead of at the display's rate |
| `frameTimes` | | `--frame-times` | `false` | Print frame times to the terminal |
| `logLevel` | `ROOK_LOG` (a level or subsystems) | `--verbose` | `info` | What the terminal shows: `error`, `warn`, `info`, `debug`, `trace` |
| | `ROOK_DATA_DIR` | | `%LOCALAPPDATA%\Rook` on Windows | Where settings, logs, crash reports and kept content live (so it can't be in `settings.json`) |

## Logs and failures

After loading, `rook-player` prints one summary of what didn't load, grouped by cause. `--verbose` lists every failure with its instance, reference and reason; `--verbose content` (or `terrain`, `render`, ...) shows everything one area logs. Each player run writes a full log (`rook-tools logs` prints its path), and a crash leaves a report in the logs' `crashes` folder. See [data and logging](../design/data-and-logging.md).

Asset delivery only talks to Roblox's asset endpoints. Rook never reads browser cookies or stored credentials.

## Working with an archive

```sh
rook-tools corpus-scan path/to/places --out reports/corpus
rook-tools corpus-run path/to/places
rook-tools terrain --scan path/to/places
```

`corpus-scan` skips duplicates and writes `corpus.json` and `summary.md`: what loads, what fails and why, and which classes, properties and script APIs the files use. `corpus-run` runs every place headless and ranks the most common script errors. `terrain --scan` counts the unique files with smooth terrain and with legacy terrain, and lists the largest of each. These two reports produce the numbers in the README's status table, and the failures they list set Rook's priorities.

## Conformance against Roblox Studio

```sh
rook-tools conformance record   # run the cases in a Studio playtest and save the output
rook-tools conformance run      # run the same cases in Rook and diff them
```

Recording uses Roblox Studio's built-in MCP server, so Studio is only observed through its public scripting API. See [conformance/README.md](../../conformance/README.md).
