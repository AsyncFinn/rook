# Bindables

Checked against Studio 0.746 on 2026-11-06.

What arrives through `BindableEvent:Fire` and `BindableFunction:Invoke`
(`rook_lua::api::methods::bindables::copy`;
`conformance/cases/signals/bindable_copies.luau`).

*When.* `Fire` copies when it's called, not when the listener runs, and each
listener gets a copy of its own, so one changing a table isn't seen by the
next. `Invoke` copies its arguments and the callback's results. A value that
can't be copied raises the error from `Fire` or `Invoke`, and nothing is
delivered.

*What's copied.* Only tables. Functions, datatypes, Instances and buffers
pass as they are, inside tables too, and nils keep their places. A thread
arrives as nil. A copy has no metatable and isn't frozen. A table reached
twice arrives as two copies, across arguments too; one reached from itself
raises `tables cannot be cyclic`.

*Arrays.* A table with a length keeps 1 to that length, holes inside included
(`{"a", nil, "c"}` has length 3), and loses every other key: `{1, 2, x = 3}`
has no `x`, and `{[1] = 1, [3] = 3}`, of length 1, loses 3.

*Dictionaries.* Any other table keeps its keys as strings:

| Key | Arrives as |
|---|---|
| A number | Its whole part: `1.5` is `"1"`, `-2.7` is `"-2"` |
| A number out of 64-bit range, infinity | `"-9223372036854775808"` |
| A Vector3 | `<Vector3> (1, 1, 1)` |
| A CFrame | `<CoordinateFrame> (0, 0, 0, 1, 0, ...)` |
| An Instance | `<Instance> (Workspace)` |
| A table | `<Table> (table: 0x...)` |
| A function | `<Function> (function: 0x...)` |
| A BrickColor | `<int> (Medium stone grey)` |
| An EnumItem | `<token> (Enum.Material.Wood)` |
| A boolean | Raises `Invalid table key type used`, nested too |

So `{[2] = "a", x = 1}`, having no length, arrives with keys `"2"` and `"x"`.
Other datatypes as keys follow the same form with their type's name
(Inference).
