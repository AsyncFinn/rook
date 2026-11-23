# Rook's in-game menu

Checked against Studio 0.744 on 2026-11-22, beside Roblox's in-game menu.

The menu is a Rook extension ([behavior provenance](../../design/architecture.md#behavior-provenance)):
Roblox's own comes from its CoreScripts. What games see of it
(`GuiService`'s members, `SetCore("TopbarEnabled")`) follows
creator-docs; where it copies the look or behavior of Roblox's menu,
that's tagged Approximation or Inference below, since no creator-docs
page or conformance recording covers Roblox's menu.

[M8](../../roadmap/m8-gui.md) step 9's first piece of Rook's own
CoreGui: the logo button at the top left and the menu it opens, with
the player's settings, Leave and Resume. Roblox's menu comes from its
CoreScripts ([Roblox's core scripts](core-scripts.md)), which only
builds with developer overrides can run, so without them a player had
no menu: Escape closed `rook-player`. Rook's is Luau shipped in the
engine, with Rook's logo and no art of Roblox's.

Code: the script in `rook-engine`'s `menu/menu.luau`, started by
`menu.rs`; what it calls on the engine (`GuiService:SetMenuIsOpen`,
`DataModel:Shutdown`) in `rook-lua`'s `api/methods/core/menu.rs`; the
window's Escape and Leave in `rook-client`.

## When it runs

- A client runs it when it doesn't run Roblox's CoreScripts, whose menu
  takes its place. The `menu` setting turns it off (`rook-player
  --no-menu`, `ROOK_MENU=off`; [settings](../../design/data-and-logging.md#run-time-settings));
  Escape then closes the player, as before. Headless runs and tests leave
  it out unless their `Options` ask for it.
- It runs as a CoreScript (`RookMenuScript` under CoreGui) before the
  client's LocalScripts start, with Roblox's own scripts' identity, so it
  can change the player's settings, which game scripts can't
  ([user settings](../user-settings.md)).
- Its GUI is the `RookMenu` ScreenGui in CoreGui (`IgnoreGuiInset`,
  `ZIndexBehavior` Sibling), out of game scripts' reach.

## Layout

Sizes and colors follow Roblox's menu as a Studio client drew it (1732
by 1121 window) on 2026-11-22 (Approximation: taken from how it looks,
not measured in isolation or recorded):

- The button: 44 by 44 at (16, 12), round, (18, 18, 21) at
  BackgroundTransparency 0.02, with a 24-pixel logo in it: Roblox's menu
  button is the same, with its own logo. It stays bright over the open
  menu, and lightens while hovered.
- The screen darkens (black at 0.5) under a button covering it, as
  Roblox's `SettingsShield` is an ImageButton (Inference): the topmost button under
  the cursor takes a click whatever covers it ([input](input.md#buttons)),
  so the game's buttons behind the menu get none. The panel is 840 by 734, black
  at 0.06, round by 10, centered; in a smaller window it keeps 20 pixels
  from each edge and the page scrolls.
- The tab bar is 60 tall, 20 in from the panel's sides, over a white
  line at 0.8; the one tab, Settings, is 160 wide and underlined in
  white. Roblox's has People, Settings, Gallery, Report and Help; Rook's
  has only what works.
- Rows are 50 tall and 777 wide, round by 8, and fill with
  (33, 36, 38) while hovered. The name is BuilderSans Medium 17 at 10
  from the left in 290; the control takes the rest from 310. Sections
  start with a BuilderSans Bold 20 header, as Roblox's newer menu has.
- A choice is shown in BuilderSans Regular 17 between two arrows 32 wide;
  the arrows, or the value itself, step through the choices, wrapping
  round.
- Camera Sensitivity's slider ends in a box, 60 by 34 at the row's
  right end (white at 0.92, round by 8), after the plus, as Roblox's
  row does. It empties when clicked, showing the value faintly, and
  takes the new one on Enter or a click elsewhere; what isn't a number
  from 0.01 leaves the value as it was. Roblox's box stops at 4 (Inference);
  Rook's doesn't, and the slider then shows all ten steps.
- A slider is ten steps, 24 tall, light (217, 217, 217) up to the value
  and dark (57, 59, 61) after, round at the ends, between circled minus
  and plus buttons 32 wide. Pressing on the steps sets the value under
  the cursor, and it follows the cursor while the button is held,
  anywhere on the screen; left of the first step is 0. Minus and plus
  move one step.
- The buttons along the bottom are 48 tall, 8 apart, 20 from the panel's
  bottom: Leave and Respawn (207, 217, 250) at 0.88, Resume (51, 95, 255)
  at 0.6, each with its key in an outlined box before its name. Roblox
  draws key images; Rook outlines the key's name.
- The logo is built from frames, the white rook of `resources/rook.svg`
  (battlements, tower with its window, base) turned by 10° as the icon
  is; the tower is straight where the icon's narrows.

## Settings

Each row changes the player's `UserSettings().GameSettings`, which
`rook-player` follows and saves ([user settings](../user-settings.md)):

| Row | Sets | Choices |
|---|---|---|
| Volume | `MasterVolume` | 0 to 1 in tenths |
| Output Device | `SoundService:SetOutputDevice` | `SoundService:GetOutputDevices`, read again each time the menu opens, the system's default first as "Default (*name*)" |
| Fullscreen | `GuiService:ToggleFullscreen` | Off, On; follows `FullscreenChanged` |
| Maximum Frame Rate | `FramerateCap` | Match display (-1), 30, 60, 120, 144, 240, Unlimited (0) |
| Camera Sensitivity | `MouseSensitivity`, and `MouseSensitivityFirstPerson` and `ThirdPerson` as (*v*, *v*) | A slider of steps 0.4 apart (0.1 at its bottom, 4 at its top), and any number from 0.01 typed in its box, beyond 4 too |
| Camera Inverted | `CameraYInverted` | Off, On |

Only settings Rook acts on are offered. A setting the engine refuses
shows its default and warns, rather than stopping the menu.

## Keys and buttons

- Escape, or the button, opens the menu; Escape, the button or Resume
  closes it. Escape that ends typing in a TextBox is the TextBox's.
- L, or Leave, asks "Are you sure you want to leave?" with Don't Leave
  (Escape) and Leave (Enter). Leaving calls `game:Shutdown()`, and the
  window closes after that frame, saving the player's settings.
- Respawn (R) is shown grayed out and does nothing: Rook has no death to
  respawn from yet ([M9](../../roadmap/m9-characters.md)).
- While the menu is open, a Modal button frees the mouse, as Roblox's
  menu does (Inference), and W, A, S, D, Space and the arrow keys are kept from the
  game's ContextActionService bindings (a core action at priority 3000
  that sinks them). UserInputService still hears them, as in Roblox (Inference).

## What games see

- `GuiService.MenuIsOpen` is true while it's open, and `MenuOpened` and
  `MenuClosed` fire as it opens and closes (creator-docs GuiService).
  Game scripts calling `SetMenuIsOpen` get "lacking capability
  RobloxScript", and `game:Shutdown()` "lacking capability LocalUser".
- `StarterGui:SetCore("TopbarEnabled", false)` hides the button, and
  `GetCore("TopbarEnabled")` reads it; Escape still opens the menu. Other
  `SetCore` names still aren't registered without Roblox's CoreScripts.
