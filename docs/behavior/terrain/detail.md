# Terrain level of detail

Checked against Studio 0.744 on 2026-10-30.

Studio draws far terrain from coarser voxels: rings around the camera,
each meshed from voxels twice the size of the ring inside it, the
cascaded level of detail zeux describes (2014, 2016; see
`docs/design/research.md`). An rbxl stores only the 4-stud voxels
(`Terrain.SmoothGrid`), so Studio builds the coarse levels at runtime.

## How it was measured

On the Baseplate place, at Studio's default graphics settings
(`settings().Rendering.QualityLevel` reads `Level21`, `EditQualityLevel`
`Automatic`), a strip of Snow 4,096 studs long with its corner on a
4,096-stud grid line: a full layer of voxels with three lanes of
checkerboard bumps on it, of 1, 2 and 4 voxels (periods of 8, 16 and 32
studs), and a row of Brick balls of radius 40 every 128 studs beside it.
Brick moves each vertex to its cell's center
([surface](surface.md)), so a ball is a stack of boxes exactly one cell
wide: the steps of its outline give the cell size wherever it stands. A
lane of bumps flattens where the cells grow too big to hold them, and
where a ring ends the strip's surface steps up or down, since coarser
voxels sit higher (below). Neon posts beside the strip every 32 studs
gave distances, read by projecting them with the camera's known pose.

Two traps:

