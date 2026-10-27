# Rigs and looks

Checked against Studio 0.744 on 2026-10-27.

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

Roblox's own rig file, `avatar/characterR15DynamicHeadV2.rbxm` from its
client packages (or the install), loaded when the first R15 character
spawns: 15 MeshParts (the body meshes 7430070991 and on, the head
123480233606534, through asset delivery) with Motor6Ds, attachments and
`OriginalSize` values, and a Humanoid with `HipHeight` 2. Studio's
`Players:CreateHumanoidModelFromDescription` with a new
HumanoidDescription builds the same parts, then scales them by the
description ([Looks](#looks)): a new description has `BodyTypeScale` 0.3
and `ProportionScale` 1, so the default body is not the file's size.
Without the file, characters are R6. (The older `characterR15.rbxm`,
which Rook read before 2026-10-27, is the 2017 body: `HipHeight` 1.35,
the head 2.15 above the root.)

The default body, from a new HumanoidDescription, R15, parented and
settled (offsets of part centers from the root part's; arms are
mirrored on Left):

| Part | Size | Offset from root |
|---|---|---|
| HumanoidRootPart | 2, 2, 1 | 0, 0, 0 |
| Head | 1.1590, 1.1820, 1.1606 | 0, 1.6738, 0 |
| UpperTorso | 1.9432, 1.6980, 1.0040 | 0, 0.2490, 0 |
| LowerTorso | 1.9914, 0.4007, 1.0040 | 0, -0.8000, 0 |
| RightUpperArm | 1.0011, 1.2416, 1.0019 | 1.4721, 0.4276, 0 |
| RightLowerArm | 1.0011, 1.1175, 1.0019 | 1.4721, -0.2021, 0 |
| RightHand | 0.9843, 0.3158, 1.0283 | 1.4721, -0.8659, 0 |
| RightUpperLeg | 0.9928, 1.3629, 0.9727 | 0.5, -1.4714, 0 |
| RightLowerLeg | 0.9928, 1.3007, 0.9728 | 0.5, -2.3339, 0 |
| RightFoot | 1.0089, 0.3120, 1.0011 | 0.5, -3.0365, 0 |

Its `HipHeight` is 2.1924: the feet's bottoms are 3.1924 below the
root's center. Rook's default body has these sizes, its offsets within
0.0021 (the legs 0.4979 out, Studio's settled constraints 0.5) and
`HipHeight` 2.1925. The PlayerModule's camera orbits 1.5 above the root,
inside the head just below its center. Measured in Edit: detached, before it is
parented, the model still has the file's offsets (head 1.586 above the
root) with the scaled sizes.

Studio 0.744 builds a playtest character's R15 joints as
AnimationConstraints with BallSocketConstraints (the avatar joint
upgrade) and `CreateHumanoidModelFromDescription` does the same in
Edit. Rook keeps the file's Motor6Ds, which the archive's scripts reach
for by name (Motor6D is in 1,674 files). Body part meshes from other
descriptions are M11's.

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
body whose Humanoid has `AutomaticScalingEnabled` is scaled: each part
by its Rthro blend times the classic scales, its attachments and its
joints' ends by its part's factor (to within 1e-4 in Studio), and
`HipHeight` becomes how far the legs reach below the root part's bottom.

- Classic: width, height and depth (the root part's too), the head by
  `HeadScale` alone.
- Rthro blend: from 1 toward the part's factor at `BodyTypeScale` 1, by
  `BodyTypeScale`; that factor goes from wide to slender by
  `ProportionScale`. Both blends are linear (measured at six pairs, to
  within 1e-5). Right parts as Left:

| Part | Wide (proportion 0) | Slender (proportion 1) |
|---|---|---|
| Head | 0.942, 0.942, 0.942 | 0.896289, 0.942, 0.896289 |
| UpperTorso | 1.033, 1.309, 1.14 | 0.905346, 1.204232, 1.013333 |
| LowerTorso | 1.033, 1.309, 1.14 | 0.985687, 1.004605, 1.013333 |
| UpperArm, LowerArm | 1.129, 1.342, 1.132 | 1.003556, 1.207921, 1.006222 |
| Hand | 1.066, 1.174, 1.231 | 0.947556, 1.174, 1.094222 |
| UpperLeg | 1.023, 1.506, 1.023 | 0.976145, 1.40093, 0.909333 |
| LowerLeg | 1.023, 1.506, 1.023 | 0.976145, 1.300518, 0.909333 |
| Foot | 1.079, 1.267, 1.129 | 1.02958, 1.133274, 1.003556 |

With HeightScale 1.05, WidthScale 0.8, DepthScale 0.9 and HeadScale 1.1
on top of the defaults, Studio's root part was 1.6, 2.1, 0.9 and
`HipHeight` 2.3020, every part matching the product. These are parts
whose `AvatarPartScaleType` is Classic (none set, as in the file);
other scale types are M11's.

`Humanoid:ApplyDescription` paints and rescales the same way, from the
applied description's scales to the new ones, and keeps a copy;
`GetAppliedDescription` returns a copy of it.
`Players:GetHumanoidDescriptionFromUserId` reads the same avatar answer
into a new HumanoidDescription. Body part meshes, clothes, accessories,
faces and animations from descriptions are M11's.
