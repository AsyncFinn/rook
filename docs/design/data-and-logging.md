# Data folder, settings and logging

How Rook reports what went wrong, where it keeps what outlives a run, and where its settings come from. The code is in `rook-engine`'s `data_folder`, `build_config`, `settings`, `user_settings` and `logging` modules; content's part is in `rook-content`'s `resolver::kept`, `resolver::packages` and `resolver::cache`.

## The data folder

Everything Rook keeps for itself lives in one folder: `%LOCALAPPDATA%\Rook` on Windows. Rook ships no content files; what it downloads goes here.

```
%LOCALAPPDATA%\Rook\
├─ GlobalBasicSettings.xml        the player's settings
├─ logs\
│  ├─ <program>-<YYYY-MM-DD_HH-MM-SS>.log
│  └─ crashes\<program>-<YYYY-MM-DD_HH-MM-SS>.txt
├─ cache\                         downloads, kept under cacheMaxSize
│  ├─ blobs\<first 2 hex>\<blake3>
│  └─ refs\
├─ content\
│  ├─ packages\                   Roblox's client packages, laid out like an install
│  │  ├─ version.txt, manifest.txt
│  │  ├─ fonts\, textures\, sky\, ...
│  │  ├─ ExtraContent\models\, ...
│  │  └─ PlatformContent\pc\fonts\, ...
│  └─ assets\                     what Rook itself loads in nearly every place
│     ├─ blobs\<first 2 hex>\<blake3>
│     └─ refs\asset-<id>-v<version>
└─ state\
   └─ repl-history.txt
```

