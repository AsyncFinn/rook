# Datatype errors

Checked against Studio 0.746 on 2026-11-06.

A failing datatype method or constructor reaches `pcall` as a string, as an
Instance member's error does
(`conformance/cases/scripting/datatype_errors.luau`). Rook's datatypes are
userdata whose methods fail with an mlua error object, so what hands an error
to a script, `pcall`, `ypcall`, `xpcall`'s handler and `coroutine.resume`,
turns one into its message (`protected_calls` in `rook_lua::api::globals`).
Errors a script raises itself pass as they are, tables and Instances too.

| Call | Message |
|---|---|
| `Vector3.Lerp(1)` | `invalid argument #1 to 'Lerp' (Vector3 expected, got number)` |
| `v:Dot(1)` | `invalid argument #2 (Vector3 expected, got number)` |
| `cframe:ToWorldSpace(1)` | `invalid argument #2 (CFrame expected, got number)` |
| `cframe * 1` | `invalid argument #2 (Vector3 expected, got number)` |
| `ray:ClosestPoint(1)` | `invalid argument #2 (Vector3 expected, got number)` |
| `CFrame.new("x")` | `invalid argument #1 to 'new' (Vector3 expected, got string)` |
| `UDim2:Lerp(1, 2)` | `Unable to cast double to UDim2` |
| `Color3:Lerp(1, 2)` | `Unable to cast double to Color3` |
| `region:ExpandToGrid("a")` | `Unable to cast string to float` |
| `TweenInfo.new("x")` | `TweenInfo.new first argument expects a number for time.` |
| `v.Foo` | `Foo is not a valid member of Vector3` |
| `brickColor:Foo()` | `'Foo' is not a valid member of BrickColor` |
| `v.X = 1` | `X cannot be assigned to` |

The `invalid argument` messages begin with the script and line in Studio.
Rook's don't: by the time the error reaches a protected call the place it was
raised has gone. Nor do Vector3's own functions called without a Vector3 say
`to 'Lerp'` in Rook: they're plain functions, which an argument error doesn't
name, and naming them would cost every call.

## Members a datatype doesn't have

Checked against Studio 0.747 on 2026-11-11, for every datatype scripts
can make. Reading a key a datatype has no member for is an error, never
nil, worded one of two ways (`crates/rook-lua/src/types/members.rs` owns
both):

- *Plain*, `Foo is not a valid member of CFrame`: CFrame, Vector3,
  Vector2, Color3, UDim2, Ray, Region3, RaycastParams, OverlapParams,
  DateTime, Random and Content. A key that isn't a string is a bad
  argument instead: `invalid argument #2 (string expected, got number)`.
  Enum items, enums and `Enum` name themselves in quotes:
  `Foo is not a valid member of "Enum.Material.Plastic"`.
- *Quoted*, `'Foo' is not a valid member of UDim`, any key (`'1'`): UDim,
  BrickColor, TweenInfo, Rect, NumberRange, NumberSequence,
  ColorSequence and their keypoints, Vector3int16, Vector2int16,
  Region3int16, Axes, Faces, PhysicalProperties, Font,
  Path2DControlPoint and CatalogSearchParams.

Games test for an Instance with `pcall(function() return v.ClassName
end)`, so a datatype that read nil passed as one.

## Constructors don't check numbers

Measured in Studio 0.747 (2026-11-11): a datatype constructor reads a number
argument that isn't a number, or a string that reads as one (`'0.5'`,
`'0x10'`, `'inf'`), as 0, and never errors. `Color3.new('New Yeller')` is
black, `Vector3.new(true, 2, 3)` is (0, 2, 3), and `CFrame.new(1, 2, 3, 'a',
0, 0, 1)` is a plain position. That holds for `Color3.new`, `fromRGB` and
`fromHSV`; `Vector2`, `Vector3`, `Vector2int16` and `Vector3int16`; `UDim`;
`UDim2.new`, `fromScale` and `fromOffset`; `Rect.new`; and `CFrame.new`'s
3, 7 and 12 number forms (`rook_lua::ctx::args::Num`). Methods and other
functions still check: `CFrame.Angles('a')`, `fromEulerAnglesXYZ`,
`fromAxisAngle` and `Color3:Lerp(c, 'a')` fail with `Unable to cast string
to float`, `NumberRange.new('a')` with `invalid argument #1 to 'new'
(number expected, got string)`. Places lean on this: one sets an alarm
light to `Color3.new('New Yeller')` on its way to enabling a button.
