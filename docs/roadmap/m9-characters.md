# M9 Characters

SpawnLocation 4,194 files, Tool 3,036, Humanoid 2,873, Seat 1,881,
Animation 1,812, HopperBin 1,813, Team 1,086, Animator 964, VehicleSeat
887. Scripts call `TakeDamage` in 2,374 files, `MoveTo` in 2,138 and
`LoadAnimation` in 2,022. Needs physics, the head mesh, terrain and GUI
(the backpack and health bar are M8's CoreScripts).

Roblox's own character scripts are in the user's Studio install as plain
Luau: the PlayerModule (camera and controls), `RbxCharacterSounds`, the
R15 Animate script and the health regeneration script. Which of those
Rook runs, against the 2018 Core-Scripts it can ship, is
[undecided](unscheduled.md#undecided) and is settled before step 5.

1. ✅ **Rigs and spawning** ([rigs and looks](../behavior/characters/rigs.md),
   [spawning and teams](../behavior/characters/spawning.md),
   [names and health over heads](../behavior/characters/overhead.md)):
   R15 from Roblox's rig file and R6 built as Studio builds it,
   StarterCharacter, `GameSettingsAvatar`, looks from the avatar API with
   the classic default offline, body colors and classic scaling in
   HumanoidDescriptions and `ApplyDescription`, spawn choice, placement,
   facing and ForceFields measured in Studio, team assignment, the team
   property rules and team spawns, respawning after `Died`, and the names
   and health bars over heads. The conformance cases
   `characters/spawn_choice` and `characters/team_properties` pass.
   Whether auto-assignment balances teams moved to M10 (it needs several
   clients); R15 body parts from descriptions and AnimationConstraint
   joints are M11's.
   - *Scope*:
     - The default R15 and R6 rigs: parts, Motor6Ds (1,674 files) and
       attachments, `HipHeight`, `RigType`; StarterPlayer's
       `StarterCharacter`; HumanoidDescription (238 files) and
       `ApplyDescription` (152) as far as body scale and parts (the rest
       is M11).
     - Spawning: SpawnLocation's `Enabled`, `Neutral`, `TeamColor`,
       `AllowTeamChangeOnTouch` and `Duration` (the ForceField, which
       scripts also create in 1,115 files); a random pick among the
       valid spawns; standing on top of the spawn clear of other parts.
     - Teams (Team in 1,086 files): `AutoAssignable`, a player's `Team`,
       `TeamColor` and `Neutral`, and team spawns. Moved here from M12,
       since spawning needs them.
     - Respawning: `Players.RespawnTime` after `Died`.
     - Names and health over heads (`DisplayName`, `DisplayDistanceType`,
       `HealthDisplayType`), whether the engine or the CoreScripts'
       `CharacterNameHandler` draws them.
   - *Studio questions*: where Studio's default rig comes from in a
     local place with no game settings, and which rig type it gets; the
     spawn point with no SpawnLocation, with only disabled ones, and
     with team spawns but no matching team; how high above the spawn
     the root part appears.
   - *Done when*: joining an archive place spawns the right rig on the
     right team's SpawnLocation, and conformance cases for spawn order
     and team assignment pass.
2. ✅ [**The client simulates its character**](../behavior/characters/ownership.md). On Roblox a player's client
   owns its character's assembly: it steps the physics and sends the
   result to the server. Today Rook's server simulates everything and
   a client's physics world only answers queries, so this comes before
   walking instead of waiting for M10.
   - *Scope*: client DataModels step the assemblies they own; the
     server takes their positions and velocities (the first changes a
     client sends up); ownership of a character goes to its player;
     `GetNetworkOwner` answers. `SetNetworkOwner` for other parts and
     automatic ownership are M10's.
   - *Studio questions*: does a client-owned part's `Touched` fire on
     the server, and against what positions; how stale is a server's
     view of a moving character (one frame? interpolated?); what happens
     to a server script's `CFrame` write to a client-owned part.
   - *How*: the client's physics world in `rook-lua`'s `World` starts
     stepping for owned assemblies; `rook-net` gains the upward half of
     the Source and Sink pair, limited to owned assemblies.
   - *Done when*: a character moved on the client is where the server
     reads it the next frame, and server Touched handlers fire for it.
3. **Movement and states**: the Humanoid controller.
   - *Scope*: floor detection, `HipHeight`, `WalkSpeed`, `MoveDirection`,
     `Move` (277 files), `MoveTo` (2,138) with `MoveToFinished`,
     `WalkToPoint`, `JumpPower`, `JumpHeight` and `UseJumpPower`,
     `AutoRotate`, `MaxSlopeAngle`, stepping up ledges, standing on
     moving parts, pushing parts; every `HumanoidStateType`, with
     `GetState` (748 files), `ChangeState` (605), `SetStateEnabled` (406)
     and the state events; climbing trusses and ladders of parts (TrussPart
     is in 1,504 files); swimming in terrain water; `PlatformStand`.
   - *Studio questions*, as recordings: positions per Heartbeat for a
     walk at known speed, a jump (apex height against JumpPower and
     gravity), a walk off a ledge, up a step, up a slope at and past
     `MaxSlopeAngle`, onto a moving platform, up a truss, and into
     water. Reads happen on the client, which owns the character.
   - *Method*: the MCP's `user_keyboard_input` holds keys in the
     playtest client; a client case logs positions each Heartbeat.
   - *Done when*: the recordings match within a tenth of a stud, the
     state sequences match, and a character walks, jumps and climbs
     through an archive obby.
4. **Health and death**: `Health`, `MaxHealth`, `TakeDamage` (blocked by
   a ForceField), regeneration, `Died`, `BreakJointsOnDeath`,
   `RequiresNeck`, `BreakJoints` (2,108 files) and `MakeJoints` (2,212)
   on characters, a body falling apart, and respawning.
   - *Studio questions*: the order of `Died`, `HealthChanged`, state
     changes and joint breaking; what removing the Neck does; regen rate.
   - *Done when*: conformance cases for damage and death pass, and a
     killed character falls apart and respawns as in Studio.
5. **Camera and controls**: the default camera and control scripts
   (source as decided above), and the engine members they use.
   - *Scope*: `CameraType` (Custom, Scriptable for the archive's
     cutscenes, Follow, Track, Attach, Watch, Fixed, Orbital),
     `CameraSubject`, zoom limits, first person (`CameraMode`), shift
     lock, occlusion modes, `UserInputService.MouseBehavior`, `Modal`,
     ContextActionService bindings, `RunService:BindToRenderStep`,
     UserGameSettings, `Camera:Interpolate` (247 files). With no
     character, `rook-player`'s fly camera stays.
   - *Evidence first*: run the chosen scripts in a client and list what
     they call that Rook lacks; the list is this step's work.
   - *Done when*: the camera follows, zooms, collides and locks as in
     Studio in a reference place, and archive cutscenes play.
6. **Animation**, enough for the Animate scripts: Animator (964 files),
   `LoadAnimation`, `AnimationTrack` play, stop, fade, `AdjustSpeed`
   (1,172 files), `AdjustWeight`, `Looped`, priorities and blending (the
   Animate scripts rely on them so tool holds win over walking),
   `KeyframeReached` and markers, `GetPlayingAnimationTracks` (379),
   KeyframeSequence (444 files) and animation assets.
   - *Evidence*: many places carry the 2010s Animate script in their
     own tree (its `playAnimation` is in 1,132 files and `moveSit` in
     1,003), so those run as saved; the rest get the default.
     Animations are assets: count how many of the archive's animation
     ids asset delivery serves anonymously, and with the key.
   - *Studio questions*: a rig's `Motor6D.Transform` read at set times
     of a track, with two tracks at different priorities and weights.
   - *Done when*: the Transform reads match, and characters idle, walk,
     run, jump, fall, climb, swim, sit and hold tools with the Animate
     script. IK, layered animation and replication to other players are
     M11's.
7. **Tools and the backpack**: Tool (scripts create one in 462 files),
   the handle held by `RightGripAttachment` or the legacy `Grip`,
   `RequiresHandle`, `CanBeDropped`, `ManualActivationOnly`, `Enabled`,
   `Equipped` with its Mouse, `Activated`, `Deactivated`, `EquipTool`
   (283 files) and `UnequipTools` (619), picking tools up by touch, and
   the number keys. HopperBin as [decided](unscheduled.md#undecided)
   before this step.
   - *Studio questions*: the grip weld's name and parts, what equipping
     does to the Humanoid's arm, the order of `Equipped` and the tool's
     reparenting, and dropping with Backspace.
   - *Done when*: the archive's classic gear (swords, slingshots, rocket
     launchers, building tools) equips and works.
8. **Seats and VehicleSeats**: the SeatWeld on touch, `Occupant`,
   `Disabled`, `Sit` (81 files), `Humanoid.Sit` and `SeatPart`, jumping
   off; VehicleSeat's `Throttle` and `Steer` from the keys, `MaxSpeed`,
   `Torque` and `TurnSpeed` driving its hinges and legacy motors, and
   the speed HUD (`VehicleHud` CoreScript).
   - *Studio questions*: the SeatWeld's C0 and C1, how long before a
     seat takes someone again, and VehicleSeat's driving forces against
     a reference car.
   - *Done when*: characters sit and stand, and an archive car drives.
