# M8 GUI

TextLabel 2,936 files, Frame 2,857, ScreenGui 2,839, ImageLabel 2,778,
TextButton 2,521, ImageButton 1,964, SurfaceGui 1,854, TextBox 1,758,
BillboardGui 1,665, ScrollingFrame 1,347. Scripts create Messages in
1,958 files and Hints in 1,225. It comes before characters: tools,
health bars and leaderboards are GUI.

1. ✅ [Layout](../behavior/gui/layout.md): `UDim2`, `AnchorPoint`, `ZIndex`,
   `ClipsDescendants`, `AutomaticSize`, UIListLayout, UIGridLayout,
   UIPadding, UIScale and the size constraints; backgrounds and borders
   drawn.
2. ✅ [Text](../behavior/gui/text.md): fonts and `FontFace` weights,
   sizing, wrapping, `TextScaled`, `RichText`; Hint 145 and Message 26,
   bars of text.
3. ✅ [SurfaceGui and BillboardGui](../behavior/gui/world.md) in the
   world: signs, labels and name tags.
4. ✅ [Images, 9-slice](../behavior/gui/images.md),
   [ScrollingFrame](../behavior/gui/scrolling.md).
5. ✅ [Input](../behavior/gui/input.md): GUI takes the mouse and keyboard
   before the world; buttons, the wheel and scroll bars, TextBoxes, the
   cursor.
6. ✅ [The legacy styles](../behavior/gui/styles.md): GuiButton's and
   Frame's `Style`, drawn from the install's textures with their hover,
   pressed and selected looks, and the insets they give children and
   text.
7. ✅ [The modern UI objects](../behavior/gui/modern.md), by files:
   [UICorner](../behavior/gui/modern.md#uicorner) 864,
   [UIGradient](../behavior/gui/modern.md#uigradient) 782,
   [UIListLayout's flex and `Wraps`](../behavior/gui/modern.md#flex)
   (UIListLayout is in 722 files, UIFlexItem in 6),
   [UIStroke](../behavior/gui/modern.md#uistroke) 212,
   [UIPageLayout](../behavior/gui/modern.md#uipagelayout) 69,
   [CanvasGroup](../behavior/gui/modern.md#canvasgroup) 34 and
   [UITableLayout](../behavior/gui/modern.md#uitablelayout) 7. Corners,
   strokes and gradients are parameters of the GUI quad drawn in its
   shader; a CanvasGroup is drawn to a texture of its own.
8. **ViewportFrame** (251 files) and WorldModel (46).
   - *Scope*: its own Camera (`CurrentCamera`), `Ambient`, `LightColor`,
     `LightDirection`, `ImageColor3` and `ImageTransparency`; WorldModel
     for queries and animation inside it.
   - *Studio questions*: how it lights its scene (just ambient and one
     directional light? shadows? materials and SurfaceAppearance?) and how
     it scales with screen size. Captures leave ViewportFrames out, on
     SurfaceGuis too, so the looks come from screenshots the user takes,
     all views asked for at once.
   - *How*: the renderer draws more than one scene: the frame's subtree
     with its camera and light into a target of the frame's size, laid
     over the frame like an image (as CanvasGroups' textures are).
   - *Done when*: a ViewportFrame's parts draw as in Studio's screenshots
     within step 4's tolerance, and raycasts against a WorldModel's parts
     return what Studio's do.
9. **The GUI Roblox draws itself**, from the CoreScripts in the user's
   Studio install, loaded at runtime and never shipped (see the
   [content policy](../design/policies.md#content-and-assets)). They
   draw the top bar, player list, backpack, health bar, ProximityPrompts
   (157 files), the legacy Dialog (352 files, through
   `MainBotChatScript2`), `Kick` and error prompts, and they answer
   `SetCoreGuiEnabled` (1,295 files), `SetCore` (868) and `GetCore`
   (472).
   - *Scope*:
     - A loader that reads Studio's `DataModelPatch.rbxm`, where the
       CoreScripts and CorePackages are all kept as Luau bytecode (the
       plain `.lua` copies leave out `CorePackages.Workspace`), and runs
       `StarterScript` with CoreScript identity into CoreGui's
       `RobloxGui`. Running: see
       [Roblox's CoreScripts](../behavior/gui/core-scripts.md).
     - StyleSheets, which the new GUI (Foundation, the top bar) is sized
       and colored by: started, without `@` queries.
     - The engine side the scripts expect: CoreGui and its security,
       GuiService's core members, the top bar inset, and stand-ins for
       services that talk to Roblox's website (friends, purchases,
       analytics, voice), which do nothing locally.
     - ProximityPromptService's engine half: which prompts are shown
       (distance, `RequiresLineOfSight`, `Exclusivity`), the key and
       `HoldDuration`, `Triggered` and `TriggerEnded`, and `Style` Custom
       for places that draw their own.
     - Without a Studio install: the 2018 Core-Scripts (Apache-2.0)
       shipped as a fallback, or no core GUI, chosen by setting.
   - *Evidence*: what the CoreScripts call that Rook lacks is listed on
     [the behavior page](../behavior/gui/core-scripts.md#what-rook-lacks),
     from running them; nothing had to be turned off. Left: the top bar,
     which waits on chat's stores (step 10).
   - *Studio questions*: what `SetCoreGuiEnabled` hides for each
     `CoreGuiType`, and which `SetCore` keys the archive uses
     (`SendNotification`, `ResetButtonCallback`, `TopbarEnabled`, ...),
     by a scan of the archive's calls.
   - *Done when*: in `rook-player` an archive place shows the player list
     with its leaderstats, the backpack with StarterPack's tools,
     working ProximityPrompts and Dialogs, and hides what its scripts
     turn off.
10. **Chat**: TextChatService and what the archive's scripts expect of
   the legacy Chat service (`Chat:Chat` bubbles in 561 files, `Chatted`
   handlers in about 830).
   - *Scope*: the chat window and input bar from the CoreScripts
     (`ExperienceChatMain`), TextChannels, `SendAsync`,
     `DisplaySystemMessage`, bubble chat over characters and parts, the
     configuration objects modern places save (ChatWindowConfiguration,
     BubbleChatConfiguration, ChatInputBarConfiguration in about 1,200
     files each), `Player.Chatted`, and chat commands (`/e`, `/w`).
     Messages are passed through unfiltered on a local server.
   - *Studio questions*: what a place set to `ChatVersion`
     LegacyChatService gets today, since Roblox announced legacy chat's
     retirement in 2025. If it gets TextChatService, Rook does the same
     and keeps only the legacy calls scripts make (`Chat:Chat`,
     `Chatted`, `Chat:FilterStringAsync`).
   - *Done when*: a player chats in an archive place, with bubbles over
     their character, and the place's `Chatted` commands run. Chat
     between machines comes with M10's remotes.
