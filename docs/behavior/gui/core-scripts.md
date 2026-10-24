# Roblox's core scripts

Checked against Studio 0.744 on 2026-10-23.

[M8](../../roadmap/m8-gui.md) step 9: the GUI Roblox draws itself (top
bar, player list, backpack, menus, prompts, chat) comes from Roblox's own
Luau CoreScripts, which Rook runs from the user's Studio install and never
ships ([content policy](../../design/policies.md#content-and-assets)). Read
from Studio 0.744's install on 2026-10-23 and run headless with
`rook-tools run-headless --join --core-scripts` on an empty baseplate;
looks from `rook-player --core-scripts --screenshot`.

Code: loading in `rook-engine`'s `core_scripts.rs` (and the
`coreScripts` setting in `settings.rs`), the compiled scripts in
`rook-formats`' `compiled.rs`, running them in `rook-lua`'s
`api/scripts.rs`, their members in `api/methods/core.rs`,
`core_gui.rs` and `localization.rs`, styling in `styles.rs`.

## Where they are

- Studio's `ExtraContent/scripts/CoreScripts` (2,425 files) and
  `ExtraContent/LuaPackages` (5,461) are plain Luau, but leave out every
  package under `CorePackages.Workspace` (187 of them, the player list,
  top bar, chat and shared flags among them), which 908 of those files
  require. `StarterScript` needs one on its third line, so the plain
  copies can't run.
- `ExtraContent/models/DataModelPatch/DataModelPatch.rbxm` holds all of
  them: 42,990 ModuleScripts under `PatchRoot`, as `CoreScripts` (the
  scripts `ScriptContext:AddCoreScriptLocal` starts, named
  `CoreScripts/<Name>`, and `StarterScript`), `DataModelInstances`
  (`CoreGui.RobloxGui.Modules` and `CorePackages`, laid out as the game
  sees them) and `PlayerScripts` (the PlayerModule and
  `RbxCharacterSounds`). Their `Source` is Luau bytecode, version 9 with
  types version 3, saved as property type 0x1D, which `rbx_binary`
  skips. The opcodes aren't encoded: Rook's Luau (0.740) loads all 42,990
  as they are, and they run.
- The player install's `InExperience.rbxm` is the same kind of file, so a
  player install would do too; Rook looks for Studio's.
- `%LOCALAPPDATA%\Roblox\ClientSettings\StudioAppSettings.json` holds the
  fast flags Studio last downloaded (`FFlagX`, `DFIntY`, ... as strings),
  so the scripts take the paths they take in Studio. Engine-side flags
  aren't in it; Rook gives the few the scripts read as numbers a value
  (`InGameGlobalGuiInset` 58, Inference: the top bar's height).

## How a client runs them

- The patch's tree is built outside the DataModel; `RobloxGui` (a
  ScreenGui, a Folder in the patch) goes into CoreGui with its `Modules`,
  and `CorePackages`' children into the CorePackages service.
- `StarterScript` runs as a CoreScript under CoreGui before the client's
  LocalScripts start, and starts the rest with `AddCoreScriptLocal`.
- Code that came compiled runs as Roblox's own: it reaches Plugin,
  LocalUser and RobloxScript members, which game scripts are still denied
  (`The current thread cannot call ... (lacking capability RobloxScript)`).
  Which code is Roblox's is decided from the calling function's
  environment, and only when a member needs more than the game identity.
- Members Rook doesn't have do nothing for Roblox's code and read as nil,
  counted under "Not implemented in Rook yet" (game scripts still get an
  error): most of them talk to Roblox's website (analytics, experiments,
  voice, purchases, friends), and one missing member mustn't stop the
  whole GUI. Requests through `HttpService:RequestInternal` and
  HttpRbxApiService fail at once, as if offline, unless their web domain
  has an endpoint ([web endpoints](../../design/web-endpoints.md)).
- `GetEngineFeature` says no except for the features whose API Rook has
  and the scripts need to take their current path (`AvatarChatServiceEnabled`,
  without which the menu's voice settings find no service, and
  `TextChatServiceSettingsSaved`, without which the chat window shows its
  translation tip every time). Saying yes to every feature stops the top
  bar.
