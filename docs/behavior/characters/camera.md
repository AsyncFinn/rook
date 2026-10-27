# The default camera and controls

Checked against Studio 0.744 on 2026-10-27.

[M9](../../roadmap/m9-characters.md) step 4: a player's camera and
controls are Roblox's own PlayerModule, run from Roblox's packages as the
[CoreScripts](../gui/core-scripts.md) are, and never shipped. These are
the engine members it uses and what Studio's engine does around it.
Measured in a Baseplate playtest on the client over Studio's MCP: wheel
and right-button input through `user_mouse_input`, keys through
`user_keyboard_input`, reads from `execute_luau`. The default R15 body has
`HipHeight` 2.0075; the place's StarterPlayer has `CameraMaxZoomDistance`
128.

Code: installing the scripts in `rook-engine`'s `core_scripts.rs`, the
camera after spawning in `session/host.rs`, what the window takes from
the camera in `bridges/camera.rs` and the input it gives in
`bridges/input.rs`; `Camera:Interpolate` and a Scriptable camera's Focus
in `rook-lua`'s `api/methods/camera_moves.rs`, `Player:Move` in
`characters/movement.rs`, `Enum.PlayerActions` bindings in
`luau/context_action.luau`; `rook-player`'s `app/camera.rs` and
`app/mouse.rs`.

## Where the scripts come from

- A client's PlayerScripts get `PlayerScriptsLoader` (a LocalScript),
  `PlayerModule` (a ModuleScript tree) and `RbxCharacterSounds` (a
  LocalScript), each unless StarterPlayerScripts has one of that name,
  and StarterPlayerScripts' children. Studio lists Roblox's before the
  place's own; Rook adds Roblox's after them.
- A place with its own `CameraScript` or `ControlScript` in
  StarterPlayerScripts gets no PlayerModule or loader, only
  `RbxCharacterSounds`; with just one of the two, Roblox adds its 2017
  script for the other (Observed: a `ControlScript` with `MasterControl`
  and `PathDisplay` modules).
- `DataModelPatch.rbxm` holds them under `PatchRoot.PlayerScripts`:
  `StarterPlayerScripts.PlayerModule` and
  `StarterPlayerScriptsCommon.RbxCharacterSounds`, both ModuleScripts,
  and no loader. The loader is one line requiring the PlayerModule;
  Rook writes its own.
- They run as game scripts, as on Roblox, where a place may replace them:
  a member Rook lacks stops them with an error, as it would a place's own
  copy.

## The camera after spawning

- After `CharacterAdded` on the client, the current camera's
  `CameraSubject` becomes the new Humanoid and `CameraType` goes back to
  Custom, also after a script made it Scriptable (Observed: about 10 ms
  after the event). The PlayerModule sets neither.
- A Player copies these from StarterPlayer once, as it joins, and later
  changes to StarterPlayer don't reach it (Observed):
  `CameraMaxZoomDistance`, `CameraMinZoomDistance`, `CameraMode`,
  `DevCameraOcclusionMode`, `DevComputerCameraMovementMode` (as
  `DevComputerCameraMode`), `DevComputerMovementMode`,
  `DevTouchCameraMovementMode` (as `DevTouchCameraMode`),
  `DevTouchMovementMode`, `EnableMouseLockOption` (as
  `DevEnableMouseLock`), `AutoJumpEnabled`, `HealthDisplayDistance` and
  `NameDisplayDistance`. Without them the zoom limits were 0 and the
  camera stopped on its first frame.
- `Humanoid.RootPart` and the legacy `Humanoid.Torso` are the model's
  HumanoidRootPart, else its Torso, else an R15 body's UpperTorso (Observed
  with each removed). The camera follows the root part.
- The camera starts 12.5 studs from its focus, 15° down, the focus 1.5
  studs above the root part's center.
