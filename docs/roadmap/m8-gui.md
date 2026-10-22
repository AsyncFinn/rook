# M8 GUI

ScreenGui 600 files, SurfaceGui 140, BillboardGui 229; Frame 558,
TextLabel 550, ImageLabel 458, TextButton 456; Hint 145 and Message 26.
It comes before characters: tools, health bars and leaderboards are GUI.

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
5. Input: buttons, `TextBox`, GUI taking input before the 3D world;
   ProximityPrompt (12 files).
6. The modern UI objects, fewest files last: UIListLayout's flex and
   wrapping with UIFlexItem (43 files use UIListLayout), UICorner 35,
   UIGradient 31, UIStroke 22, CanvasGroup 9, ViewportFrame 5.
7. The GUI Roblox draws itself (player list, backpack), from Core-Scripts
   (Apache-2.0) where they fit, and chat: TextChatService (62 files) and
   the legacy Chat.
