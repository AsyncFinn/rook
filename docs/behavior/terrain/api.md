# The Terrain API

Checked against Studio 0.742 on 2026-10-16.

Measured in an empty place, reading each call's result back with `ReadVoxels`
(bytes below are `SmoothGrid`'s, 255 full) and recorded as the
conformance case `terrain/terrain_api`. Archive use, by files calling
each method name (some names are shared with other classes, so these
are upper bounds): `WorldToCellPreferSolid` 339, `GetCell` 329,
`WorldToCell` 169, `CellCenterToWorld` 166, `SetCell` 147,
`AutowedgeCells` 137, `SetWaterCell` 104, `PasteRegion` 69, `FillBlock`
65, `CopyRegion` 51, `GetMaterialColor` 42, `ReadVoxels` 41, `SetCells`
40, `GetWaterCell` 39, `WriteVoxels` 17, `WorldToCellPreferEmpty` 13,
`FillBall` 8, `CountCells` 7, `SetMaterialColor` 6, `AutowedgeCell` 4,
`FillRegion` 3, `FillWedge` and `FillCylinder` 2, `ReplaceMaterial` and
`CellCornerToWorld` 1.

*Fills* (Confirmed): each voxel near the shape gets an occupancy from
where its center lies, rounded to the nearest byte:

- **`FillBlock`**: the least, over the box's own three axes, of how much
  of the voxel's 4-stud span the box covers. A 1-stud cube in a voxel
  fills a quarter of it (64), not 1/64; a 4-stud cube straddling eight
  voxels fills each half (128); one shifted half a stud up fills 223 and
  32. A box turned 45° fills its edge neighbors 202 and its corner
  neighbors 22, and nothing outside its world bounding box.
- **`FillBall`**: `(r - d) / 4 + 1/2` of the distance `d` from the
  center: radius 0.5 fills its voxel 159, radius 4 its face neighbors
  128 and edge neighbors 22.
- **`FillCylinder`** (along the CFrame's Y): `(r - d) / 4` of the
  distance from the axis, rounded down (127 where a ball would give
  128), wherever the voxel reaches into the height at all (a voxel only
  touching an end is left out; one reaching 2 studs past it is filled
  whole).
- **`FillWedge`** (a WedgePart's shape, high at +Z): across, as a block;
  along the slope, `(t - y) / 4 + 1/2` where `t` is the slope's height
  half a voxel towards its low end, so a voxel-sized wedge fills
  nothing; its bottom and back cut wherever the voxel reaches into
  them at all.
- Materials terrain doesn't have (Plastic, Wood, Neon, Glass,
  ForceField, ...) fill as Grass. A negative size or radius is
  "Extents cannot be empty".

*Merging* (Confirmed): a solid material takes a voxel when it fills at
least as much as what is there (Rock 128 over Grass 128 is Rock 128;
Rock 128 over Grass 255 leaves the Grass). Water fills air and water but
never takes a solid voxel's place (full water over half Grass leaves the
Grass at 128). Air takes its occupancy away (half a voxel of Air on full
Grass leaves 127).

*Regions* (Confirmed): `FillRegion`, `ReplaceMaterial`, `ReadVoxels` and
`WriteVoxels` take a Region3 whose corners, in voxels, are truncated
toward zero (1 to 8 is voxels 0 and 1; -1.9 to 2.1 is none) and a
resolution that must be 4 ("Resolution has to be 4"). `ReadVoxels`
returns `[x][y][z]` arrays with a `Size` field, occupancies `(byte +
1) / 256` and 0 for air; more than 4,194,304 voxels is "Region is too
large". `WriteVoxels` checks the arrays' shape ("Bad argument materials
to 'WriteVoxels' (2x1x1 array expected)") and each item's type, clamps
occupancies to 0..1 and leaves air for 0. `FillRegion` fills each voxel
whole, water over solids too. `ReplaceMaterial` keeps each voxel's
occupancy (half Grass becomes half Water); Air as the target clears, and
Air as the source fills whole. `Clear` empties the terrain.

*Cells* (Confirmed): modern Roblox still honors the legacy cell
methods on smooth voxels; a cell is a voxel.

- `WorldToCell` floors; `CellCenterToWorld` and `CellCornerToWorld`
  round fractional cells half away from zero. `WorldToCellPreferSolid`
  and `WorldToCellPreferEmpty` pick the side of a face a point sits on
  that is solid (or empty).
- `SetCell` and `SetCells` write the smooth material standing in for
  the legacy one (Granite is Slate; Iron, Aluminum, Gold, CinderBlock
  and Cement are Concrete; WoodLog is WoodPlanks, Gravel Asphalt,
  MossyStone Pavement, RedPlastic Sandstone, BluePlastic Ice) with an
  occupancy for the block shape (Solid 255, the vertical and horizontal
  wedges 127, CornerWedge 85, InverseCornerWedge 170); Empty clears.
- `GetCell` names the legacy material back (Grass for materials no
  legacy one maps to, such as Rock and Basalt), a block shape for how
  full the voxel is (Solid up to byte 42, CornerWedge to 84,
  VerticalWedge to 127, InverseCornerWedge to 212, then Solid) and
  always `NegZ`.
- `SetWaterCell` fills the cell with water over whatever was there;
  `GetWaterCell` says whether it's water, with `None` and `NegX`.
  `AutowedgeCell` returns false and `AutowedgeCells` does nothing.

*Copies and colors* (Confirmed): `CopyRegion` takes a Region3int16 in
cells, both corners included ("The region can not be empty or
inverted."), and returns an unparented TerrainRegion with its
`SizeInCells` and `IsSmooth` true; `PasteRegion` puts it at a corner,
its air replacing what is there only with `pasteEmptyCells`.
`GetMaterialColor` and `SetMaterialColor` read and write
`MaterialColors`; Water and Air are "Unsupported terrain material",
other materials "Invalid terrain material".

*When collision follows* (Confirmed): reads see an edit at once, but rays
find a fill only 2 frames later, and still hit a carved hole's old
surface for a frame.

*Rook*: the edits live in `rook_terrain::edit` and `rook_terrain::cells`;
the Lua side is `rook-lua`'s `api/methods/terrain.rs`. Scripts edit the
physics world's decoded copy of Workspace's Terrain
(`rook_physics::terrain::edit`), so reads see edits at once. At the end
of the frame the colliders of the chunks the frame's edits reach (2
voxels around them, as far as the mesher reads) come down, sleeping
bodies there wake, and the voxels are written to `SmoothGrid` once,
where the renderer and replication see them. So rays see edits from the
next frame. A changed `SmoothGrid` from anywhere else (a client
receiving the server's edit) is compared with the voxels already held,
and only the chunks that differ are rebuilt, in physics and in the
renderer. Workspace always has a Terrain now, as in Roblox.

*Costs* (release build, an 86k-part place, the archive's largest terrain: 2.5 MB of
`SmoothGrid`, 3,165 chunks): decoding takes 250 to 300 ms and encoding
90 ms, so a frame with an edit there costs about that on the server and
on each client (meshing a chunk again is about 2 ms). Small terrains
cost next to nothing.

*Approximations and gaps*:

- `CountCells` counts voxels that aren't air. Studio counts differently
  for small terrain (a lone voxel, or two side by side, count 2; a full
  chunk its 32,768).
- `WorldToCellPreferEmpty` on a point where every cell around is solid
  returns the floor cell; Studio returned another cell, two away.
- The fill rules were measured with axis-aligned wedges and cylinders
  (and turned blocks); turned wedges and cylinders follow the same rules
  in their own frame (Inference).
- Water over a solid voxel fills its water channel (shorelines), which
  Rook doesn't draw yet; Rook keeping a replaced water voxel's water as
  the solid's is Inference.
- The renderer rebuilds its scene list after an edit, and every edit
  frame decodes and encodes the whole `SmoothGrid` (see Costs).
- Not implemented: `ReadVoxelChannels` and `WriteVoxelChannels` (no
  archive use), the `*_beta` and Roblox-only methods, TerrainRegion's
  transforms.