- `Camera.NearPlaneZ` is -0.1 (Observed; it's read-only). It isn't where
  `ViewportPointToRay` starts its rays (0.5 studs out, see
  [the mouse](../gui/input.md)). The occlusion below reads it.

## Zoom, occlusion and first person

The PlayerModule does all of this; these are the numbers it gave in
Studio, which Rook's run matches.

- The wheel zooms through `UserInputService.PointerAction(wheel, pan,
  pinch, gameProcessedEvent)`, not InputChanged: from 12.5, a step out
  goes to 19.75 then 30.625, a step in to 7.667, easing over about half a
  second each.
- Zoomed in to `CameraMinZoomDistance` (0.5) it's first person:
  `UserInputService.MouseBehavior` is LockCenter and the body's parts and
  decals have `LocalTransparencyModifier` 1, which hides them on this
  client only.
- Holding the right button makes `MouseBehavior` LockCurrentPosition, and
  releasing it Default; mouse movement turns the camera through
  InputChanged's `Delta`. While the mouse is free, a move's `Delta` and
  `GetMouseDelta` are zero (Observed), so the camera only turns while the
  mouse is held.
- An opaque 20 × 20 × 1 wall 5.5 to 6.5 studs behind the character pulls
  the camera in to 5.775 from its focus: its near plane stops at the wall,
  so the eye is a little inside it. A wall with `CanCollide` off, or
  `Transparency` 0.3, doesn't (Observed both).
- Shift lock needs the player's own setting (`UserGameSettings.ControlMode`
  MouseLockSwitch, in Roblox's menu), which only Roblox's code may set
  (game code: "lacking capability RobloxScript"), so it wasn't measured.
  The PlayerModule then locks the mouse to the center
  (`MouseBehavior` LockCenter) with LeftShift.

- With `UserGameSettings.RotationType` CameraRelative, which the
  PlayerModule sets in first person and shift lock, the engine turns the
  character to face the camera's heading, standing still too: 90° within
  one frame, overshooting 4° and settling over two more (Observed with a
  Scriptable camera turned 90°). MovementRelative, the default, turns it
  toward where it walks.

## Controls

- The keyboard controls bind `Enum.PlayerActions` items, not keys. A
  keyboard gives CharacterForward for W and Up, CharacterBackward for S
  and Down, CharacterLeft for A, CharacterRight for D and CharacterJump
  for Space (Observed; Left and Right turn the camera instead).
- Every frame the controls call `Player:Move(direction, true)`, which
  moves the character's Humanoid relative to the camera, and set
  `Humanoid.Jump` while Space is held.

## Scripted cameras

- `Camera:Interpolate(endCFrame, endFocus, duration)` moves the camera's
  position straight to `endCFrame`'s and turns it to look at a focus
  moving straight from its Focus to `endFocus`'s, both at a steady pace,
  and ends on `endCFrame` itself, then fires `InterpolationFinished`
  (Observed: after 1.0002 s of a 1 s call).
- It errors on a camera that isn't Scriptable ("Attempted to use
  interpolation with a camera mode other than scriptable.") and for a
  duration of 0 or less ("Interpolation time must be positive.").
- A second call starts from where the camera is. A script setting the
  camera's CFrame ends it, and the camera stays where written.
- A Scriptable current camera's Focus is 20 studs in front of it each
  frame, even right after a script set Focus (Observed), and so during an
  interpolation too.

## In Rook

- What the PlayerModule calls that Rook lacked, from running it in a
  client (`rook-tools run-headless --join --core-scripts`): two members
  (`Player:Move`, `UserGameSettings:SetGamepadCameraSensitivityVisible`)
  and behavior behind members Rook had: the StarterPlayer copies,
  CameraSubject and CameraType after spawning, `RootPart`, `NearPlaneZ`,
  `Enum.PlayerActions` bindings, `PointerAction`, what `MouseBehavior`
  does to the window, and `LocalTransparencyModifier` in drawing.
  `RunService:BindToRenderStep` and ContextActionService were there.
- `rook-player` draws from `Workspace.CurrentCamera` while a script drives
  it: a Scriptable camera, or the player's character with a PlayerModule
  or `CameraScript` in PlayerScripts. Otherwise its own flying camera
  moves, as before, and gives the game its CFrame.
- The window holds the mouse as `MouseBehavior` says, unless a shown
  GuiButton is `Modal` or a TextBox has focus (Inference: typing frees it),
  and only while it's focused. A held mouse's movement reaches scripts as
  InputChanged with its `Delta` (summed over the frame, as the camera
  reads only the last), with the cursor at the window's center for
  LockCenter.
- `PointerAction` fires after the wheel's InputChanged (Inference), with
  no pan or pinch.
- A client's frame runs the render step (RenderStepped, the PlayerModule's
  camera) and lays out GUI after its physics, just before it's drawn, so
  Roblox's cycle (RenderStepped, draw, physics) holds and the camera aims
  at the pose that's drawn; run first, it aimed at the last frame's, and
  trailed a walking character.
- Facing the camera closes the turn within a 60 Hz frame, leaving out
  Studio's overshoot (Approximation).
- The client steps once per drawn frame, that frame's length, as Roblox
  runs a frame per drawn frame (Studio's playtest client ran at 60);
  physics simulates whole 240 Hz steps and carries what's left to the
  next frame, so walking and jumping keep Studio's pace at any frame rate.
- Interpolations and a Scriptable camera's Focus are updated after
  RenderStepped, before drawing (Inference).

## What's left

- Gamepad, touch (thumbsticks, the jump button, pinching) and VR input.
- Roblox's 2017 `CameraScript` and `ControlScript` for places with only
  one of them.
- Shift lock's look in Rook (the menu's switch sets it) isn't compared
  with Studio.
- `Camera.DiagonalFieldOfView` and `MaxAxisFieldOfView` read 0: Studio's
  values (120.4 and 96.6 at 70° in a 1270 × 793 viewport) weren't
  consistent from one FOV to the next, and the PlayerModule doesn't read
  them.
