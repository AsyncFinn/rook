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
5. **Input**: GUI takes the mouse and keyboard before the 3D world does.
   Today `gameProcessedEvent` is always false and clicks go straight to
   ClickDetectors (`rook-engine/src/bridges/input.rs`).
   - *Scope*:
     - Hit-testing: the object under the cursor, in the order step 1
       draws (ScreenGuis by `DisplayOrder`, then `ZIndex`), cut by
       `ClipsDescendants` and ScrollingFrames, turned by `Rotation`, and
       only if `Visible`, `Active` and `Interactable` allow it.
       SurfaceGuis and BillboardGuis with `Active` set take the mouse's
       world ray. `GetGuiObjectsAtPosition` (51 files).
     - Events: GuiButton's `MouseButton1Click`, `Down` and `Up`, the
       button 2 set, and `Activated`; every GuiObject's `MouseEnter`,
       `MouseLeave`, `MouseMoved`, the wheel events and `InputBegan`,
       `InputChanged` and `InputEnded`; `GuiState`; `AutoButtonColor`'s
       tints and ImageButton's `HoverImage` and `PressedImage`.
     - `gameProcessedEvent` on UserInputService's events, and GUI
       blocking ClickDetectors and the mouse behind it.
     - TextBox (scripts create one in 764 files): focus by clicking or
       `CaptureFocus` (817 files) and `ReleaseFocus`, `Focused`,
       `FocusLost` with `enterPressed`, typing through the platform's
       text input (IME included), the caret and selection
       (`CursorPosition`, `SelectionStart`), `ClearTextOnFocus`,
       `MultiLine`, `TextEditable`, `PlaceholderText`, copy and paste,
       `UserInputService:GetFocusedTextBox`. Keys typed into a box don't
       reach the game.
     - ScrollingFrames scrolled by the wheel and by dragging their bars.
     - The cursor: `Mouse.Icon` and `UserInputService.MouseIcon`
       (`GetMouse` is in 1,993 files, and tools set icons).
   - *Not here*: gamepad selection (`Selectable`, GuiService's selected
     object) and touch go to M19; `Modal` freeing the mouse in first
     person comes with the camera (M9).
   - *Studio questions*, in one session. Studio's `screen_capture` leaves
     GUI out, so the user looks at the tints live:
     - Overlaps: does a Frame with `Active` off, or one that is fully
       transparent, let a click through to a button under it? To a
       ClickDetector? Is `gameProcessedEvent` true for a click on a plain
       TextLabel?
     - Press on one button and release on another, or drag off and back:
       which of Down, Up, Click and Activated fire, in what order next to
       UserInputService's `InputBegan`?
     - Can the clipped-away part of a button be clicked? A rotated
       button's hit area; a disabled ScreenGui; `Visible` off on an
       ancestor; `Interactable` off.
     - AutoButtonColor's hover and press tints on light and dark colors,
       and whether reading the color back shows them.
     - TextBox: `enterPressed` for Enter, Escape, Tab and clicking away;
       Enter in a `MultiLine` box; `ClearTextOnFocus` with
       `CaptureFocus`; the order of `Text` Changed events while typing.
   - *Method*: client conformance cases with clicks and keys. The
     `--! client mouse` line gains presses and key strokes, played in
     Studio through the MCP's `user_mouse_input` and `user_keyboard_input`
     and in Rook as `InputEvent`s.
   - *How*: `rook-gui` hit-tests its own layout, since it already holds
     the draw order and clip rectangles, and returns the object and its
     LayerCollector. The input bridge asks GUI first and the world after.
     The window's text events become a new `InputEvent`.
   - *Done when*: the input cases pass, and in `rook-player` an archive
     shop menu, a TextBox form and a scrolling list work without clicks
     falling through to the world.
6. **The modern UI objects**, by files: UICorner 864, UIGradient 782,
   UIListLayout's flex and `Wraps` (UIListLayout is in 722 files,
   UIFlexItem in 6), ViewportFrame 251, UIStroke 212, UIPageLayout 69,
   CanvasGroup 34, UITableLayout 7.
   - *Scope*:
     - UICorner on backgrounds, images and borders, and how it meets
       `ClipsDescendants`.
     - UIGradient on backgrounds, images and text: `Color` and
       `Transparency` sequences, `Rotation`, `Offset`.
     - UIStroke: `ApplyStrokeMode` (a text outline or a border),
       `Thickness`, `LineJoinMode`, `Transparency`, with UICorner and
       under UIScale.
     - Flex: `HorizontalFlex`, `VerticalFlex`, `ItemLineAlignment`,
       `Wraps`, and UIFlexItem's `FlexMode`, grow and shrink ratios.
     - UIPageLayout: pages, `JumpTo`, `Next`, `Previous`, the tween
       between pages, `Circular`, the wheel, and its events.
     - UITableLayout's rows and columns.
     - ViewportFrame: its own Camera (`CurrentCamera`), `Ambient`,
       `LightColor`, `LightDirection`, `ImageColor3`, and WorldModel (46
       files) for queries and animation inside it.
     - CanvasGroup: the group drawn to a texture, then with
       `GroupTransparency` and `GroupColor3`, clipped by its UICorner.
   - *Studio questions*: how ViewportFrame lights its scene (just ambient
     and one directional light? shadows? materials and SurfaceAppearance?)
     and how it scales with screen size; whether UICorner's edge is
     anti-aliased; the color space gradients blend in; whether UIStroke
     grows with UIScale; how sharp a CanvasGroup's texture is.
   - *Method*: layout and property reads as conformance cases; looks
     compared to screenshots the user takes, all views asked for at once.
   - *How*: rounded corners, gradients and strokes become parameters of
     the GUI quad, drawn in its shader, not new geometry. ViewportFrame
     reuses the scene renderer on a small target with the frame's
     subtree, and CanvasGroup is a GUI layer drawn to a texture.
   - *Done when*: the cases pass and the reference views match Studio
     within step 4's tolerance.
7. **The GUI Roblox draws itself**, from the CoreScripts in the user's
   Studio install, loaded at runtime and never shipped (see the
   [content policy](../design/policies.md#content-and-assets)). They
   draw the top bar, player list, backpack, health bar, ProximityPrompts
   (157 files), the legacy Dialog (352 files, through
   `MainBotChatScript2`), `Kick` and error prompts, and they answer
   `SetCoreGuiEnabled` (1,295 files), `SetCore` (868) and `GetCore`
   (472).
   - *Scope*:
     - A loader that finds Studio's `ExtraContent/scripts/CoreScripts`
       and `LuaPackages` (about 2,400 and 4,700 files) and runs
       `StarterScript` with CoreScript identity into CoreGui's
       `RobloxGui`.
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
   - *Evidence first*: run Rook's script scan over the CoreScripts to
     list the services and members they call that Rook lacks, by the
     scripts StarterScript starts. That list sizes this step and says
     which scripts to turn off (voice, purchases, sharing).
   - *Studio questions*: what `SetCoreGuiEnabled` hides for each
     `CoreGuiType`, and which `SetCore` keys the archive uses
     (`SendNotification`, `ResetButtonCallback`, `TopbarEnabled`, ...),
     by a scan of the archive's calls.
   - *Done when*: in `rook-player` an archive place shows the player list
     with its leaderstats, the backpack with StarterPack's tools,
     working ProximityPrompts and Dialogs, and hides what its scripts
     turn off.
8. **Chat**: TextChatService and what the archive's scripts expect of
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
