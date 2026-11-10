# Gamepads

Checked against Studio 0.747 on 2026-11-10.

[M19](../../roadmap/m19-input-and-platforms.md) step 1. Measured in
Studio 0.747 on 2026-11-10 in a playtest client on Baseplate, with a
DualShock and an Xbox pad on Windows, every UserInputService and
ContextActionService event logged by a client script. Values without a
pad connected came from the edit DataModel.

## Pads and their numbers

- A pad connecting takes the lowest free `Gamepad1` to `Gamepad8`. Two
  pads were Gamepad1 and Gamepad2; with Gamepad1 unplugged, the other
  stayed Gamepad2.
- Connecting fires, in order: `GamepadEnabled` turning true (when it was
  false), `PreferredInput` turning Gamepad, then `GamepadConnected(pad)`.
  No `LastInputTypeChanged`.
- Disconnecting first ends each thumbstick that was off center: an
  `InputEnded` with Position 0, after `LastInputTypeChanged` to the pad.
  Then `GamepadEnabled` turns false and `PreferredInput` turns
  KeyboardAndMouse, if it was the last pad, and `GamepadDisconnected(pad)`
  fires.
- `GetConnectedGamepads` lists the connected pads. `GetNavigationGamepads`
  lists all eight with none connected, and `IsNavigationGamepad` is true
  for each.
- While the window is unfocused, a pad sends nothing: presses made then
  fired no event.

## The InputObjects

- Each of the eight pads has 24 InputObjects, one per KeyCode, made
  whether or not the pad is connected, unparented, with
  `UserInputState` End and Position and Delta zero. `GetGamepadState`
  returns them in this order: ButtonStart, ButtonA, ButtonSelect,
  ButtonB, ButtonX, ButtonY, ButtonL1, ButtonR1, ButtonL2, ButtonR2,
  ButtonL3, ButtonUp, ButtonR3, ButtonLeft, DPadUp, DPadDown, DPadLeft,
  DPadRight, Thumbstick1, Thumbstick2, ButtonCenter, ButtonBack,
  ButtonDown, ButtonRight. Each call makes a new table of the same
  objects. `GetGamepadState` of a non-pad input type returns an empty
  table.
- Every event a pad fires passes that pad's object for the KeyCode, so
  `InputBegan`, `InputChanged` and `InputEnded` all see the same object,
  updated in place.

## Buttons

- A button (face buttons, shoulders, D-pad, sticks' clicks, Start and
  Select) fires `InputBegan` with Position (0, 0, 1) and Delta (0, 0, 1),
  then `InputEnded` with Position (0, 0, 0) and Delta (0, 0, -1).
- The Xbox and PlayStation buttons in the middle fire nothing.
- `GetSupportedGamepadKeyCodes` for the DualShock gave ButtonA, ButtonB,
  ButtonSelect, the four DPad codes, ButtonL1, ButtonL3, ButtonL2,
  Thumbstick1 twice, ButtonR1, ButtonR3, ButtonR2, Thumbstick2 twice,
  ButtonStart, ButtonX, ButtonY: a stick once per axis. With no pad it's
  empty and `GamepadSupports` is false.

## Thumbsticks

- A stick only fires `InputChanged`, one event per axis that changed:
  Position holds both axes, Delta only the one that changed, both with Z
  zero. Right and up are positive; each axis is clamped to -1..1. There
  is no deadzone: a pad at rest sends its jitter (the DualShock's
  alternated between 0.0196 and 0.0274 several times a second).
- Values are the device's own steps: the DualShock's in 1/127.5, the
  Xbox pad's much finer.

## Triggers

- ButtonL2 and ButtonR2 fire `InputChanged` with the pull in Position.Z
  (Delta.Z the change) on every change, on both pads.
- `InputBegan` fires when the pull reaches 1, after that change's
  `InputChanged`, with Delta zero; `InputEnded` fires each time it
  returns to 0, after its `InputChanged`, even when the pull never
  reached 1 and no `InputBegan` came.

## Which input is in use

- `GetLastInputType` and `LastInputTypeChanged` follow every input: the
  mouse's movement, buttons and wheel, the keyboard, a pad (`GamepadN`),
  and `Focus` when the window gains or loses focus. The change fires
  before the input's own event, and only when the type differs.
- A pad's button or a trigger's change counts. A thumbstick's change
  counts once the stick is at least 0.2 from center: pushes at 0.1765
  and 0.1843 didn't change it, 0.2000 and 0.2079 did. Jitter never does.
- `PreferredInput` turns Gamepad when a pad's input counts as above and
  KeyboardAndMouse when a key or mouse button does. Mouse movement and
  the wheel change `LastInputType` but not `PreferredInput`. Its
  changed signal fires after `LastInputTypeChanged`.
- With no pad ever connected it's KeyboardAndMouse.

## Names and images

- `GetStringForKeyCode` gives a pad KeyCode's own name (`ButtonA`,
  `DPadUp`, `Thumbstick1`), except ButtonCenter, which gives "". For keys
  it gives what the key types on a US keyboard (`Q`, `;`), and "" for
  keys that type nothing (LeftShift).
- `GetImageForKeyCode` gives
  `rbxasset://textures/ui/Controls/DefaultController/<KeyCode>@2x.png`
  for each pad KeyCode, ButtonCenter included, and "" for keys.
- Both follow the pad once one has connected. A PlayStation pad names
  ButtonA, B, X and Y ButtonCross, ButtonCircle, ButtonSquare and
  ButtonTriangle, and Start and Select ButtonOptions and ButtonShare; an
  Xbox pad names L1, R1, L2, R2, L3 and R3 ButtonLB, ButtonRB, ButtonLT,
  ButtonRT, ButtonLS and ButtonRS. Other pad keys keep their KeyCode's
  name, and ButtonCenter, ButtonBack and the ButtonUp, Down, Left and
  Right keys give "" on both.
- Images are in the pad's folder (`PlayStationController`,
  `XboxController`) named as the key's string, or as its KeyCode where
  the string is "". A PlayStation's Options and Share are under `PS5/`
  or `PS4/` in its folder.
- With two pads, the one connected last names the keys: an Xbox pad as
  Gamepad1 and a DualSense connected after it gave ButtonCross for
  ButtonA, before and half a second after pressing the Xbox pad's A.

## How Rook does it

Code: pads are read in `rook-player`'s `app/gamepads.rs` (gilrs, default
filters off) and reach the engine as `InputEvent`s; the engine side is
`rook-engine/src/bridges/input/gamepad.rs`, the input type in
`bridges/input/input_type.rs`, and the queries in
`rook-lua/src/api/methods/input.rs`.

- Rook asks the platform's gamepad API for the pad's values through
  gilrs, so the steps match the device's, not Studio's scaling of them
  (Approximation: Studio's DualShock values are in 1/127.5 around 127.5;
  gilrs maps the same byte in 1/128 around 127.5).
- `GetSupportedGamepadKeyCodes` lists what gilrs maps on the pad, each
  stick once per axis as Studio does, in `GetGamepadState`'s order
  (Approximation: Studio's order for the DualShock is its own).
