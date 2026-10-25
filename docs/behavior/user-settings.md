# User settings

Checked against Studio 0.744 on 2026-10-23.

The player's own settings: `UserSettings().GameSettings`, the
UserGameSettings the in-game menu's Settings page edits. Read from the
Roblox client's saved file (by hand) and Studio 0.744's CoreScripts on 2026-10-23,
and run headless with `rook-tools run-headless --join --core-scripts
--user-settings <file>` on an empty baseplate.

Code: the instance and its defaults in `rook-lua`'s `user_settings.rs`,
its members in `api/methods/user_settings.rs`; loading and saving in
`rook-engine`'s `user_settings.rs`; the window following it in
`rook-player`'s `app.rs`.

## Where Roblox keeps them

- The client saves UserGameSettings to
  `%LOCALAPPDATA%\Roblox\GlobalBasicSettings_13.xml` as place-file XML:
  `<roblox version="4">` holding one `<Item class="UserGameSettings">`
  named `GameSettings`, with every property that serializes
  (`MasterVolume`, `MouseSensitivity`, `MouseSensitivityFirstPerson` and
  `ThirdPerson`, `CameraYInverted`, `Fullscreen`, `GraphicsQualityLevel`,
  `SavedQualityLevel`, `ComputerMovementMode`, `ChatTranslationEnabled`,
  the window's `StartScreenPosition` and `StartScreenSize`, ...), the
  Instance ones included (`Capabilities`, `SourceAssetId`, `Tags`).
- `UserSettings()` sits outside the DataModel; `GameSettings` is its
  child, there before any script asks for it.
- Nearly all its properties are `RobloxScriptSecurity` (the API dump):
  Roblox's own CoreScripts read and write them, and a game's scripts get
  "lacking capability RobloxScriptSecurity". rbx-dom's database calls
  such properties not scriptable, so Rook takes who may use a property
  from the API dump's security instead, for every class.

## Rook's file

- Rook saves them the same way, as `GlobalBasicSettings.xml` in its data
  folder ([data folder](../design/data-and-logging.md#the-data-folder)),
  and loads it into each `rook-player` client before scripts start.
  Properties that don't serialize (`RotationType`,
  `IsUsingCameraYInverted`) aren't saved, as in Roblox's file.
- Without that file, Rook starts from Roblox's defaults (a fresh
  client's file: volume and sensitivity 1, windowed, automatic
  graphics), and writes it the first time something changes. It never
  reads the Roblox client's file; the shared format lets a launcher
  bring those settings over.
- Properties Rook's API snapshot doesn't know are kept as the file had
  them and written back.
- A change is saved once nothing has changed for a second of game time
  (a dragged slider writes the file once), and on exit.
- Only a window's player saves; headless runs, `--screenshot` and
  `--bench` keep the defaults unless `run-headless --user-settings`
  names a file.

## What Rook follows

- `MasterVolume`: everything heard. `rook-player --volume` holds for
  the run until the menu changes the volume.
- `FramerateCap`: how often `rook-player` draws. A fresh client's file
  has -1, and the API has `GetDefaultFramerateCap`, so -1 is read as
  Roblox's default, which Rook takes as the display's rate (Inference:
  not seen in Studio, whose Settings page offers no frame rate). A cap
  above 0 draws at most that many frames a second without waiting for
  the display, and 0 draws as fast as it can (Rook's reading; older
  Rooks' `uncapped` becomes 0). `rook-player --uncapped` overrides it
  for a run, in builds with developer overrides.
- `RookOutputDevice`, Rook's own: the name of the output device
  `rook-player` plays through, the system's default when it is missing
  or empty. Roblox's UserGameSettings has no such property (its client
  picks a device with `SoundService:SetOutputDevice`), so Rook keeps
  one beside Roblox's in the same file, where a launcher can set it.
  Nothing in the menu sets it yet; older Rooks' `audioDevice` moves
  here, and `ROOK_AUDIO` overrides it in builds with developer
  overrides.
- `MouseSensitivityThirdPerson` (the menu's Camera Sensitivity slider
  sets it and the first-person one) and `CameraYInverted`: how fast and
  which way `rook-player`'s camera turns with the mouse.
  `UserGameSettings:GetCameraYInvertValue()` is -1 when inverted, 1
  otherwise.
- `Fullscreen`: `rook-player`'s window fills the screen (borderless).
  The menu's switch calls `GuiService:ToggleFullscreen()`, which flips
  it and fires `FullscreenChanged`; `InFullScreen()` reads it
  (Approximation: Roblox reports the window itself).
- Everything else is stored, saved and readable, with nothing in Rook
  acting on it yet.

## The Settings page

Opening the menu (the logo button, then the Settings tab) shows, in
order: Volume, In-game friends chat notifications, Fullscreen,
Background transparency, Text size, Performance Stats, MicroProfiler,
Graphics Mode, Graphics Quality, Haptics, Reduce motion, Camera Mode,
Camera Sensitivity (with its number box), Movement Mode, Shift Lock
Switch and UI navigation toggle. Changing Volume and Fullscreen, then
loading again, shows them kept.

What doesn't work yet:

- Camera Inverted isn't offered: the menu shows it once the camera
  scripts call `SetCameraYInvertVisible`, and Rook doesn't run the
  PlayerModule yet ([M9](../roadmap/m9-characters.md)).
- Camera Mode, Movement Mode and Shift Lock Switch show "Set by
  Developer", and nothing moves a character by them yet.
- Graphics Mode and Graphics Quality are saved, but Rook's renderer has
  no quality levels to follow.
- Performance Stats, MicroProfiler and Haptics are saved with nothing
  behind them.
