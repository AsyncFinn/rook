# Terrain surface

Checked against Studio 0.742 on 2026-10-15.

How Roblox builds terrain's surface from the [voxels](voxels.md), measured
with them.

*Surface nets* (Confirmed; zeux, "Roblox: Large-scale
terrain", SIGGRAPH 2020, names the method):

- The corners of the dual grid are voxel centers. Every cell (the cube
  between eight voxel centers) where solid meets air has one vertex, the
  mean of the points where its edges cross the surface. Each voxel edge
  that crosses gets a quad joining the vertices of the four cells around
  it.
- An edge from a solid voxel of occupancy `o` to air crosses `4o` studs
  from the solid voxel's center. So a lone full voxel is a cube reaching
  1.333 studs from its center, a half-full one 0.667, and a full one-voxel
  slab is 8 studs thick (its top 4 above its voxels' centers, at the
  center of the air above). The edge of a full slab slopes down 1 in 2,
  of a half-full one 1 in 3.
- Edges between two solid voxels never cross, whatever their occupancies
  or materials.
- Each quad is made of its solid voxel's material, which raycasts report.
- Measured: a layer of random occupancies over a full layer (2,209 rays
  from above, worst error 0.006 studs, the occupancy rounding), and a 4³
  block of random voxels in Grass, Snow and Sand (2,300 rays from all six
  sides). Two-voxel cases (faces, edges and corners touching, mixed
  occupancies and materials) match to 0.003 studs.

*Which diagonal splits a quad* (partly known): every quad of the
heightfield above splits the same way (Rook's `DIAGONALS`), but in the
random block about a third of the quads that aren't flat split the other
way. Shortest diagonal, best angles, flattest fold, outward bulge, closest
to the edge's own crossing, and the cells' materials all fail to predict
it. Rook splits every quad the heightfield's way (Approximation). In the
random block every ray but six of 2,300 hits Rook's surface within 0.01
studs with one split or the other; only bent quads differ, and smooth
terrain's quads are nearly flat.

*Materials that move their vertices* (zeux 2020: "shift", "cubify",
"quantize"), each vertex by its cell's dominant material, the one with the
most occupancy among its corners. Measured with a lone voxel, a slab, and
the same random block in one material:

| Material | What it does | Lone full voxel reaches | Confidence |
|---|---|---:|---|
| Grass, LeafyGrass, Sand, Snow, Mud, Ground, Asphalt, Salt, Glacier, Limestone, Sandstone | Nothing: plain surface nets | 1.333 | Confirmed |
| Brick, WoodPlanks | Cubify all the way: each vertex at its cell's center, so voxels are 4-stud boxes at any occupancy | 2.00 | Confirmed (random block exact) |
| Cobblestone | Quantize to 0.15 of the cell (0.6 studs; a slab's top 3.8 above its centers). Step 1 read this as cubifying a tenth of the way plus a random shift; Studio's table (step 2) shows it quantizes, which predicts both measurements | 1.40 | Confirmed (lone voxel and slab) |
| Concrete | Quantize to halves of the cell (2 studs) | 2.00 | Confirmed (random block exact) |
| Pavement | Quantize to quarters of the cell (1 stud) | 1.00 | Confirmed (random block exact) |
| Rock, Slate, CrackedLava, Basalt, Ice | A pseudo-random shift of each vertex, up to about 1 stud, fixed by position (Slate and CrackedLava shift alike, Rock, Basalt and Ice each their own way) | 0.4 to 1.6, varying | Approximation: since step 2, Rook's own pseudo-random shift of the table's size |
| Water | Half a voxel lower than a solid's surface (step 2) | | Confirmed |

A voxel whose occupancy byte is 0 still holds 1/256 of its material
(`ReadVoxels` reads it so), so a cell whose only solid corners are such
voxels takes their material. Rook once let Air win that tie, and Air,
untextured and uncolored, drew white specks a few studs across where
those faint voxels lay over grass (fixed 2026-10-18).

Quantizing rounds halves away from the cell's center: the edge of a
half-full Concrete slab comes in to its voxels' centers. Where a cell
mixes these materials, Rook moves its vertex by the dominant material, and
13% of the rays into a random block of Brick, WoodPlanks, Pavement,
Concrete and Grass miss (Approximation; neither the fullest single voxel,
corner counts, nor a blend of each material's move does better). The
shift's pseudo-random offsets depend on Roblox's own hash, so Rook can't
reproduce Rock and the other rough materials' facets exactly.

*Shading* (observed): Brick, WoodPlanks, Concrete and Pavement are drawn
faceted (hard edges) and Grass and Snow smooth ("materials also define
soft/hard edges"). Since step 2 Studio's table decides: its `hard` and
`hardsoft` materials (Concrete, Brick, WoodPlanks, Rock, Basalt,
Cobblestone, Pavement) get flat normals per triangle, the rest smooth
normals averaged over the faces around each vertex across chunk and
material seams.

*Drawing it* (Rook): `rook-player` meshes Workspace's Terrain when the
place loads (and again when its `SmoothGrid` changes), a chunk per thread,
and draws one mesh per chunk and material through the mesh-file path, in
the material's `Terrain.MaterialColors` color (the default table when the
place saved none; textured since step 2). Water is drawn in `WaterColor` at `WaterTransparency`
and casts no shadow. In Play mode the client draws its own copy: Rook
replicates `SmoothGrid`, which the API dump marks NotReplicated because
Roblox streams voxels to clients its own way. Far chunks are meshed from coarser voxels and hung
with skirts: see [level of detail](detail.md).

*Screenshot comparison* (2026-10-15): a FillBall of Grass, FillBlocks of
Brick and Concrete, a FillBall of Pavement, a random Snow heightfield and
a block of Water, built in Studio, read back and written into a copy of
Studio's Baseplate template. Rook's render from the same camera has the same
outlines: a smooth Grass ball, a sharp Brick box, the faceted Concrete
block and Pavement ball, and the Snow heightfield's shape. Colors,
textures and the water surface are step 2's. A 75,000-voxel place, from the
same camera in both, shows the same landmass.

*Costs* (release build, meshing only): a 75,000-voxel place 20 ms (56,000
triangles), a 1.18-million one 63 ms (426,000), a 751,000 one 92 ms (553,000),
a 25.2-million one 317 ms (1.73 million, mostly the flat water). `rook-tools terrain
<file>` prints what a file's Terrain holds and how long it takes to mesh.

## Scripts

Scripts changing a Terrain property or `Workspace.GlobalWind` redraw the
terrain: the waves, wind and grass are read again, and the water and
blade entries redrawn (`RenderChanges::terrain`); a changed `SmoothGrid`
rebuilds the scene.
