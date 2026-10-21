# Movers

Checked against Studio 0.742 on 2026-10-12.

Instances that push an assembly with forces each 240 Hz step
(`rook_physics::joints::movers`). The legacy BodyMovers (BodyForce,
BodyThrust, BodyVelocity, BodyAngularVelocity, BodyPosition, BodyGyro,
RocketPropulsion) act on the assembly of the part they're parented to;
VectorForce, Torque and SpringConstraint act through attachments.

Observed in Studio unless marked otherwise:

- A mover acts on its part's whole assembly, with the assembly's mass,
  at the center of mass (no torque) unless it has a point of its own.
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