- Roblox still answers a few spellings its API dump no longer lists, and
  the CoreScripts use them: `RunService.renderStepped`, `Instance.name`,
  `RbxAnalyticsService:setRBXEventStream` (without it the settings menu
  stops halfway, its bottom buttons unsized) and `UDim2:lerp`.

## What runs

On an empty baseplate, 29 CoreScripts start and run without errors.
The top bar (Roblox's logo button, and the menu and chat buttons in their
pill), the player list and Roblox's notification toasts draw (Roblox's
own React code, laid out by Rook's GUI). Clicking the logo button opens
the in-game menu (its People, Settings, Report and Help tabs, and Leave,
Respawn and Resume with their keys), dimming the screen and hiding the
player list. `RobloxGui` stacks its objects by sibling
(`ZIndexBehavior` Sibling; Observed: the menu's newer buttons, at ZIndex
1, show over its older panel). The menu slides in with
`GuiObject:TweenPositionInternal`, the CoreScripts' own copy of
`TweenPosition`. The settings menu, backpack,
emotes menu, purchase prompts, screenshots and the fullscreen title bar
load. Clicking the menu's tabs shows the People, Report and Help pages
laid out as in Roblox. The chat window shows messages sent and received
through [TextChatService](chat.md), with each speaker's name. Chat's
stores need `Player.ChatAvailabilityStatus` (a string starting `Enabled`
when the player may chat), which Rook's API snapshot doesn't have; Rook
gives `Enabled`.

### Health bar and backpack

Run with a spawning character, two Tools in StarterPack and a server
script calling `TakeDamage` every half second:

