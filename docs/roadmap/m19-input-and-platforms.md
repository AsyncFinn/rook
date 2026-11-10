# M19 Input and platforms

Roblox's own PlayerModule already runs in Rook (M9 step 4), and it has
gamepad and touch controls of its own: thumbsticks, the touch jump
button, gamepad movement and camera. Most of the engine's side is
feeding it the input and the `UserInputService` state it reads. The
members CoreScripts needed to boot answer fixed values today
(`GetGamepadConnected`, `GetImageForKeyCode`, `GetStringForKeyCode`,
`GetPlatform`, `GuiService:IsTenFootInterface` in
`crates/rook-lua/src/api/methods/core.rs`); steps 1 and 2 move each one
they cover next to the other input members
(`crates/rook-lua/src/api/methods/input.rs`) and have it read the
devices Rook sees.

Gamepads and touch share only the InputObject and `LastInputType`
plumbing, which whichever lands first builds.

1. Gamepads: devices connecting and disconnecting, buttons, triggers and
   thumbsticks as InputObjects, `GetConnectedGamepads`,
   `GetGamepadState`, `GamepadEnabled`, button images and names, and GUI
   selection navigation (`GuiService.SelectedObject`, the selection
   image, `GuiNavigationEnabled`).
2. Touch: `TouchEnabled`, `TouchStarted`, `TouchMoved` and `TouchEnded`,
   the gestures (`TouchTap`, `TouchLongPress`, `TouchPan`, `TouchPinch`,
   `TouchRotate`, `TouchSwipe`), GUI taking touches, and the
   PlayerModule's touch controls showing.
3. ContextActionService and UserInputService in full: the members left
   after steps 1 and 2, and ContextActionService's touch buttons.
4. Rebindable controls and the keyboard and mouse settings Roblox has.
5. Linux and macOS builds, checked in CI.
