# The default camera and controls

Checked against Studio 0.747 on 2026-11-10.

[M9](../../roadmap/m9-characters.md) step 4: a player's camera and
controls are Roblox's own PlayerModule, run from Roblox's packages as the
[CoreScripts](../gui/core-scripts.md) are, and never shipped. These are
the engine members it uses and what Studio's engine does around it.
Measured in a Baseplate playtest on the client over Studio's MCP: wheel
and right-button input through `user_mouse_input`, keys through
`user_keyboard_input`, reads from `execute_luau`. The playtest body (the
signed-in user's avatar, not the [default](rigs.md#r15)) has
`HipHeight` 2.0075; the place's StarterPlayer has `CameraMaxZoomDistance`
128.

Code: installing the scripts in `rook-engine`'s `core_scripts.rs`, the
camera after spawning in `session/host.rs`, what the window takes from
the camera in `bridges/camera.rs` and the input it gives in
`bridges/input.rs`; `Camera:Interpolate` and a Scriptable camera's Focus
in `rook-lua`'s `api/methods/camera/moves.rs`, `Player:Move` in
`characters/movement.rs`, `Enum.PlayerActions` bindings in
`luau/context_action.luau`; `rook-client`'s `window/camera.rs` and
`window/mouse.rs`.

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

## The camera before spawning

- Camera isn't replicated: each client makes its own current camera as it
  joins. Before a character spawns it's Fixed with no `CameraSubject`, its
  Focus at (0, 0, -5) and its CFrame 12.5 studs back from it, 45° down
  (at (0, 8.839, 3.839)), `FieldOfView` 70. Mouse and key input don't
  move it (Observed).
- `Instance.new("Camera")` starts elsewhere: Fixed at (0, 20, 20), Focus
  (0, 0, -5).

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
- `GetMouseDelta` is the mouse's movement in the last frame (creator-docs
  UserInputService:GetMouseDelta): every move in that frame added up, and
  zero once a frame passes without one, so a camera script that turns by it
  every frame stops when the mouse does.
- An opaque 20 × 20 × 1 wall 5.5 to 6.5 studs behind the character pulls
  the camera in to 5.775 from its focus: its near plane stops at the wall,
  so the eye is a little inside it. A wall with `CanCollide` off, or
  `Transparency` 0.3, doesn't (Observed both).
