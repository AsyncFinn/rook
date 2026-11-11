# Faces, Axes and PhysicalProperties

Checked against Studio 0.747 on 2026-11-11.

Rook keeps these three as values with no Lua datatype of their own
(`Opaque` in `crates/rook-lua/src/types/misc.rs`), one userdata type
per kind, so each acts as its Roblox datatype:

- `typeof` gives `Faces`, `Axes` or `PhysicalProperties`.
- `tostring` of Faces lists the faces set in the order Right, Top, Back,
  Left, Bottom, Front; of Axes, X, Y, Z (`Axes.new(Enum.NormalId.Top)`
  prints `Y`); joined by `, `, and empty when none are set.
  PhysicalProperties print their six numbers as `%.9g`.
- Two with the same members compare equal.
- Reading a member they don't have is an error with the key quoted
  ([datatype errors](datatype-errors.md#members-a-datatype-doesnt-have)).
