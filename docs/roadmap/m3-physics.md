# M3 Physics

Rapier underneath, Roblox's rules on top, every rule checked in Studio.

1. ✅ Rapier world, anchoring, write-back.
2. ✅ Joints and assemblies (welds, Snaps, Motor6D, legacy surface joints, `MakeJoints`).
3. ✅ Contacts, `Touched`, collision groups, materials.
4. ✅ BodyMovers, driven hinges (RotateV, RotateP), the most-used constraints, `Velocity` and conveyors.
5. ✅ Queries on Rapier, legacy first, by files using them: `FindPartOnRay` (205), `FindPartsInRegion3` (135), `FindPartOnRayWithIgnoreList` (123), `GetTouchingParts` (27), `FindPartOnRayWithWhitelist` (15), `Workspace:Raycast` with `RaycastParams`/`RaycastResult` (15), `FindPartsInRegion3WithIgnoreList`/`WithWhiteList` (17), `GetPartBoundsInBox`, `GetPartsInPart` and `GetPartBoundsInRadius` with `OverlapParams` (12), `Blockcast` and `Spherecast` (8); also Camera `ScreenPointToRay`/`ViewportPointToRay` (51), which scripts pair with raycasts. The mouse and ClickDetectors moved over, and the engine's own raycast geometry is gone. Clients got a physics world that only answers queries.

M3's exit criterion is met: the physics conformance batch (`physics/*`, six cases) passes, and unanchored places settle plausibly.