- The health bar is the top bar's (`TopBarApp`), 125 by 20 pixels at
  the window's top right, 16 from its edge. It shows
  only while `Health` is below `MaxHealth` (Roblox's `HealthBar`), so it
  appears with the first damage and shortens, green to yellow to red, as
  `HealthChanged` arrives; the top bar's `HurtOverlay`, a red edge over
  the whole window, shows on hits (a still can't tell whether it fades as
  Roblox's does).
- The backpack is `RobloxGui.Backpack` (`Modules/BackpackScript`, plain
  Luau in the install too): the hotbar is centered at the bottom, a row
  of ten 60-pixel slots 5 apart in a 655-pixel frame 70 tall, showing
  only slots with a tool, each with the tool's name (or `TextureId`
  image) and its key's number at 15% of the slot. Clicking a slot, or
  pressing its number key, equips the tool, outlining the slot in blue
  (90, 142, 233); again, or another slot, unequips it. Backquote opens
  the inventory above the hotbar (655 by 320, with a search box) and
  shows all ten slots.
- The script equips by setting `tool.Parent = Character` on the client
  (its own comment says `Humanoid:EquipTool` was broken), and unequips
  with `Humanoid:UnequipTools()`. A client's moves of its own player's
  Tools between their Backpack and character reach the server, so a
  server Script in the tool hears `Equipped` (Inference: what classic
  gear relies on; Rook repeats only those moves, nothing else a client
  changes). Legacy server Scripts run in a player's Backpack, so gear
  connects `Equipped` before its first equip (Inference, as above).
- `Tool.Equipped` gives the player's Mouse on the client and nil on the
  server (Inference; the server's argument isn't measured), and fires
  after the move, as does `Unequipped` (order not measured).
- The tools are held nowhere: characters have no arms or grip yet
  ([M9](../../roadmap/m9-characters.md) steps 1 and 7), so an equipped
  tool sits in the character without moving.

The positions above are from `--dump` and Rook's screenshot; they follow
the script's constants, but haven't been compared with a Roblox
screenshot.

## StyleSheets

The new GUI (Foundation, the top bar) sizes, places and colors its
objects only through StyleSheets: a component sets tags (`size-full-full`,
`bg-surface-100`) and the sheet's StyleRules give the properties. Rook
applies them each frame before GUI lays out:

- A StyleLink attaches its `StyleSheet` to its parent's tree; StyleDerive
  children of a sheet add another sheet's rules, at lower precedence.
  Only one sheet applies to a tree (Confirmed: creator-docs "UI styling");
  where links nest, the nearest one styles what's under it (Inference:
  CoreGui has a link of its own, and the chat's must still apply).
- Selectors: a class name, `#Name`, `.tag`s and `:Hover`, `:Press` or
  `:NonInteractable` (the object's `GuiState`), chained by `>` (child) and
  `>>` (descendant), comma-separated; `::Class` makes a pseudo-instance of
  that class (a UICorner, a UIPadding) under what matches and styles it.
  `@` queries and nested rules aren't supported.
- Values starting with `$` are tokens: the sheet's attribute of that
  name, then its derived sheets'.
- Higher `Priority` wins, then the later rule. A rule sets a property only
  where the object hasn't set it itself (Inference, from Foundation
  setting some properties directly and leaving the rest to styles); a
  property no rule sets any more goes back to its default.

## What Rook lacks

The members the CoreScripts call that Rook doesn't have, from the run
above, by calls:

| Calls | Members |
|---|---|
| Analytics and telemetry | `RbxAnalyticsService` (`SetRBXEventStream` 43, `GetPlaySessionId`, `GetSessionId`, `ReportCounter`), `EventIngestService:SendEventDeferred` |
| Roblox's app and website | `MessageBusService` (`Call`, `Publish`, `Subscribe`), `LinkingService:RegisterLuaUrl`, `SessionService:SessionExists`, `ThirdPartyUserService:IsSingleSignOnSupported`, `AchievementService:IsAvailable`, `CaptureService:RetrieveCaptures`, `InsertService:LoadLocalAsset` |
| Stored settings | `MemStorageService` (`GetItem` 21, `BindAndFire`), `LocalStorageService` (`GetItem`, `WhenLoaded`) |
| Experiments | `IXPService` (`InitializeUserLayers`, `RegisterUserLayers`, `GetUserLayerLoadingStatus`) |
| Gamepad and keyboard navigation | `GuiService` (`AddKey` 22, `AddSpecialKey`, `AddSelectionParent`, `SelectedCoreObject`), `ContextActionService` (`BindCoreAction` 14, `UnbindCoreAction`, `BindAction`) |
| The top bar's space | `GuiService:SetGlobalGuiInset`, `GuiService:SetTopbarInset`, `GuiService.CoreGuiFolder`, `CoreEffectFolder`, `SendCoreUiNotification` |
| Chat and voice | `TextChatService.ChatTranslationToggleEnabled`, `VoiceChatService:IsVoiceEnabledForUserIdAsync`, `AudioFocusService:GetFocusedContextId`, `FaceAnimatorService` |
| Other | `GuiService:GetNotificationTypeList`, `LocalizationService:GetCountryRegionForPlayerAsync`, `ScreenGui.OnTopOfCoreBlur`, `StarterPlayer.PlayerModuleStatus` |

Behavior the health bar and backpack still miss, from characters
([M9](../../roadmap/m9-characters.md)): `Humanoid.Died` and death (the
bar's dead state, the backpack clearing on respawn), regeneration,
`Player:RequestTool` (behind a flag and engine feature Studio has off),
dropping tools with Backspace, holding a tool by its grip, and gamepad
and touch selection (`GuiService.SelectedCoreObject` is read on every
input and answers nil).

Game scripts need some of these too, and lacked them before this step;
these now work for both: `StarterGui:SetCore`, `GetCore` and
`SetCoreGuiEnabled`, `RunService:BindToRenderStep`, `TextService:GetTextSize`,
CollectionService's tag methods, `Teams:GetTeams`,
`PolicyService:GetPolicyInfoForPlayerAsync`, `Workspace:GetServerTimeNow`,
LocalizationTables and Translators, `DateTime`, `settings()` and
`UserSettings()`, and PlayerScripts' movement modes.
