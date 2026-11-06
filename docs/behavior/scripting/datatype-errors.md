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
