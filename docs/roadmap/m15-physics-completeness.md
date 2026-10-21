# M15 Physics completeness

1. MeshPart and union collision with their real shapes (collision
   fidelity, `PhysicsData`), replacing boxes. Unions done: they collide
   and are queried as the hulls Studio saved (see
   [collision](../behavior/physics/collision-shapes.md#unions)); MeshParts remain.
2. The remaining constraints and movers, by files using them:
   RopeConstraint 17, RodConstraint 12, SpringConstraint 11, Prismatic 5,
   AlignPosition and AlignOrientation 4, BallSocket 2, LinearVelocity,
   AngularVelocity, Plane, Cylindrical, LineForce, NoCollisionConstraint.
3. Buoyancy in terrain water, and `Workspace.FluidForces` (aerodynamics).
4. Sleeping and adaptive timestepping as Roblox does them.
5. A second physics conformance batch, recorded in Studio.
