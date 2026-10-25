# Client packages

Checked against Studio 0.744 on 2026-10-24.

Roblox's client files (fonts, textures, sky, sounds, the CoreScripts model) are what `rbxasset://` paths name. Rook never ships them ([content policy](../../design/policies.md#content-and-assets)); like Roblox's own bootstrapper, it downloads the packages they come in from Roblox's deployment CDN onto your machine, one package at a time as a path needs it, so Rook runs with no Roblox or Studio install. An install, when there is one, is the fallback.

Code: `rook-content`'s `resolver/packages.rs`, asked by the resolver's `install` provider before the install (`resolver.rs`), and by `rook-engine`'s `settings::core_scripts` for the CoreScripts model.

## How Roblox deploys a client

Checked with requests to Roblox's hosts and against the installs under `%LOCALAPPDATA%\Roblox\Versions` on 2026-10-23.

- The live version comes from `GET https://clientsettings.roblox.com/v2/client-version/<channel>`, which answers `{"version": "0.744.0.7441077", "clientVersionUpload": "version-7aaa6838d36c4448", "bootstrapperVersion": ""}`. The channel is `WindowsPlayer` for the player and `WindowsStudio64` for Studio; the two are built separately (player `version-9ac8595712b447a7`, Studio `version-7aaa6838d36c4448` that day). Bloxstrap asks the same endpoint. An install's folder is named after `clientVersionUpload`.
- `GET https://setup.rbxcdn.com/<version>-rbxPkgManifest.txt` lists that version's packages: `v0`, then four lines per package (its file name, the MD5 of the zip, its packed and unpacked size). Each package is `GET https://setup.rbxcdn.com/<version>-<name>`, and its MD5 matches the manifest's.
- A package's zip names its files relative to the one folder it extracts to, with backslashes (`\DataModelPatchConfig\DataModelPatchConfig.json`), and holds an entry for that folder itself (`\`).
- The player's manifest has 22 packages, Studio's 32. The content ones are in both, and fonts, textures, sky, sounds and terrain are the same files (the same MD5); Studio's also carry its own plugins, Qt and its UI's textures.

Where each content package extracts to in an install (Bloxstrap's table, confirmed against the zips and installs), and where Rook puts it:

| Package | In an install | In Rook's content folder |
|---|---|---|
| `content-avatar.zip` | `content\avatar` | `avatar` |
| `content-configs.zip` | `content\configs` | `configs` |
| `content-fonts.zip` | `content\fonts` | `fonts` |
| `content-models.zip` | `content\models` | `models` |
| `content-sky.zip` | `content\sky` | `sky` |
| `content-sounds.zip` | `content\sounds` | `sounds` |
| `content-textures2.zip` | `content\textures` | `textures` |
| `extracontent-luapackages.zip` | `ExtraContent\LuaPackages` | `ExtraContent\LuaPackages` |
| `extracontent-models.zip` | `ExtraContent\models` | `ExtraContent\models` |
| `extracontent-textures.zip` | `ExtraContent\textures` | `ExtraContent\textures` |
| `extracontent-translations.zip` | `ExtraContent\translations` | `ExtraContent\translations` |
| `content-platform-dictionaries.zip` | `PlatformContent\pc\shared_compression_dictionaries` | the same |
| `content-platform-fonts.zip` | `PlatformContent\pc\fonts` | the same |
| `content-terrain.zip` | `PlatformContent\pc\terrain` | the same |
| `content-textures3.zip` | `PlatformContent\pc\textures` | the same |

The rest hold programs and their libraries (`RobloxApp.zip`, `RobloxStudio.zip`, `shaders.zip`, `ssl.zip`, ...) or files only the player or Studio use (`extracontent-places.zip`, `extracontent-scripts.zip`, the plain-Luau CoreScripts), and Rook never downloads them.

## The CoreScripts model

- Studio's `extracontent-models.zip` (16.6 MB) holds `DataModelPatch\DataModelPatch.rbxm`, the model Rook runs the CoreScripts from ([CoreScripts](../gui/core-scripts.md)).
- The player's `extracontent-models.zip` (27.7 MB) has no `DataModelPatch`; it holds `InExperience\InExperience.rbxm` (with an `InExperience_checksum`) instead, the model `content\configs\InExperiencePatchConfig` names. Rook doesn't run it yet: its `StarterScript` fails with "attempt to load a text chunk", and nothing else starts.

So Rook follows Studio's channel, `WindowsStudio64`: its content packages are the player's, and it has the model Rook runs.

## What Rook does

- **On first need.** Resolving an `rbxasset://` path looks in the folders an install would (`content`, then `ExtraContent`, then `PlatformContent\pc`); where the path's first folder belongs to a package not yet downloaded, Rook downloads that package, checks its MD5 and extracts it, then serves the file. Each download is announced in the terminal and the log (`downloading Roblox's content-fonts.zip (5.9 MB) for version-... into ...`). A package that fails is warned about once and not tried again that run; the install answers instead.
- **Where.** In the data folder's `content\packages`, `%LOCALAPPDATA%\Rook\content\packages` on Windows, beside the content Rook keeps for good (`content\assets`). The install's `content` folder is the top of it, so paths read as they do under `rbxasset://` (`fonts\`, `textures\`), and `ExtraContent` and `PlatformContent\pc` sit below as in an install (see the table). `version.txt` names the version the packages are from, and `manifest.txt` is its manifest, so it isn't downloaded again.
- **Following the live version.** Rook asks for the live version once per run, the first time a path needs a package. When it differs from `version.txt`, the package folders are removed, `version.txt` and `manifest.txt` are written for the new version, and packages download again as paths need them, so two versions' files never mix. A folder still open in another running Rook can't be removed: that is reported, the run keeps to the packages already there (then the install), and the next run tries again.
- **A second run** with the same live version downloads nothing: it asks for the version (a small request) and finds every package it needs in place.
- **Which programs download.** Those that download content (`rook-player`, `rook-tools content`, or any with `assetDelivery` on). Headless runs and tests don't, but use what is already downloaded.
- **Turning it off.** Downloads go through the [web endpoints](../../design/web-endpoints.md): the `clientsettings` and `setup` domains. `setup = "off"` under the build settings' `[webDomains]` (or `webBase = "off"`) stops them; a mirror given there serves them instead. Off or offline, the packages already downloaded still answer, then the install, exactly as an install alone did.
- **CoreScripts.** `--core-scripts` takes `DataModelPatch.rbxm` from the packages first, then from the install `robloxInstall` names or the newest Studio install. With neither, the client runs without Roblox's UI, and a warning says why (packages off, not downloaded in this program, or failed).
- Fast flags still come from Studio's `ClientSettings\StudioAppSettings.json` when it's there ([CoreScripts](../gui/core-scripts.md#where-they-are)); without it the scripts take their flags' defaults.
