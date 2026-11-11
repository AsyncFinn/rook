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
   - *Design*: as engines read pads: the platform's own API for most
     pads, and HID drivers of their own for the pads the platform reads
     badly (SDL's HIDAPI drivers for PlayStation and Switch pads, under
     Godot and most engines). Rook reads them with gilrs, built with its
     default filters off: Studio passes raw values with no deadzone or
     jitter filter, and the PlayerModule applies its own deadzone. On
     Windows gilrs only has Windows.Gaming.Input, which doesn't list a
     DualSense at all (measured: none in its controller list with a
     DualSense on USB), so Rook reads Sony pads there over HID itself.
     Pads, like winit, belong to `rook-client`'s `window` module
     (`deny.toml`).
     - *Host*: `window/gamepads.rs` gives each pad the lowest free
       `GamepadN` slot until it disconnects, drains its sources at the
       frame's start after the window's events, and turns each pad's
       new state into [`InputEvent`]s: a pad connecting or
       disconnecting, a button going down or up, one axis changing (a
       thumbstick's X or Y, or a trigger). A pad's input is dropped
       while the window is unfocused, as Studio does, except a key
       coming back to rest. Sources, each in `window/gamepads/`:
       - `system.rs`: gilrs, for every pad it lists; on Windows it skips
         Sony's vendor id, which `sony.rs` owns.
       - `sony.rs` (Windows): DualShock 4 and DualSense over HID through
         `windows-sys`, as the published report layouts give them (USB,
         and Bluetooth after reading the calibration feature report,
         which turns on full reports). A device-interface notification
         (`CM_Register_Notification` on the HID class) marks the list
         stale, and the next read enumerates again, so hotplug costs
         nothing between changes. Each pad has a thread blocking in
         `ReadFile` that parses reports into a state and sends it; the
         frame diffs states in order. Sticks map a byte as Studio does,
         `(b - 127.5) / 127.5` with up positive, and triggers as
         `b / 255`, so values match Studio's exactly.
       - Every pad lists the keys Studio lists for a DualShock and an
         Xbox pad, in its order (measured, same for both), less what
         gilrs doesn't map on a pad it reads.
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
   - *Done when*: in `rook-player` with each pad (a DualSense on USB and
     Bluetooth), the character walks
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
