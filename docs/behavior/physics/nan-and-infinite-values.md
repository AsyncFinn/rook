# NaN and infinite values

Checked against Studio 0.744 on 2026-11-05, in a Baseplate playtest
(server), by script.

Scripts write NaN and infinity into parts more than you'd think: a
script dividing by a zero distance, say. Roblox never
lets either reach a part.

- **CFrame.** A CFrame with a NaN or infinite number anywhere in it,
  position or rotation, is written as the identity rotation at
  (0, -1,000,000, 0), and reads back so at once. The same for
  `Position` and `Orientation` writes. That is below any
  `FallenPartsDestroyHeight` (-500 by default), so a loose part goes on
  its next step; an anchored one stays down there. Huge finite numbers
  (1e30) are kept. Rook: `CFrame::written`, applied where parts' CFrames
  are written (`rook_physics::assembly::move_parts`, and Lua's
  `Position`, `Orientation` and `Rotation`).
- **Size.** A Size with a NaN or infinite number in it is ignored; the
  part keeps its size. A negative axis reads back 0.001 (not yet Rook's).
- **Velocity.** `AssemblyLinearVelocity`, `AssemblyAngularVelocity`,
  `Velocity` and `RotVelocity` with a NaN or infinite number in them
  write zero, the whole vector (`rook_physics::assembly::set_velocity`).
- `Orientation` and `Rotation` read no negative zeros: a new part's are
  `0, 0, 0`, though `CFrame.new():ToOrientation()` returns `-0` for X.

Physics also leaves out any part whose box has a NaN or infinite number
in it, whatever wrote it (a file, or a body thrown far enough), so one
can't stop the world: `rook_physics::shape::size_box` returns `None` for
it. A place's server crashed on one before.