On macOS the data is in `~/Library/Application Support/Rook` and the logs in `~/Library/Logs/Rook`. On Linux the data is in `$XDG_DATA_HOME/rook`, and logs and state in `$XDG_STATE_HOME/rook`. The `directories` crate finds those folders. `ROOK_DATA_DIR` moves all of it under one folder, in builds with [developer overrides](#developer-overrides).

The cache (`rook_content::resolver::cache::Cache`) is the part Rook trims itself: under the build's `cacheMaxSize`, dropping what was used least recently. `rook-tools cache` shows it and prunes, trims or clears it, and anything dropped is fetched again. `content\` is never trimmed: `packages\` follows Roblox's live version ([client packages](../behavior/content/client-packages.md)), and `assets\` holds a fixed list ([kept content](#kept-content)).

Two things are deliberately elsewhere. The Open Cloud API key belongs in the OS credential store ([content policy](policies.md#content-and-assets)); until Rook reads one, it comes from `ROOK_OPEN_CLOUD_API_KEY`. A Roblox install's files are read where they are, never copied in.

`state\recent.json` (recent places, the window's size and place) has its path reserved (`DataDir::recent`). Nothing writes it yet.

### Older data folders

`logging::init` brings a folder older Rooks laid out up to date (`data_folder::migrate`), logging each change. Each step finds nothing to do on later runs.

- `settings.json` is removed. What still has a home moves to `GlobalBasicSettings.xml`, unless that file already holds it: `uncapped: true` becomes `FramerateCap` 0, `audioDevice` becomes `RookOutputDevice`, and `masterVolume` becomes `MasterVolume`. Every other key that was set is logged as dropped. A file that doesn't parse is dropped whole.
- A download cache in `%TEMP%\Rook\cache` (`~/Library/Caches/Rook` on macOS, `$XDG_CACHE_HOME/rook` elsewhere) moves into `cache\`, and the old folder is removed.
- What sat straight in `content\` is sorted: `blobs\` and `refs\` into `content\assets\`, the rest (Roblox's packages and their markers) into `content\packages\`.

## Settings

Every setting is one of three kinds. `rook-engine`'s `settings` module resolves them and hands plain values down (`rook_content::Config`, the audio device, the thread budget); no other crate reads the environment.

- **Build settings**: whoever builds Rook decides them, and they are compiled in.
- **The player's settings**: what a player chooses, on `UserSettings().GameSettings`, saved in `GlobalBasicSettings.xml` as Roblox's client saves its own ([user settings](../behavior/user-settings.md)).
- **Developer overrides**: flags and `ROOK_*` variables, which win over the other two only in builds that allow them.

### Build settings

`rook-build.toml` at the repository's root holds Rook's defaults. A build with `ROOK_BUILD_CONFIG=<file>` (relative to the repository, or absolute) reads that file over it: its keys win, and the keys it leaves out keep Rook's. `rook-engine`'s `build.rs` reads them into the constants of `build_config`; an unknown key or a bad value fails the build, naming the file, the key and what is wrong. TOML, like `Cargo.toml` and `clippy.toml`, because only the build reads it.

They are constants so that nothing on a player's machine changes them, and so the compiler leaves out what a build turns off: with `coreScripts = false` and no developer overrides, the CoreScripts loader is not in the program.

| Key | Default | What it does |
|---|---|---|
| `webBase` | left out | Where web requests go ([web endpoints](web-endpoints.md)). Left out, asset delivery and Roblox's packages use Roblox and every other API answers locally; else `"off"`, a domain (`example.com`) or a URL (`http://127.0.0.1:8080`) |
| `webDomains` | none | Per Roblox web domain (`users`, `assetdelivery`, ...), a base URL or `"off"`, winning over `webBase` |
| `coreScripts` | `false` | Run Roblox's own CoreScripts on each client ([CoreScripts](../behavior/gui/core-scripts.md)) |
| `contentOrder` | `defaults, install, folders, delivery` | Where content comes from, in order. `install` is Roblox's packages, downloaded as needed, then a Roblox install ([client packages](../behavior/content/client-packages.md)) |
| `assetDelivery` | left out | Whether content may be downloaded. Left out, each program decides: on in `rook-player` and `rook-tools content`, off in headless runs |
| `cacheMaxSize` | `"2GB"` | The cache's size limit, with a unit (`500MB`, `5GB`) |
| `threads` | left out | Logical cores to plan background threads for; left out, the machine's |
| `developerOverrides` | left out | Whether developer overrides work. Left out, on in debug builds and off in release builds |

### Developer overrides

In a build with `developerOverrides` (every debug build, unless its build settings say otherwise), these win. In other builds they are ignored, as if they weren't given.

| Variable or flag | Over | What it does |
|---|---|---|
| `ROOK_DATA_DIR` | the data folder | Everything under one folder |
| `ROOK_CACHE_DIR` | `cache\` | Where downloads are cached |
| `ROOK_CACHE_MAX` | `cacheMaxSize` | The cache's size limit (`500MB`; a bare number is MB) |
| `ROOK_CONTENT_ORDER` | `contentOrder` | Comma-separated |
| `ROOK_CONTENT_DIRS` | | Folders laid out like an install's `content` (content packages, local overrides), separated like `PATH` |
| `ROOK_ASSET_DELIVERY` | `assetDelivery` | `on` or `off` |
| `ROOK_ROBLOX_INSTALL` | the newest install found | A Roblox version or `content` folder, or `off` |
| `ROOK_THREADS` | `threads` | |
| `--core-scripts`, `ROOK_CORE_SCRIPTS` | `coreScripts` | The flag (`rook-player`, `rook-tools run-headless --join`), then the variable (`on` or `off`) |
| `ROOK_WEB_BASE` | `webBase` | |
| `ROOK_AUDIO` | the player's output device | `default`, `off` (`rook-player` opens no device), or a device's name |
| `rook-player --uncapped` | the player's `FramerateCap` | Present as fast as frames are drawn |
| `rook-player --frame-times` | | Print frame times every second and a summary at exit |

These work in every build: `--verbose` and `ROOK_LOG` ([logging](#logging)), and the Open Cloud key, `ROOK_OPEN_CLOUD_API_KEY`, tried for assets refused anonymously (it needs the `legacy-asset:manage` scope) and only ever sent to Roblox. `ROOK_STUDIO`, which picks the Studio window `rook-tools conformance` drives, is a variable of `rook-cli` alone.

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
- **`--verbose`:** `debug` and up: every failure, with instance path, content reference and reason (`Workspace.Ruins.Union: rbxassetid://123: union geometry CSGMDL3 is not supported yet`).
- **`--verbose <subsystem>`** (or several, comma-separated): everything those subsystems log, at any level, plus the usual `info`. `--verbose content` before a file name is taken as the subsystem; a bare `--verbose` before a file name is not (`logging::fix_verbose_args`).

Libraries' own events reach the terminal only as warnings and errors.

### Output off the game thread

Printing a line costs the thread that printed it only the crash report's copy (`logging::crash::remember`) and a channel send. A thread of its own, `rook-output` (`console::writer`), writes the terminal and the log file, in the order lines were printed. A Windows console write takes tens of microseconds and the log file flushes every line, so a game that prints or errors every frame used to pay both in frame time. A Baseplate whose script prints 20 lines and raises 5 errors every Heartbeat, run with `--bench 600` in a Windows console, took a median 6.1 to 7.2 ms a server step before and 0.26 to 0.33 ms after (2026-10-26); with output to a pipe, simulation went from 0.72 to 0.52 ms a frame. The crash report's tail stays on the printing thread, so a report still has the lines printed just before a panic. `console::flush` waits for the thread; `logging::run` calls it when `main` returns or panics, and commands that print straight to stdout after the game's output (`rook-tools run`'s tree and summary) call it first. Lines for the REPL's prompt (`set_output`) go out from that thread too.

What Roblox does, measured in Studio 0.744 (Baseplate, play mode, 2026-10-26):

