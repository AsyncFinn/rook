# Pivots

Checked against Studio 0.744 on 2026-10-26.

Where `GetPivot` and `PivotTo` measure from: `rook_lua`'s
`api::methods::physics::models`.

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

## Model scale

Measured in Studio 0.748 on 2026-11-22, with the rest from creator-docs
`Model:ScaleTo`:

- `GetScale` reads the model's `Scale`, 1 until it's scaled.
- `ScaleTo` scales the model about its pivot by the new scale over the old:
  each part's size, `PivotOffset` and offset from the pivot, the offsets of
  Attachments and Bones, joints' `C0` and `C1`, WeldConstraints' offsets,
  and meshes' `Offset`. A file mesh's `Scale` scales, but a primitive
  mesh's (a Sphere SpecialMesh, a BlockMesh) doesn't, as it already scales
  with its part. A model inside moves its pivot and scales its own `Scale`
  (measured: a part of size 2, 4, 6 read 4, 8, 12 after `ScaleTo(2)`; a
  nested model's pivot at 10, 4, 0 went to 20, 8, 0 and its `Scale` to 2).
- Lengths, distances and speeds inside the model scale too: Rope and Rod
  `Length` and `Thickness`; Spring `FreeLength`, `Radius`, `MinLength` and
  `MaxLength`; Prismatic `UpperLimit` and `LowerLimit`; ParticleEmitter
  `Size`, `Speed` and `Acceleration`; Fire `Size`; Beam `Width0`, `Width1`
  and `CurveSize0`; Trail `MaxLength`; Texture `StudsPerTileU` and `V`;
  Sound `RollOffMinDistance` and `RollOffMaxDistance`; BillboardGui
  `StudsOffset` and `MaxDistance`; Humanoid `HipHeight`, `WalkSpeed` and
  `JumpHeight`. A Humanoid's `JumpPower` scales by the square root (50 read
  70.71 after `ScaleTo(2)`), so the height it reaches scales by the factor.
  A BillboardGui's `Size` and a Light's `Range` stay. Inference: a
  CylindricalConstraint's limits scale as a Prismatic's, a Beam's
  `CurveSize1` as its `CurveSize0`, and a ParticleEmitter's `Size`
  envelopes with its sizes.
- Zero and negative scales raise `Only positive non-zero scale values are
  allowed.`; NaN changes nothing. Inference: infinity is ignored like NaN.

## BulkMoveTo

`WorldRoot:BulkMoveTo` moves each part to its CFrame, as setting its
`CFrame` would. Its errors, measured in Studio 0.748 and checked in this
order:

- Something in the part list that isn't an Instance: `Unable to cast value
  to Object`.
- Lists of different lengths: `Length of part list (2) and length of CFrame
  list (1) do not match.`
- Something that isn't a part: `Can only call BulkMoveTo on BaseParts,
  partList[1] is a Folder.`
- Something that isn't a CFrame: `CFrame list must only contain CFrames,
  cframeList[1] is a Vector3.`
- An `eventMode` that isn't a `BulkMoveMode`: `Unable to cast string to
  BulkMoveMode`. Approximation: Rook signals the moves as any other move,
  whichever mode is given (listed in [stubs](../guide/stubs.md)).
