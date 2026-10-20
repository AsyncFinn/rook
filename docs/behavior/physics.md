# Physics

What Rook found about joints, movers and scene queries in Studio, and the
rules `rook-physics` follows. Collision shapes have their own page,
[collision](collision.md).

## Surface joints (`MakeJoints`)

Observed in Studio 0.741 (MakeJoints still works there);
`rook_physics::joints::surfaces`.

- Two faces join when they face each other within 0.01 studs (gap or
  overlap), their edges line up (within about half a degree) and they
  overlap with some area (touching along an edge isn't enough).
- A Hinge, Motor or SteppingMotor surface on either face makes a Rotate,
  RotateV or RotateP; the calling part's wins. Otherwise Weld on either
  side makes a Weld, then Glue a Glue, then Studs, Inlet or Universal on
  both sides a Snap, except Studs to Studs and Inlet to Inlet.
- The joint is named after its class, parented to Part0 and framed at
  Part0's face centre with Z along the face's outward normal. Part0 is
  the part whose rotating surface decided the type, otherwise the
  calling part. Parts already joined aren't joined again.
- Only parts in Workspace join; they may join parts anywhere in it.
- A wedge's slope is a face; its box's top and front aren't. A wedge
  calling MakeJoints joins what's on its slope only through its own
  Weld surfaces, TopSurface before FrontSurface, framed at that box
  face. A part calling it on a slope sees the wedge's FrontSurface
  there. Rotating surfaces on either side of a slope join nothing.
- When both parts of a pair call (a model's MakeJoints), the earlier
  tries first and the later tries again if that made no joint, so a
  slope joins if either side would (Part0 is then the later caller).

Every other part is treated as its box (a wedge's side triangles as
rectangles). Balls, cylinders and corner wedges are boxes too: in
Studio a ball's Hinge makes a Rotate and its Studs a Snap, and a
cylinder's caps and curved side join, all framed at the box face.

## Movers

Instances that push an assembly with forces each 240 Hz step
(`rook_physics::joints::movers`). The legacy BodyMovers (BodyForce,
BodyThrust, BodyVelocity, BodyAngularVelocity, BodyPosition, BodyGyro,
RocketPropulsion) act on the assembly of the part they're parented to;
VectorForce, Torque and SpringConstraint act through attachments.

Observed in Studio 0.742 unless marked otherwise:

- A mover acts on its part's whole assembly, with the assembly's mass,
  at the centre of mass (no torque) unless it has a point of its own.
  Several movers add up. Anchored assemblies don't move.
- Limits (MaxForce, MaxTorque) clamp each world axis separately.
- BodyVelocity and BodyAngularVelocity reach their target within one
  frame whatever their P: they're velocity constraints whose only limit
  is MaxForce or MaxTorque, a true force and torque. BodyVelocity holds
  against gravity on the axes it may push along.
- BodyPosition and BodyGyro are spring-dampers whose response doesn't
  depend on mass or inertia (`movers::damping` for how D scales).
  BodyGyro's gains act at a quarter strength, on the sine of the angle,
  and its MaxTorque limits angular *acceleration*: a cube 2 or 4 studs
  wide speeds up by the same 1 rad/s² per unit of MaxTorque.
- RocketPropulsion, once fired, flies straight at its target at
  MaxSpeed, holding against gravity, with at most MaxThrust (ThrustP
  made no difference). It turns like a BodyGyro (TurnP, TurnD,
  MaxTorque) toward a direction between the target's (CartoonFactor 1)
  and its thrust's (0). Within TargetRadius it fires ReachedTarget and
  holds still until Abort.
- An assembly a mover only holds in place (a BodyGyro or BodyPosition
  at its target, a zero BodyVelocity, a force friction balances) falls
  asleep like any resting one, hinged vehicles included. Changing the
  mover wakes it. A mover asking for motion keeps it awake: BodyVelocity
  at 0.03 studs/s and BodyAngularVelocity at 0.05 rad/s did, at 0.01
  neither did (Approximation: any nonzero target keeps it awake).

## Scene queries

Rays, shape casts, regions, overlaps and touching parts
(`rook_physics::query`).

Queries see the current DOM, not last frame's physics: scripts often
create or move a part and query in the same frame (Observed: a ray hits a
part created or moved just before it). So every query first brings the
world up to date, then puts the colliders of bodies moved or rebuilt
since the last step where they are now, in the broad phase too (Rapier
only does that when it steps). Filters (`CanQuery`, ignore lists,
collision groups) are read from the DOM per candidate, so they're always
current as well.

Observed in Studio 0.742 unless noted:

- A part is skipped when both `CanCollide` and `CanQuery` are off
  (`CanQuery` alone does nothing while `CanCollide` is on).
  `Transparency` doesn't matter; anchored and unanchored parts count.
- Rays don't hit a part they start inside (or on the surface of), and
  reach at most 15,000 studs (`query::MAX_RAY_LENGTH`).
- Region3 and the `GetPartBounds*` queries test each part's `Size` box
  (`shape::world_box`); touching faces count. `GetPartsInPart` and
  `GetTouchingParts` test real shapes and need the parts to overlap.
- Terrain: rays and shape casts hit its surface, water's too unless they
  ignore water, and report Terrain with the material hit; a ray starting
  inside it hits nothing until it leaves. Region3, the `GetPartBounds*`
  queries and `GetPartsInPart` never return it; `GetTouchingParts` does
  where a part overlaps its solid surface, whatever the part's
  `CanCollide`.
