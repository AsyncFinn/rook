# Rigs and looks

Checked against Studio 0.744 on 2026-10-24.

What a player's character is built from: `rook_lua`'s `characters::rigs`
and `characters::appearance`, and `rook_engine`'s `rigs`.

## Which body

- StarterPlayer's `StarterCharacter`, a Model, is cloned as it is
  (creator-docs StarterPlayer).
- Otherwise the rig is StarterPlayer's `GameSettingsAvatar` (the game's
  avatar settings on Roblox; scripts can't set it): R6, R15, or the
  player's own with PlayerChoice. In a local place with no settings,
  Studio's playtest characters are R15 (observed).
- The model is named after the player, its `PrimaryPart` is the
  HumanoidRootPart.

Studio 0.744 builds a playtest character's R15 joints as
AnimationConstraints with BallSocketConstraints (the avatar joint
upgrade) and `Players:CreateHumanoidModelFromDescription` does the same in
Edit. Rook keeps Roblox's Motor6Ds, which the archive's scripts reach for
by name (Motor6D is in 1,674 files).

## R15

Roblox's own rig file, `avatar/characterR15.rbxm` from its client
packages (or the install), loaded when the first R15 character spawns:
the 15 parts with their meshes (asset ids 1699715537 and on, through
asset delivery), Motor6Ds and attachments, and a Humanoid with
`HipHeight` 1.35. Without the file, characters are R6.

Studio's own characters differ in two ways Rook doesn't follow yet:
their body parts come from the HumanoidDescription (an empty one gives
newer meshes, 7430070991 and on, and `HipHeight` 2.19), and they are
scaled with `BodyTypeScale` 0.3 and `ProportionScale` 1. Both are M11's.

## R6

Built by Rook from Studio's: Head (2, 1, 1) with a Head SpecialMesh at
scale 1.25 and the `face` Decal (`rbxasset://textures/face.png`), Torso
(2, 2, 1) with an empty `roblox` Decal, arms and legs (1, 2, 1), and an
invisible HumanoidRootPart (2, 2, 1, Medium stone grey, `CanCollide`
off). Only the Head and Torso collide. In Studio's child order: Head,
Torso, Left Arm, Right Arm, Left Leg, Right Leg, Humanoid,
HumanoidRootPart.

Motor6Ds (C0 and C1 share each rotation, given as rows):

| Joint | In | Part1 | C0 | C1 | Rotation |
|---|---|---|---|---|---|
| Right Shoulder | Torso | Right Arm | (1, 0.5, 0) | (-0.5, 0.5, 0) | (0,0,1 / 0,1,0 / -1,0,0) |
| Left Shoulder | Torso | Left Arm | (-1, 0.5, 0) | (0.5, 0.5, 0) | (0,0,-1 / 0,1,0 / 1,0,0) |
| Right Hip | Torso | Right Leg | (1, -1, 0) | (0.5, 1, 0) | (0,0,1 / 0,1,0 / -1,0,0) |
| Left Hip | Torso | Left Leg | (-1, -1, 0) | (-0.5, 1, 0) | (0,0,-1 / 0,1,0 / 1,0,0) |
| Neck | Torso | Head | (0, 1, 0) | (0, -0.5, 0) | (-1,0,0 / 0,0,1 / 0,1,0) |
| RootJoint | HumanoidRootPart | Torso | (0, 0, 0) | (0, 0, 0) | (-1,0,0 / 0,0,1 / 0,1,0) |

Attachments, all unrotated: Hair and Hat (0, 0.6, 0), FaceFront
(0, 0, -0.6) and FaceCenter on the Head; Neck (0, 1, 0), BodyFront and
BodyBack (0, 0, ∓0.5), Left and Right Collar (∓1, 1, 0), Waist Front,
Center and Back (0, -1, ∓0.5 or 0) on the Torso; Shoulder (0, 1, 0) and
Grip (0, -1, 0) on each arm; Foot (0, -1, 0) on each leg.

## Looks

A player's look comes from the `avatar` web domain
([web endpoints](../../design/web-endpoints.md)): `GET
/v1/users/{id}/avatar`, read for `playerAvatarType`, `bodyColors`
(BrickColor numbers) and `scales`. The first spawn waits for the answer.
With no endpoint, or when it fails, the look is Roblox's classic
default: Bright yellow head and arms, Bright blue torso, Br. yellowish
green legs.

The body is painted, gets a `Body Colors` BodyColors, and its Humanoid a
HumanoidDescription of the look (Studio's characters carry both). An R15
body whose Humanoid has `AutomaticScalingEnabled` is scaled the classic
way: parts, their attachments and their joints' ends by width, height
and depth (the head by its own scale), `HipHeight` by height
(Approximation: Roblox blends toward Rthro proportions too).

`Humanoid:ApplyDescription` paints and rescales the same way, from the
applied description's scales to the new ones, and keeps a copy;
`GetAppliedDescription` returns a copy of it.
`Players:GetHumanoidDescriptionFromUserId` reads the same avatar answer
into a new HumanoidDescription. Body part meshes, clothes, accessories,
faces and animations from descriptions are M11's.