- **No throttling or collapsing.** 5,000 identical `print`s each cost as much as 5,000 different ones (about 50 µs a call on the server, 110 µs on the client), and every one reached the Output window and the log file. Rook doesn't collapse repeats either.
- **Studio pays on its main thread.** At 2,000 prints a frame the server's median Heartbeat step was 1.6 s instead of 3 ms; the log file had written those lines within 150 ms, so the Output window, not the file, is the cost. Frames stayed slow (0.44 s) for seconds after the printing stopped while the window caught up, and the client drew almost no frames meanwhile.
- **The log file** (`%LOCALAPPDATA%\Roblox\logs\*_Studio_*_last.log`) gets every line as `<UTC time>,<seconds>,<thread id hex>,6,Info [FLog::CreatorOutput] <message>` (`Warning [FLog::CreatorWarning] Warning: ...`, `Error [FLog::CreatorError] Error: ...`). The thread id is the printing script's, and the time advances call by call, so each line is stamped when printed; whether the disk write happens later can't be seen. An error is three lines: the message, `Info: Stack Begin` and `Info: Stack End` (with the stack between when there is one).
- **`LogService.MessageOut` is deferred:** none of 5,000 prints fired it during the loop that printed them; all fired after the script yielded. `GetLogHistory` keeps the last 512 messages. Rook has neither yet.

What a live client does can't be measured here; nothing public says it writes output differently.

### Log files

Each run of the player writes one file, `logs\rook-player-<time>.log`. `rook-tools` writes one only for long commands (`corpus-scan`, `corpus-run`, `conformance`). The file gets Rook's `debug` and up whatever the terminal shows, plus every line the terminal printed, script output included. It is plain text without colors, in the terminal's line format. At startup, old logs are cleaned: a file is deleted once it is beyond the newest 30 runs and older than 14 days, and the oldest go first past 100 MB in total. Crash reports are cleaned by the same rules.

`rook-tools logs` prints the newest log's path. `rook-tools logs --open` opens the folder.

`rook-player` is a console program, so a terminal waits for it and it prints there, but opened from Explorer it has no console, so the log file is the only record there. Its manifest (`apps/rook-player/rook-player.manifest`) asks Windows 11 24H2 and later not to make one (the [console allocation policy](https://learn.microsoft.com/en-us/windows/console/console-allocation-policy)); on older Windows it closes the one it was given at startup when no other program shares it (`apps/rook-player/src/console_window.rs`). What ends a windowed run is shown in a native message box, what failed as its title and why as its text (`apps/rook-player/src/alert.rs`): a place that can't be read or fails to start, a GPU Rook can't draw with, a crash. Rook has no CoreGui of its own yet to show these in its window. Runs that make a file or a measurement (`--screenshot`, `--bench`, `--record-audio`) only print, so scripts aren't held up.

### Crash reports

`logging::init` installs a panic hook that notes each panic: message, place, thread and a backtrace. Rook catches some panics on purpose, such as a decoder that panics on a damaged file, so the hook writes nothing itself. The program's `main` runs inside `logging::run`. When a panic gets there, `run` writes `logs\crashes\<program>-<time>.txt`, prints where it is and hands that line to the program (the player shows it in a message box), then exits with code 101. The report holds the panic, the backtrace, the OS, the GPU and driver (once a renderer exists), the log file's path and the last 200 lines logged. `rook-player --crash-test` (hidden) panics after loading, to check this.

## Failures carry their reason

Content failures are `rook_content::Failure`: a `FailureKind` (`not-found`, `forbidden`, `unsupported`, `offline`, `network`, `invalid`, `unknown-version`) and a message. The decoders return typed errors (`MeshError`, `ImageError`, `AudioError`). The loader turns them into failures, keeping "a version Rook can't read yet" (`unknown-version`) apart from "damaged or not that kind of file" (`invalid`). `rook-view`'s scene keeps each failed file's failure and logs it where the instance path is known (`rook_view::content::report`). See the error rule in [CONTRIBUTING](../../CONTRIBUTING.md#errors).

`rook-tools content <place>` lists every content reference in a place and what became of it: where it came from, or why it didn't load or decode.

## Kept content

Some content Rook loads itself in nearly every place: the built-in materials' texture packs, terrain's packs, and the images those name. These are kept for good in `content\assets\` rather than the cache. The list is fixed (`crates/rook-content/data/kept.txt`), never what places happen to load. Each asset is pinned to a version, because an asset id can be uploaded again and a version never changes. Asset delivery gives the version in its `roblox-assetversionnumber` header.

A request for a kept asset by its bare id is answered at the pinned version: from `content\assets\` first, then the cache (copying it there), then asset delivery, which writes it there. It is never trimmed. It has the cache's layout, blobs by hash and one ref file per key (`asset-<id>-v<version>`), so one reader handles both. `rook-tools content --pin` writes the list again from the materials' and terrain's tables with each asset's version today. A test checks that every pack in those tables is listed.

The default sky is not kept. It comes from Roblox's packages or install, and without one Rook draws its own plain sky.
