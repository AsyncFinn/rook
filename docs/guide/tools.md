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
| `rook-tools content` | Show how content references resolve: from Roblox's packages or install, content folders, the cache or asset delivery, and why any fail |
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
rook-tools run-headless place.rbxl --join --core-scripts --dump CoreGui
                                                 # a player's client too, running Roblox's CoreScripts; print its CoreGui
rook-tools run-headless place.rbxl --join --core-scripts --click 38,34 --dump CoreGui
                                                 # the same after clicking at (38, 34) in a 1280 x 720 window
rook-tools run-headless place.rbxl --join --core-scripts --user-settings settings.xml
                                                 # the player's settings load from settings.xml and are saved back
rook-tools run-headless place.rbxl --join --core-scripts --key One --dump CoreGui
                                                 # the same after pressing 1 (an Enum.KeyCode name)
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

Rook downloads the Roblox client files it needs (fonts, textures, the CoreScripts) from Roblox's CDN into its data folder (`%LOCALAPPDATA%\Rook\content\packages`) as places ask for them, one package at a time and announced in the terminal, and uses your Roblox install when downloads are off or fail; it finds the install by itself ([client packages](../behavior/content/client-packages.md)). Downloads are cached in `%LOCALAPPDATA%\Rook\cache`, kept under 2 GB.

Rook's settings come in three kinds ([settings](../design/data-and-logging.md#settings) has every key, variable and flag):

- **Build settings** are decided by whoever builds Rook, in `rook-build.toml` at the repository's root (or a file of your own over it, named by `ROOK_BUILD_CONFIG` when you build): where web requests go, whether clients run Roblox's CoreScripts, where content comes from, the cache's size. They are compiled in; editing the file means building again.
- **Your own choices as a player** are where Roblox keeps them: volume, camera sensitivity, fullscreen, the frame rate cap and the rest of the in-game menu's Settings page are on `UserSettings().GameSettings`, saved as `GlobalBasicSettings.xml` in the data folder (see [user settings](../behavior/user-settings.md)). The file has the same format as the Roblox client's own `GlobalBasicSettings_13.xml`, though Rook never reads that one: it starts from Roblox's defaults. `rook-player --volume` sets the volume for one run, until you change it in the menu.
- **Developer overrides** are flags and `ROOK_*` variables (`ROOK_CACHE_DIR`, `ROOK_ROBLOX_INSTALL`, `ROOK_ASSET_DELIVERY`, `--core-scripts`, `--uncapped`, ...), for `rook-tools`, `rook-player` and headless runs alike. They work in debug builds; a release build ignores them unless its build settings turn on `developerOverrides`. `--verbose`, `ROOK_LOG` and `ROOK_OPEN_CLOUD_API_KEY` work in every build.

Rook used to keep its settings in `settings.json`; the first run of a newer Rook carries what still has a home over to your player settings, logs what it dropped, and removes the file ([older data folders](../design/data-and-logging.md#older-data-folders)).

## Logs and failures

After loading, `rook-player` prints one summary of what didn't load, grouped by cause. `--verbose` lists every failure with its instance, reference and reason; `--verbose content` (or `terrain`, `render`, ...) shows everything one area logs. Each player run writes a full log (`rook-tools logs` prints its path), and a crash leaves a report in the logs' `crashes` folder. See [data and logging](../design/data-and-logging.md).

## Web endpoints

Every web request Rook makes, and every scripting API Roblox answers from its servers (user lookups, friends, groups, badges, text filtering, the CoreScripts' requests), goes through one build setting, `webBase` in `rook-build.toml` (and `ROOK_WEB_BASE` over it, in builds with developer overrides), so whoever builds Rook chooses where they go:

- **Left out** (the default): asset delivery uses Roblox, and everything else answers on your machine. Text passes through unfiltered, the session's players are the only users, and nobody is in a group or owns a badge.
- **Offline**: `webBase = "off"` sends nothing anywhere; assets and Roblox's files come only from your install, content folders, cache and the packages already downloaded.
- **A mirror or your own server**: `webBase = "example.com"` sends `users.roblox.com`'s requests to `users.example.com`, and so on for every domain. `webBase = "http://127.0.0.1:8080"` sends them all to one server, under `/users`, `/assetdelivery`, ...
- **Per domain**: under `[webDomains]`, `users = "https://users.example.com"` or `badges = "off"` overrides `webBase` for that domain only.
- **Roblox's client packages**: they come from the `clientsettings` (the live version) and `setup` (`setup.rbxcdn.com`) domains; `setup = "off"` under `[webDomains]` stops downloading them, and the packages already downloaded and your install answer instead.

A server answers in Roblox's request and response shapes; [web endpoints](../design/web-endpoints.md) lists every request and what each API does without one. An Open Cloud key is only ever sent to Roblox, and only while asset delivery goes to Roblox. Rook never reads browser cookies or stored credentials.

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
