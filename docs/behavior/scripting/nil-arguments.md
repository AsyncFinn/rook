# Nil Instance arguments

Checked against Studio 0.747 on 2026-11-14.

An engine method's Instance parameter (`Object` in the API dump) takes an
explicit `nil` as the null object. Only an argument the call leaves out is
`Argument 1 missing or nil`, and a value that isn't an Instance is
`Unable to cast value to Object`. Scripts lean on this: a handler that hands
`Debris:AddItem` a variable that's still nil goes on to its next line, so an
error there stops what comes after it.

What the null object means is up to each method. A method that checks the
class of its argument gives null the same message it gives any other class:

| Call with `nil` | Result |
|---|---|
| `Debris:AddItem(nil, t)` | nothing; `t` is still cast to a number |
| `x:IsDescendantOf(nil)` | `true` |
| `x:IsAncestorOf(nil)` | `false` |
| `CollectionService:AddTag(nil, tag)`, `RemoveTag` | nothing; the tag must be passed but isn't checked |
| `CollectionService:HasTag(nil, tag)` | `false` |
| `CollectionService:GetTags(nil)` | `{}` |
| `Humanoid:EquipTool(nil)` | nothing, as for anything but a Tool |
| `AudioEmitter:GetAudibilityFor(nil)`, `AudioListener`'s | `0` |
| `TweenService:Create(nil, info, {})` | a Tween whose `Instance` is nil; it plays and cancels like any other |
| `TweenService:Create(nil, info, goals)`, goals not empty | `TweenService:Create failed because Instance is null` |
| `PhysicsService:SetPartCollisionGroup(nil, g)` | `Parameter 1 must be BasePart in SetPartCollisionGroup.` |
| `PhysicsService:CollisionGroupContainsPart(g, nil)` | `Parameter 2 must be BasePart in CollisionGroupContainsPart.` |
| `part:CanCollideWith(nil)` | `CanCollideWith expects a BasePart argument.` |
| `WorldRoot:GetPartsInPart(nil)` | `Part parameter must be BasePart` |
| `Terrain:PasteRegion(nil, ...)` | `region has to be a TerrainRegion` |
| `Humanoid:ApplyDescription(nil)` | `Argument 1 missing or nil` |
| `MarketplaceService:PlayerOwnsAsset(nil, id)` | `Argument 1 missing or nil` |
| `Chat:FilterStringForBroadcast(s, nil)` | `Argument 2 missing or nil` |

`SoundService:SetListener`'s listener isn't an `Object` but a Tuple: left out
it is `SoundService:SetListener called with an incorrect argument.`, and nil
or a value that isn't an Instance is
`SoundService:SetListener value given is not a valid instance`.

CollectionService's forms number their arguments from the instance, so
`CollectionService:AddTag(nil)` is `Argument 2 missing or nil`.

## How Rook does it

`Args::object` in `rook_lua::runtime::ctx::args` reads an `Object` parameter as
Roblox casts it: it tells a missing argument from an explicit nil by how many
the call passed. `Args::object_of` adds the class check, so null and other
classes fall through to the method's one message. `Args::instance` is kept
for the methods whose own check turns null away as missing. The Luau-written
`TweenService:Create` is passed the argument count for the same reason.
