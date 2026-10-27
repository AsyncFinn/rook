# Pivots

Checked against Studio 0.744 on 2026-10-26.

Where `GetPivot` and `PivotTo` measure from: `rook_lua`'s
`api::methods::models`.

- A part's pivot is its `CFrame` times its `PivotOffset`.
- A Model with a `PrimaryPart` pivots on that part's pivot, whatever its
  stored pivot is (measured: a model with `WorldPivot` set 50 studs up
  and a PrimaryPart 10 studs over pivoted at the part; `WorldPivot`
  still read the stored 50). Setting `WorldPivot` then changes neither.
- Without a PrimaryPart, a Model pivots on its stored pivot
  (`WorldPivotData`), or its bounding box's center when it has none.
- `PivotTo` moves every part of the model rigidly so the pivot lands on
  the target, and the stored pivot with them.
- Not yet: Studio sets the stored pivot to the PrimaryPart's pivot when
  `PrimaryPart` is cleared (measured: a cleared model's pivot was where
  its PrimaryPart had been moved to); Rook keeps the stored one.
