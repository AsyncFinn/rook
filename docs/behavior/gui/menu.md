# Rook's in-game menu

Checked against Studio 0.748 on 2026-11-23.

[`menu/studio-0.748.json`](menu/studio-0.748.json) records Roblox's menu in a Studio client, at 1760 by 990 and at 630 by
990. [`menu/dump.luau`](menu/dump.luau), the script that recorded it, can
take it again. The recording holds only measured values: no script
source and no asset ids.

The menu is a Rook extension ([behavior provenance](../../design/architecture.md#behavior-provenance)):
Roblox's own comes from its CoreScripts. What games see of it
(`GuiService`'s members, `SetCore("TopbarEnabled")`) follows
creator-docs. Its layout, colors and motion come from the recording.
Where Rook differs, this page says so and why.

It is the first piece of Rook's own CoreScripts
([Rook's own](core-scripts.md#rooks-own)): the top bar's button and the menu it
opens. Roblox's menu comes from its CoreScripts, which only builds with
developer overrides can run, so without them a player had no menu:
Escape closed `rook-player`.

Code: `crates/rook-engine/corescripts/`. `TopBar/` holds the button,
`Menu/` the menu (`MenuApp.lua`, `LeaveConfirm.lua`, `Pages/`),
`Components/` the rows and controls, with every measured value in
`Components/Theme.lua`, and `PlayerSettings/` the settings. What the
menu calls on the engine (`GuiService:SetMenuIsOpen`,
`DataModel:Shutdown`) is in `rook-lua`'s `api/methods/core/menu.rs`.
The window's Escape and Leave are in `rook-client`. The test
`the_layout_is_roblox_s` (`rook-engine`'s `tests/gui/menu.rs`) checks
the layout against the recording at both sizes.

## When it runs

- A client runs Rook's CoreScripts when it doesn't run Roblox's, whose
  menu takes its place. The `menu` setting turns them off (`rook-player
  --no-menu`, `ROOK_MENU=off`; [settings](../../design/data-and-logging.md#run-time-settings));
  Escape then closes the player, as before. Headless runs and tests leave
  them out unless their `Options` ask for them.
- They run as CoreScripts, with Roblox's own scripts' identity, so they
  can change the player's settings, which game scripts can't
  ([user settings](../user-settings.md)).
- The button is in the `TopBarApp` ScreenGui and the menu in `RookMenu`,
  both in CoreGui (`IgnoreGuiInset`, `ZIndexBehavior` Sibling), out of
  game scripts' reach.

## Layout

Each value is from the recording, named by the Roblox instance it was
read from; `Components/Theme.lua` lists them. Positions are window
pixels.

- **The button** (`IconHitArea`): 44 by 44 at (16, 12), round, (18, 18,
  21) at 0.016. Hovered, an overlay of (208, 217, 251) at 0.92 covers
  it. Its icon is 24 pixels, and 30 while the menu is open. It doesn't
  jump between them: it springs there, critically damped (no overshoot)
  at 1 / 0.35 cycles a second, so it settles in about 0.4 s, as Roblox's
  `ScalingIcon` does (Studio 0.748's `TopBar` IconButton). Roblox shows
  its own logo from an icon font; Rook shows its logo, drawn from
  `resources/rook.svg` without its tile.
- **The backdrop** (`DarkenBackground`): black at 0.5. It is a button,
  so the game's buttons behind it get no clicks while the menu opens.
- **The panel** (`MenuContainer`): 840 by 734, black at 0.06, round by
  10, centered 10 below the window's middle. It is padded 20 at the
  sides and 14 at the bottom. Top to bottom it holds:
  - the tab bar (`HubBar`, 60 tall, a white line at 0.8 under it);
  - the page (`PageViewClipper`, 600 tall), which scrolls in a frame 20
    shorter, 2 below its middle, with a 1-pixel scroll bar. The page is
    padded 5 at the top, 12 at the left and 11 at the right, so rows
    are 776 wide;
  - the buttons along the bottom (`BottomButtonFrame`, 60 tall).
- **Tabs**: each takes a fifth of the bar (160 by 60). It holds a
  24-pixel icon, then 10 pixels, then its name in BuilderSans Medium 19.
  The selected tab has a 2-pixel white line under it; the others are at
  0.5. Rook's icons are [Lucide](https://lucide.dev)'s (`users`,
  `settings`, `circle-question-mark`); Roblox's come from its own icon
  font.
- **Rows** (`<name>Frame`): 50 tall, or taller with a description,
  round by 8. The name is BuilderSans Medium 17, 10 in, taking 0.4 of
  the row less 20, padded 10 above and below. A description goes 8
  under it, in Medium 12 at 0.25. The control takes 0.6 of the row at
  the right, centered on its height.
  - The row the mouse enters fills with (35, 37, 39) at once, and stays
    lit until the mouse enters another row, even after it leaves for
    the backdrop.
- **Sections**: a header 33 tall, its text 10 in with 8 below
  (`HeaderWrapper`). Between sections, a 1-pixel line of (208, 217, 251)
  at 0.88, 10 in, with 4 above and 16 below (`DividerWrapper`).
  - Inference: Roblox sets the header's font through a style sheet, which
    the recording doesn't read; its text is 25 tall, which BuilderSans
    Bold 20 fills.
- **Selectors** (`Selector`): arrows 32 by 50 at either end, in (204,
  204, 204). The choice is between them in BuilderSans Regular 17. The
  arrows, or the choice itself, step through the choices, wrapping round.
- **Sliders** (`Slider`): ten steps (`StepsContainer`, the slider less
  100), each a tenth less 4, 24 tall. A step is (217, 217, 217) up to
  the value and (57, 59, 61) after, round at the slider's outer ends.
  Minus and plus buttons, 32 by 50 with a 30-pixel icon in (204, 204,
  204), sit at the ends. Minus hides at 0 and plus at the top.
  - Pressing on the steps sets the value under the cursor. The value
    follows the cursor while the button is held, anywhere on the screen.
    Left of the first step is 0.
- **Camera Sensitivity**'s slider ends 80 short of the row. Its box
  (`CameraSensitivityTextBox`) sits 10 past that: 60 wide, 0.8 of the
  slider's 44, (51, 51, 51), BuilderSans Regular 14.
- **Dropdowns** (`DropDownFrameButton`): 40 tall, (35, 37, 39), round by
  8 and outlined white at 0.8. The choice is 15 in, Medium 17 at 0.3,
  with an arrow 12 from the right. Its list opens under it, a row 50
  tall for each choice.
  - Roblox opens a separate list; how it looks open wasn't recorded.
- **Bottom buttons**: three, 48 tall, 8 apart, centered in their frame.
  Leave and Respawn are (208, 217, 251) at 0.88; Resume is (51, 95, 255)
  at 0.6. A hovered button's transparency is 0.92 of it.
  - Inference: Roblox's buttons set their text and key images through
    style sheets, which weren't recorded. Rook's show the key's name in
    an outlined box, then the name in BuilderSans Bold 17.

### A narrow window

At 630 wide Roblox's panel takes the whole width. Rook does the same
below 880 (Inference: the panel's 840 and its margins; Roblox's switch
point wasn't recorded). In that window:

- The tab bar is 54 tall. The icons shrink to 14 and sit above the names.
- The page is the window's height less 266 tall.
- The bottom frame is 54 tall.
- The panel is still round by 10 and still 10 below the middle.
- Row names drop to 12; choices stay 17.

### Opening and closing

Opening, `SettingsShield`, the screen-sized frame holding the panel,
starts a window's height and 36 below its place and slides up over 0.5 s
(Quart InOut). At the same time the backdrop shows and fades from clear
to 0.5 over 0.5 s (Quad Out). Games hear `MenuOpened` at once.

Closing, the backdrop hides at once and the shield slides back down over
0.4 s (Quad In). Games hear `MenuClosed`, and `MenuIsOpen` turns false,
when it's gone.

## Pages

The menu opens on People, as Roblox's does.

- **People**: everyone in the server, by display name and `@name`, the
  local player first.
  - Roblox's page also offers friending, blocking and voice controls,
    which Rook has no service behind.
- **Settings**: see below.
- **Help**: the keyboard and mouse controls of Roblox's PlayerModule.
- Roblox also has Gallery (screenshots) and Report (abuse reports). Rook
  takes no screenshots and has nowhere to send reports, so it leaves
  them out.

## Settings

Roblox's Settings page has four sections: Audio, Chat & Language,
Display & Graphics, and View & Controls. Rook shows the rows it acts on,
under Roblox's headers and in Roblox's order. Each row keeps Roblox's
LayoutOrder and name, so rows added later fall into place. Chat &
Language has none of Rook's rows yet, so it isn't shown.

Each row changes the player's `UserSettings().GameSettings`, which
`rook-player` follows and saves ([user settings](../user-settings.md)):

| Row | Control | Sets | Choices |
|---|---|---|---|
| Volume | Slider | `MasterVolume` | 0 to 1 in tenths |
| Output Device | Selector | `SoundService:SetOutputDevice` | `SoundService:GetOutputDevices`, the system's default first as "Default (*name*)" |
| Fullscreen | Selector | `GuiService:ToggleFullscreen` | Off, On |
| Maximum Frame Rate | Dropdown, with a description | `FramerateCap` | Default (-1, the display's rate), then 60, 120, 144, 160, 165, 180, 200 and 240 FPS |
| Camera Inverted | Selector | `CameraYInverted` | Off, On |
| Camera Sensitivity | Slider and box | `MouseSensitivity`, and `MouseSensitivityFirstPerson` and `ThirdPerson` as (*v*, *v*) | Slider steps 0.4 apart, 0.1 at its bottom and 4 at its top. A number typed in the box is kept between 0.1 and 4 |

- **Maximum Frame Rate**: Roblox lists its default, then the rates up to
  the display's. Studio 0.748 on a 240 Hz display listed those above.
  Rook can't read the display's rate, so it lists them all. The
  description is Rook's own wording.
- **Camera Sensitivity**: the recording doesn't show what Roblox's box
  takes beyond the slider's range, so typed values are kept to that
  range.
- A setting the engine refuses shows its default and warns, rather than
  stopping the menu.

## Keys and buttons

- Escape, or the button, opens the menu; Escape, the button or Resume
  closes it. Escape that ends typing in a TextBox is the TextBox's.
- L, or Leave, asks "Are you sure you want to leave?" with Don't Leave
  (Escape) and Leave (Enter). Leaving calls `game:Shutdown()`, and the
  window closes after that frame, saving the player's settings.
- Respawn (R) is shown faded and does nothing: Rook has no death to
  respawn from yet ([M9](../../roadmap/m9-characters.md)).
- While the menu is open, a Modal button covering the shield frees the
  mouse, as Roblox's `SettingsShield.Modal` does. W, A, S, D, Space and
  the arrow keys are kept from the game's ContextActionService bindings
  (a core action at priority 3000 that sinks them). UserInputService
  still hears them, as in Roblox (Inference).

## What games see

- `GuiService.MenuIsOpen` is true while the menu is open. `MenuOpened`
  and `MenuClosed` fire as it opens and once it has closed (creator-docs
  GuiService).
- Game scripts calling `SetMenuIsOpen` get "lacking capability
  RobloxScript". Calling `game:Shutdown()` gets "lacking capability
  LocalUser".
- `StarterGui:SetCore("TopbarEnabled", false)` hides the button, and
  `GetCore("TopbarEnabled")` reads it; Escape still opens the menu
  (creator-docs StarterGui:SetCore). Rook's `CoreApi` module answers it,
  the one place Rook's CoreScripts register SetCore names. Other `SetCore`
  names still aren't registered without Roblox's CoreScripts.