- Shift lock needs the player's own setting (`UserGameSettings.ControlMode`
  MouseLockSwitch, in Roblox's menu), which only Roblox's code may set
  (game code: "lacking capability RobloxScript"). With it on (set from
  the menu, Studio 0.747 on 2026-11-10), LeftShift or RightShift toggles
  shift lock through ContextActionService (`MouseLockSwitchAction`). On,
  in the frame of the press: `MouseBehavior` LockCenter, `MouseIcon`
  `rbxasset://textures/MouseLockedCursor.png` (set through the Mouse,
  [input](../gui/input.md#the-cursor)), `RotationType` CameraRelative,
  and the camera and its Focus 1.75 studs to the camera's right of where
  they were, with no easing (the head read 1.75 studs left of center at
  12.5 studs). Off undoes each in the frame of the press. Rook runs the
  same PlayerModule and reads the same values frame for frame, the
  setting loaded from the player's saved settings.
- A playtest whose shift lock was switched on from the menu while the
  game ran once kept LockCenter and CameraRelative with no camera offset,
  toggling as usual; a fresh playtest offset it. Not reproduced, so not
  modeled (Observed once).

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

## Camera methods

Observed in Studio 0.747 on 2026-11-14, on the playtest server and
client; the server's half is the conformance case `physics/camera`. Code
in `rook-lua`'s `api/methods/camera/rays.rs` (roll, near plane),
`camera_moves.rs` (orbit steps, ZoomToExtents) and
`camera_occlusion.rs`.

- `NearPlaneZ` is -0.1 for a client's current camera and -0.5 for every
  other camera, a server's current one included.
- `SetRoll(angle)` keeps the angle as given (7 stays 7) on a Scriptable
  camera, until its CameraType changes; any other camera ignores it.
  `GetRoll` reads that roll, never the CFrame's own. `GetRenderCFrame`
  is `CFrame * CFrame.Angles(0, 0, -roll)`, and the window draws a
  Scriptable current camera with it.
- `PanUnits(n)` and `TiltUnits(n)` orbit the Focus at the camera's
  distance from it (a camera on its Focus goes a stud out), heading and
  pitch taken from the look vector, then aim it with
  `CFrame.lookAt(position, Focus)`, which drops the roll. Pan rounds the
  heading to a multiple of 45° and turns 45° clockwise (from above) a
  unit, keeping the look's height, so a camera `lookAt` aimed straight
  down stays exactly as it was. Tilt rounds
  the pitch to a multiple of 10°, raises it 10° a unit, keeps it within
  81° of level, and returns false only when the pitch asked for is the
  one it had. `n` is rounded half away from zero (1.5 is 2).
- `ZoomToExtents(cframe, size)` moves Focus to the box's center
  (unturned) and backs the camera, turned as it was, off it until the
  sphere around the box's world-axis box (radius at least 0.5) fills the
  narrower half of the field of view: vertical, or horizontal in a
  viewport taller than wide.
- `GetPanSpeed` and `GetTiltSpeed` read 0.
- `GetPartsObscuringTarget(points, ignore)` casts from the camera to each
  point in turn, listing each ray's parts nearest first and each part
  once. A part the ray starts in or only reaches at its end is left out,
  one holding the point is in; CanCollide and Transparency don't matter,
  CanQuery does as for any ray, collision groups act as Default, Terrain
  is never listed, there's no length limit, and entries that aren't
  Vector3s are skipped.
- `GetLargestCutoffDistance(ignore)` is how far in the camera must move
  to see past what's between it and its Focus. Five rays run parallel to
  the Focus-to-camera line, through the near plane's center and corners
  (by the field of view and the viewport, along the camera's axes): each
  from that offset off the Focus to that point on the near plane. A ray
  that hits something (from the Focus side, skipping a part it starts
  in) cuts off the stretch from there to the near plane; the longest
  wins, 0 if none hit. It passes through parts with CanCollide off or
  more than 0.95 transparent and groups that don't collide with Default,
  and stops at Terrain, though not at its water.
- Both find nothing for a camera outside Workspace.
- Studio's two occlusion queries don't see a part created since the
  last physics step (its other queries do); Rook's see it at once.

## In Rook

- What the PlayerModule calls that Rook lacked, from running it in a
  client (`cargo xtask run-headless --join --core-scripts`): two members
  (`Player:Move`, `UserGameSettings:SetGamepadCameraSensitivityVisible`)
  and behavior behind members Rook had: the StarterPlayer copies,
  CameraSubject and CameraType after spawning, `RootPart`, `NearPlaneZ`,
  `Enum.PlayerActions` bindings, `PointerAction`, what `MouseBehavior`
  does to the window, and `LocalTransparencyModifier` in drawing.
  `RunService:BindToRenderStep` and ContextActionService were there.
- `rook-player` draws from `Workspace.CurrentCamera` while a script drives
  it: a Scriptable camera, or a player who joined with a PlayerModule or
  `CameraScript` in PlayerScripts, character or not, so a client waiting
  to spawn shows its Fixed camera as Studio does, made where Studio's
  client makes it (it was a new Camera's (0, 20, 20)). What the player
  joined with counts, not what's left: a place that turns Roblox's camera
  off with a `CameraScript` that destroys itself and moves the camera with
  its own code is drawn from that code's camera, as in Studio's Play mode.
  Otherwise (Run mode, or a player Rook couldn't give camera scripts) its
  own flying camera moves, as before, and gives the game its CFrame. Tests:
  `a_client_draws_its_camera_before_its_character_spawns`,
  `a_place_with_its_own_camera_code_is_drawn_from_it`.
- The window holds the mouse as `MouseBehavior` says, unless a shown
  GuiButton is `Modal` or a TextBox has focus (Inference: typing frees it),
  and only while it's focused. GUI keeps the buttons with `Modal` set as
  the journal changes them, so asking costs nothing for the rest of the
  GUI; the window asks every frame and on every held-mouse movement. A held mouse's movement reaches scripts as
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
- A CameraRelative character faces the camera as the frame is drawn,
  once the render step has turned it, and the physics steps hold that
  heading; Studio's overshoot is left out (Approximation). Roblox's
  order puts it a frame or more behind (Studio's scripts read it two 60 Hz
  frames behind a camera turning steadily, 2026-11-03), and players feel
  even one: steered only by physics, Rook's trailed the mouse by a frame
  ([frame order](../../design/architecture/threads-and-frames.md#frame-order), Rook
  extension).
- The client steps once per drawn frame, that frame's length, as Roblox
  runs a frame per drawn frame (Studio's playtest client ran at 60).
  Physics runs whole 240 Hz steps and carries the rest to the next frame.
  Measured: a script reading a moving part each frame, in frames of 13
  to 18 ms, saw it move by exactly 3, 4 or 5 steps, never part of one, so
  scripts see the stepped state. Drawing doesn't: each moving part this
  side simulates is drawn between its last two steps, by how far the
  carried time is into the next (the standard interpolation, at most a
  step behind), and the current camera is moved by as much as its
  subject's root part is drawn away from where scripts see it, so the
  character stays still on screen while the world moves evenly. A 144 Hz
  client drawn at the stepped state moved one step, then two, frame to
  frame. `Dom::drawn_cframe` is where anything draws a part or looks
  through the camera.
- At the display's rate the window steps each frame by a whole number
  of refreshes, never less than one, and carries what the measured time
  leaves over or under (`rook-client`'s `window/frame_clock.rs`). After a late frame the
  next is usually measured early by as much, yet is shown a refresh
  later: at 60 Hz a 25 ms frame stepped two refreshes and the 8 ms one
  after it only its 8 ms, so for one shown frame the character and the
  camera on it moved a quarter as far, which looked like a jump backward
  while walking. Frames step as measured only once the steps run more than two
  refreshes ahead of the clock, as they can't while the display paces
  them.
- The mouse looks under the cursor again (Mouse.Hit, Target,
  ClickDetector hovers) when the camera, the cursor, `TargetFilter` or
  anything queries see changed (`Physics::generation`), not every frame.
  Measured: Mouse fires no `Changed` for any of its properties, moving the
  cursor or the camera (`Move` fires), so Rook writes them without
  signals.
- A character's root part, wherever Rook needs it (movement,
  `Humanoid.RootPart`, ClickDetector reach, the client sending its pose,
  lassos), is one rule: `rook_lua::root_part`.
- Interpolations and a Scriptable camera's Focus are updated after
  RenderStepped, before drawing (Inference).

## What's left

- Gamepad, touch (thumbsticks, the jump button, pinching) and VR input.
- Roblox's 2017 `CameraScript` and `ControlScript` for places with only
  one of them.
- `Camera.DiagonalFieldOfView` and `MaxAxisFieldOfView` read 0: Studio's
  values (120.4 and 96.6 at 70° in a 1270 × 793 viewport) weren't
  consistent from one FOV to the next, and the PlayerModule doesn't read
  them.
