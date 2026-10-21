# Data folder, settings and logging

How Rook reports what went wrong, and where it keeps what outlives a run. The code is in `rook-engine`'s `data_folder`, `settings` and `logging` modules; content's part is in `rook-content`'s `resolver::kept`.

## The data folder

Everything Rook keeps for itself lives in one folder: `%LOCALAPPDATA%\Rook` on Windows. `ROOK_DATA_DIR` moves all of it.

```
%LOCALAPPDATA%\Rook\
├─ settings.json
├─ logs\
│  ├─ <program>-<YYYY-MM-DD_HH-MM-SS>.log
│  └─ crashes\<program>-<YYYY-MM-DD_HH-MM-SS>.txt
├─ content\
│  ├─ blobs\<first 2 hex>\<blake3>
│  └─ refs\asset-<id>-v<version>
└─ state\
   └─ repl-history.txt

%TEMP%\Rook\
└─ cache\
```

On macOS the data is in `~/Library/Application Support/Rook` and the logs in `~/Library/Logs/Rook`. On Linux the data is in `$XDG_DATA_HOME/rook`, and logs and state in `$XDG_STATE_HOME/rook`. The `directories` crate finds those folders.

The download cache is not in the data folder. It stays in `%TEMP%\Rook\cache` (`rook_content::resolver::cache::Cache`) so Windows can clear it; anything cleared is fetched again. Two things are deliberately elsewhere too. The Open Cloud API key belongs in the OS credential store ([research §7](research.md#7-content-and-asset-policy)). The Roblox install's files are read where they are, never copied in.

`state\recent.json` (recent places, the window's size and place) has its path reserved (`DataDir::recent`). Nothing writes it yet.

## settings.json

Rook writes `settings.json` the first time it runs, with every key at its default and a `//` comment saying what each does.

JSON rather than TOML because Roblox developers know JSON. It is read leniently: `//` and `/* */` comments and trailing commas are allowed, and unknown keys are ignored. A file that doesn't parse is reported once and Rook's defaults are used; the file is left as it is.

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

Each value comes from the first of these that sets it: the command-line flag, the `ROOK_*` environment variable, `settings.json`, then Rook's default (`settings::pick`). `rook-engine`'s `settings` module resolves every one and hands plain values down (`rook_content::Config`, the audio device and volume, the thread budget); no other crate reads the environment. `ROOK_STUDIO`, which picks the Studio window `rook-tools conformance` drives, is a developer's variable of `rook-cli` alone.

`masterVolume` is Rook's version of the in-game menu's volume, `UserGameSettings.MasterVolume`. Once `UserGameSettings` exists (branch `feat/user-settings`), its `MasterVolume` should read and write this setting.

## Logging

Rook's messages go through the `tracing` crate. A layer prints them in the console's line format, the one Studio's Output uses: `05:38:06.439  Loaded place.rbxl in 0.17 s  -  Rook`. They print above the REPL prompt like script output. Events are grouped by subsystem: a crate's events (`rook_content::…`), or ones logged with that target (`target: "content"`). The subsystems are `content`, `terrain`, `render`, `physics`, `audio`, `lua`, `engine`, `net`, `formats` and `scene`.

What the terminal shows:

- **By default:** `info` and up. After loading, one summary of what didn't load, grouped by cause, with how many instances and files each cause hit and the first instance:

  ```
  4 unions and 1 mesh didn't load (--verbose for details)
      3 unions (2 files): union geometry CSGMDL3 is not supported yet (unknown-version) (first: Workspace.Ruins.Union)
      1 union (1 file): Authentication required to access Asset (forbidden) (first: Workspace.Gate)
      1 mesh (1 file): Authentication required to access Asset (forbidden) (first: Workspace.Tree.Mesh)
  ```

  Failed asset loads that Studio shows in its Output are always printed, once per file, as errors (red), as Studio does. These are assets that are missing or not allowed, and every failed sound (`Failed to load sound rbxassetid://…: …`). Rook's own gaps, such as a version it can't read yet, appear only in the summary and the detail. The image and mesh warning's wording, `Image "<ref>" failed to load in "<path>": <reason>`, is an Inference from memory, not checked against Studio.
- **`--verbose`:** `debug` and up: every failure, with instance path, content reference and reason (`Workspace.Ruins.Union: rbxassetid://123 — union geometry CSGMDL3 is not supported yet`).
- **`--verbose <subsystem>`** (or several, comma-separated): everything those subsystems log, at any level, plus the usual `info`. `--verbose content` before a file name is taken as the subsystem; a bare `--verbose` before a file name is not (`logging::fix_verbose_args`).

Libraries' own events reach the terminal only as warnings and errors.

### Log files

Each run of the player writes one file, `logs\rook-player-<time>.log`. `rook-tools` writes one only for long commands (`corpus-scan`, `corpus-run`, `conformance`). The file gets Rook's `debug` and up whatever the terminal shows, plus every line the terminal printed, script output included. It is plain text without colours, in the terminal's line format. At startup, old logs are cleaned: a file is deleted once it is beyond the newest 30 runs and older than 14 days, and the oldest go first past 100 MB in total. Crash reports are cleaned by the same rules.

`rook-tools logs` prints the newest log's path. `rook-tools logs --open` opens the folder.

### Crash reports

`logging::init` installs a panic hook that notes each panic: message, place, thread and a backtrace. Rook catches some panics on purpose, such as a decoder that panics on a damaged file, so the hook writes nothing itself. The program's `main` runs inside `logging::run`. When a panic gets there, `run` writes `logs\crashes\<program>-<time>.txt` and exits with code 101. The report holds the panic, the backtrace, the OS, the GPU and driver (once a renderer exists), the log file's path and the last 200 lines logged. `rook-player --crash-test` (hidden) panics after loading, to check this.

## Failures carry their reason

Content failures are `rook_content::Failure`: a `FailureKind` (`not-found`, `forbidden`, `unsupported`, `offline`, `network`, `invalid`, `unknown-version`) and a message. The decoders return typed errors (`MeshError`, `ImageError`, `AudioError`). The loader turns them into failures, keeping "a version Rook can't read yet" (`unknown-version`) apart from "damaged or not that kind of file" (`invalid`). `rook-view`'s scene keeps each failed file's failure and logs it where the instance path is known (`rook_view::content::report`). See the error rule in [CONTRIBUTING](../../CONTRIBUTING.md#errors).

`rook-tools content <place>` lists every content reference in a place and what became of it: where it came from, or why it didn't load or decode.

## Kept content

Some content Rook loads itself in nearly every place: the built-in materials' texture packs, terrain's packs, and the images those name. These are kept for good in `content\` rather than the cache. The list is fixed (`crates/rook-content/data/kept.txt`), never what places happen to load. Each asset is pinned to a version, because an asset id can be uploaded again and a version never changes. Asset delivery gives the version in its `roblox-assetversionnumber` header.

A request for a kept asset by its bare id is answered at the pinned version: from `content\` first, then the cache (copying it into `content\`), then asset delivery, which writes it into `content\`. `content\` is never trimmed. It has the cache's layout, blobs by hash and one ref file per key (`asset-<id>-v<version>`), so one reader handles both. `rook-tools content --pin` writes the list again from the materials' and terrain's tables with each asset's version today. A test checks that every pack in those tables is listed.

The default sky is not in `content\`. It comes from the Roblox install, read in place, and without one Rook draws its own plain sky.
