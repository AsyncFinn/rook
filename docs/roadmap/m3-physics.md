# M3 Physics

Rapier underneath, Roblox's rules on top, every rule checked in Studio.

1. ✅ Rapier world, anchoring, write-back.
2. ✅ Joints and assemblies (welds, Snaps, Motor6D, legacy surface joints, `MakeJoints`).
3. ✅ Contacts, `Touched`, collision groups, materials.
4. ✅ BodyMovers, driven hinges (RotateV, RotateP), the most-used constraints, `Velocity` and conveyors.
5. ✅ Queries on Rapier, legacy first, by files using them: `FindPartOnRay` (205), `FindPartsInRegion3` (135), `FindPartOnRayWithIgnoreList` (123), `GetTouchingParts` (27), `FindPartOnRayWithWhitelist` (15), `Workspace:Raycast` with `RaycastParams`/`RaycastResult` (15), `FindPartsInRegion3WithIgnoreList`/`WithWhiteList` (17), `GetPartBoundsInBox`, `GetPartsInPart` and `GetPartBoundsInRadius` with `OverlapParams` (12), `Blockcast` and `Spherecast` (8); also Camera `ScreenPointToRay`/`ViewportPointToRay` (51), which scripts pair with raycasts. The mouse and ClickDetectors moved over, and the engine's own raycast geometry is gone. Clients got a physics world that only answers queries.

6. **Match Studio**: close every difference left in what steps 1 to 5
   built.
   - *Scope*:
     - RotateV's torque rule and RotateP's kinematic motion.
     - BodyPosition and BodyGyro damping (fitted today); BodyGyro's and
       RocketPropulsion's `MaxTorque` axes on rotated parts;
       RocketPropulsion's `ThrustP`/`ThrustD` and settling speed.
     - Hinge servo easing and `MotorMaxAcceleration`.
     - Movers solved with the constraints, as Roblox does, and their
       `GetLastForce`.
     - Joints snapping parts as soon as they activate; which assembly
       and root Roblox picks (several anchored parts, equal
       candidates); legacy automatic joints as parts move; a saved
       disabled WeldConstraint.
     - When touches end and which part fires first; the old
       `CollisionGroups` string; `TouchEventsUseCollisionGroups`.
     - Query result order; regions finding parts made this frame;
       `OverlapParams.Tolerance` and the overlap threshold; TouchInterest
       for `GetTouchingParts`; `Shapecast` and `ArePartsTouchingOthers`.
     - `Model:MoveTo` stepping up out of obstructions; `Mouse.Hit`'s own
       ray rules.
     - Out: the remaining constraints, buoyancy and adaptive timesteps
       (M15).
   - *Evidence*: the [stubs](../guide/stubs.md) rows taken over by this
     step, and the open Approximations and Inferences
     on the physics pages.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 5, and no stubs row names this step.

Steps 1 to 5 met the old exit criterion (the conformance batch, `physics/*`,
passes and unanchored places settle plausibly); step 6 holds them to Studio.
