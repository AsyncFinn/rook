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

1. **Gamepads**: a player walks, jumps and turns the camera with an
   Xbox or PlayStation pad through Roblox's PlayerModule, and scripts
   see the pad as Studio shows it.
   - *Scope*: pads connecting and disconnecting as `Gamepad1` to
     `Gamepad8`; buttons, D-pad, triggers and thumbsticks as InputObjects
     through `InputBegan`, `InputChanged` and `InputEnded`;
     `GamepadEnabled`, `GamepadConnected`, `GamepadDisconnected`,
     `GetConnectedGamepads`, `GetGamepadConnected`, `GetGamepadState`,
     `GetSupportedGamepadKeyCodes`, `GamepadSupports`,
     `IsGamepadButtonDown`, the navigation gamepads,
     `GetStringForKeyCode` and `GetImageForKeyCode`; `PreferredInput`,
     `GetLastInputType` and `LastInputTypeChanged`, which the
     PlayerModule picks its controls by, for every input type, not only
     pads. ContextActionService as the PlayerModule's Gamepad controller
     needs it: hearing input before UserInputService, a sunk input
     reaching `InputBegan` as processed, thumbstick and trigger changes,
     `BindActivate` and `UnbindActivate`. Out: GUI selection with a
     pad (step 3); vibration (`HapticService`) and the remaining
     ContextActionService members (step 4); tools activating through
     `BindActivate`'s button, which M9's tools step builds on what this
     step records.
   - *Evidence*: measured in Studio 0.747 on 2026-11-10 with a
     DualShock and an Xbox pad, a client script logging every event
     ([gamepads](../behavior/input/gamepads.md),
     [ContextActionService](../behavior/input/context-actions.md)). The
     PlayerModule's ControlModule picks the Gamepad controller when
     `PreferredInput` is Gamepad; that controller and the camera's
     `CameraInput` bind `Thumbstick1`, `Thumbstick2`, `ButtonA`, `ButtonR2`
     (`BindActivate`), `ButtonL3` and `ButtonR3` through
     ContextActionService and read `GetGamepadState(Gamepad1)`.
   - *Design*: as engines read pads, from one library over each
     platform's API, polled once a frame on the main thread, in event
     order (SDL's gamepad API in Godot, gilrs in Bevy, Unity's Input
     System over XInput and HID). Rook reads them with gilrs, built with
     its default filters off: Studio passes raw values with no deadzone
     or jitter filter, and the PlayerModule applies its own deadzone.
     gilrs, like winit, belongs to `rook-player` alone (`deny.toml`).
     - *Host*: the app drains gilrs's events at the frame's start beside
       the window's (`app/inbox.rs`), in order, into new
       [`InputEvent`] variants: a pad connecting or disconnecting, a
       button going down or up, and one axis changing (a thumbstick's X
       or Y, or a trigger). The app gives each pad the lowest free
       `GamepadN` slot and keeps the slot until it disconnects, and
       drops a pad's input while the window is unfocused, as Studio
       does.
     - *Engine*: each slot keeps the 24 InputObjects Studio keeps per
       pad, made on the slot's first use and reused by every event and
       `GetGamepadState` call, so an event costs one property update and
       a fire, never an allocation or a scan. One choke point in
       `DataModel::input` notes each event's input type for
       `LastInputType` and `PreferredInput` (one owner for both), and
       offers each InputObject to ContextActionService before
       UserInputService fires, for keys and mouse buttons too. The
       queries read slot state kept beside `keys` and `buttons` in
       `rook-lua`'s `InputState`; nothing reads gilrs from Luau.
     - *Cost*: per event, as keys are now. A pad at rest still sends
       jitter events (Studio passes them on too), a few hundred a second
       at worst, each one update and one fire.
   - *Done when*: in `rook-player` with each pad, the character walks
     with the left stick, jumps with A (Cross), the camera turns with the
     right stick and zooms with R3 as in Studio; a conformance case
     recorded from the logs above passes, event for event, and the
     ContextActionService order case passes for keys too.
2. Touch: `TouchEnabled`, `TouchStarted`, `TouchMoved` and `TouchEnded`,
   the gestures (`TouchTap`, `TouchLongPress`, `TouchPan`, `TouchPinch`,
   `TouchRotate`, `TouchSwipe`), GUI taking touches, and the
   PlayerModule's touch controls showing.
3. GUI selection: `GuiService.SelectedObject` moved with a pad's D-pad
   and stick or the keyboard, the selection image, `Selectable`,
   `NextSelection*`, `GuiNavigationEnabled`, `AutoSelectGuiEnabled`,
   and a pad's A pressing the selected button.
4. ContextActionService and UserInputService in full: the members left
   after steps 1 to 3, ContextActionService's touch buttons, and
   `HapticService`.
5. Rebindable controls and the keyboard and mouse settings Roblox has.
6. Linux and macOS builds, checked in CI.