- The levels follow the camera Studio draws from, and catch up over a
  few frames, a piece at a time: a screenshot taken from a temporary
  camera (the MCP capture's `camera_position`) shows the levels of the
  camera before it, except the terrain right under the temporary camera,
  part of which (a 512-stud block, once) refines during the capture. The
  clean shots set `Workspace.CurrentCamera.CFrame` by script, waited 4 to
  5 seconds, and captured from that camera.
- Studio's edit camera puts `Camera.Focus` back next to itself, so
  whether the levels follow the camera or its Focus wasn't separated
  (they follow the pair).

## What Studio draws

| Cells (studs) | From the camera, along the strip | Seen by | Confidence |
|---:|---|---|---|
| 4 | out to 256 | balls at 192 in 4-stud steps; every lane bumpy | Confirmed |
| 8 | 256 to about 768 | balls at 320 to 704 in 8-stud steps; the 1-voxel lane flat, the strip steps at 256 | Confirmed |
| 16 | about 768 to 1,024 | balls at 832 and 960 in 16-stud steps | Observed (balls only) |
| 32 | 1,024 to about 2,048 | the 4-voxel lane flat past 1,024 with a step there; balls merged into 64-stud blocks | Confirmed |
| 64 and up | past about 2,048 | another step at 2,048 to 2,100 | Observed |

- The rings stay put in the world: with the camera moved 128 studs along
  the strip, the steps at 1,024 and 2,048 didn't move. Each level's ring
  snaps to a grid of that level's chunks rather than following the camera
  smoothly (Confirmed).
- Far rings lie where near ones do (Observed, 2026-10-28, on a 3,072-stud
  Grass slab 24 studs thick with hills and a lake): seen along the slab's
  side from its corner, its top and bottom edges run straight to the
  horizon across every ring, where a coarse surface reaching half its own
  voxel past its solid would step up 2 to 62 studs. A small step does
  show at a ring's edge where a hill crosses it (seen at 4 degrees). No
  cracks showed at the steps.
- The rings are squares on the ground, not boxes (Observed, 2026-10-28):
  straight above the lake, from 320, 1,200 and 4,000 studs up (fields of
  view 70, 20 and 6 degrees), the shore, a 14-stud rock and a sand island
  draw in 4-stud cells, as close up. A box 256 studs across each way
  around the camera would draw them in 32-stud cells or coarser from
  1,200 up.
- Small shapes fade into far rings rather than standing out: the slab's
  14-stud rock and the hills' rock faces stay smooth at 1,500 studs.
- Nothing is cut off far away: from 20,000 studs off, the strip still
  draws, as one thick slab of the coarsest level (Observed).
- How far each ring reaches by graphics quality wasn't measured: Studio
  ran at its top level, which is what Rook draws.

The 768-stud edge is the odd one: 256, 1,024 and 2,048 are powers of two,
and a ring of 8-stud cells reaching 768 doesn't fit the nesting of the
others (a level's ring made of the next level's whole chunks). It rests
on balls 704 and 832 studs out, seen from 400 studs up, with nothing on
the strip to confirm it.

## How Rook does it

`rook_terrain::lod`, drawn by `rook-view`'s terrain cache:

- Six levels, cells of 4 to 128 studs. Each level's voxels average the
  2×2×2 blocks of the level below: a coarse voxel holds the eight's mean
  occupancy of whichever of solid or water holds more, solids taking the
  material with the most occupancy. Materials that move their vertices
  move them by the coarse cell, so far Brick is boxes of the coarse cells,
  as in Studio. The levels are made once when the voxels load, and an
  edit averages again only the coarse chunks over what it changed.
- Level `k` draws a square of 4×4 columns of its 32-voxel chunks (256
  studs across each side of its center at level 0, doubling), every chunk
  in them however high or low, less the square of the level inside it;
  the last level draws everything outside the square before it, however
  far. Each square's center is a corner of the next level's chunks, so
  the hole it leaves there is whole columns, and moves only once the
  camera is half that chunk and a sixteenth more away, so a camera at an
  edge doesn't flip it back and forth. Each square is kept inside the
  next.
- A coarse voxel's surface crosses where its eight 4-stud voxels' would:
  half a 4-stud voxel past the solid it holds, measured from its own
  bottom, rather than half its own voxel past it, so thick ground lies at
  the same height in every ring. A voxel too empty to reach past its
  center crosses there, so shapes smaller than half its voxel shrink
  away. Rock's random vertex shift stays its 4-stud size far away
  (scaled with the coarse cell, it threw far rock's vertices tens of
  studs about); Brick's cubes and the quantized materials' steps still
  scale with the cell.
- From a camera on a grid corner that puts the edges at 256, 512, 1,024,
  2,048 and 4,096 studs: Studio's except the 8-stud ring, which ends at
  512 instead of about 768 (Approximation: a ring ending at 768 isn't
  whole chunks of the next level, so it can't nest the same way).
- Chunks are hung with skirts: from every edge where a chunk's
  upward-facing surface ends at the chunk's side, a strip one of its
  voxels deep, straight down, in the surface's own layers (Ulrich's
  chunked level of detail hangs them so). Where rings meet, the higher
  side's strip covers the crack; inside a ring the strips lie in the
  solid. Strips hung from undersides, along the normals and two voxels
  deep, stood up through any slab thinner than that as walls and shards
  (the stray triangles seen from 300 studs up). Skirts are drawn only:
  collisions use the bare 4-stud surface.
- Water has no skirts (they'd show through it); its surface crosses as a
  solid's does, so open water lies at the same height in every ring. A
  coarse voxel near water holds the eight's mean reach of water's surface
  (`Voxel::water_surface_fill`: a Water voxel's occupancy, a solid's water,
  a whole voxel for a dry solid), as water rests on what's under it.
  Averaging the water alone sank a lake one voxel deep into its bed once
  the coarse voxels grew past four times its depth (a level-2 lake drew at
  -10); with the solids counted it lies at its level in every ring
  (`far_lakes_lie_at_their_level`).
- A coarse voxel with air on both sides along a crossing (a slab
  thinner than the voxel) holds its solid centered, not resting on the
  voxel's floor: its surface lies the solid's own thickness from the
  center when that's under 4 studs, as one 4-stud voxel that full
  crosses, else half a 4-stud voxel past the solid. Measured on
  2026-10-30 (Studio 0.744, Snow strips 4,096 studs long, each side seen
  straight on at a narrow field of view beside a striped ruler at 96 to
  3,000 studs): a one-voxel slab a quarter full (20 to 24 studs) spans
  21.1 to 23.0 near, 19.1 to 21.1 in 8-stud cells, 23.0 to 25.0 in 16 and
  15.1 to 17.1 in 32, each its coarse voxel's center and 1 either side,
  which the rule gives exactly; a full slab from 8 to 32 spans 1.3 to
  30.9 in 32-stud cells, where the rule gives 2 to 30 (Approximation).
  So Studio's thin ground steps between rings too, by up to half a
  coarse voxel. Collapsed to the voxel's center as before, thin ground
  (a common painted layer) drew a stud off Studio's in every ring, and
  past the finest ring it covered grass blades or left them floating.
- When the camera moves a box, the chunks whose level changed are meshed
  on a thread of their own, one after another, while the old ones stay
  drawn, then swapped in all at once: no hole, nothing drawn twice, and
  the frame never waits (Studio, too, catches up over a few frames). A
  screenshot meshes in place and waits.
- Grass blades grow on the drawn chunks of the levels that can come
  within their 200 studs (`lod::nearest`: the 8-stud ring can start 112
  studs out, the 16-stud one no nearer than 224), so they stand on the
  ground that's drawn, as Studio plants them on the triangles it draws
  (`FFlagRenderGrassUseBarycentricPlantingPoints`). Planted on the 4-stud
  surface wherever a coarser ring drew the ground, they hung over it or
  sank under it in a straight line along the ring's edge.

*Costs* (dev build, the 103k-instance place of M21 step 2, holding
still): the main pass draws 4.3 million triangles in 782 draws instead
of 10.7 million in 5,445, and takes 3.3 to 3.6 ms of GPU instead of 5.7
to 6.3. Flying 8 studs a frame for 300 frames (2,400 studs, across
every ring), scene updates take the same time as before (median 2.6 to
2.8 ms against 2.5 to 3.0, the same two load-time frames over 16.7 ms),
with 2.5 million triangles instead of 7.2. Screenshots from above the
place's terrain, before and after: the terrain near the camera matches
(the pixels that differ are moving parts), and only the far shore
differs, coarser. Level 0 is meshed exactly as before, so near terrain
keeps its comparison with Studio ([surface](surface.md)).

*Costs of the 2026-10-28 rings* (release builds, `--bench 300 --fly 8`
across each place's terrain, base and new runs alternating, twice, on the
archive's three largest voxel places): frames, scene updates and the
slowest ring moves unchanged within noise (the largest, 25 million water
voxels: median frame 3.40 and 2.81 ms before, 3.01 and 3.06 after; 72k
to 89k triangles before, 55k to 62k after; the mountain place's frames
are its content loading in both builds). A ring move meshes whole
columns, but the empty chunks in them are skipped as before.
