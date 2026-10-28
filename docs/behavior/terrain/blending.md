# Terrain blending

Checked against Studio 0.744 on 2026-10-28.

Where two materials meet, or a material's top texture meets
its side one, Studio blends them over about a cell instead of switching
along the voxels' stair steps. Read from Studio 0.742's
terrain shaders (`shaders/shaders_glsl3.pack`, the texture-array
`SmoothCluster` vertex and fragment shaders, plain GLSL) and checked on
screenshots in Studio and Rook of the same fills.

*How Studio does it* (Confirmed, from its shaders and screenshots):

- Each vertex of a triangle carries one texture layer (a material's top,
  side or bottom texture), and every vertex of the triangle carries all
  three corners' layers, so the triangle samples three layers. A corner's
  barycentric weight (1 at its own vertex) is how much of its layer shows:
  the color, normal and specular maps are mixed by the plain weights, so a
  seam is a linear ramp about a cell wide.
- The pack holds two kinds of terrain shader. One mixes by the plain
  weights alone. The other can sharpen them by each layer's height map
  (weight times height, the highest winning over a soft edge, from the
  `blend` numbers in the material tables), but falls back to the plain
  weights when a per-vertex value is 1 or more, and fades to them far
  away. What picks between them isn't known.
- Studio draws the plain ramp: screenshots of the same seams in TerrainTest
  show the same soft, even edges with the legacy and the 2022 textures,
  whose height maps differ, so the heights play no part (checked
  2026-10-28). The `blend` numbers go unused in Rook.
- The vertex shader also turns and shifts each layer's coordinates by a
  per-vertex random number (`detiling`, `rotation`; see
  [materials.md](materials.md)).

*Which layer a vertex takes* (Inference, consistent with the screenshots):
its cell's material (the solid corners' material with the most occupancy,
the lower index on a tie, as the shapes use) and, for materials whose top
and side differ, the facing of its smooth normal (more than 22.5 degrees
up for the top). Measured with a 32 × 48-stud Grass slab against a Rock
one: Studio's blend starts at the voxels' boundary, on the grass side,
and runs 5 to 6 studs into the rock (Grass has the lower index, so the
boundary's vertices are Grass); a Grass ball fades from grass to its side
texture over a band rather than along a zigzag. Sand at water's edge
doesn't blend: water is its own surface over the sand, as in Rook.

*How Rook draws it*: `rook-terrain` gives every vertex its layer and
splits each chunk's drawn surface (`ChunkMesh::draws`; collisions keep
`surfaces`, by the voxels' materials) into the triangles wholly of one
layer, drawn as before, and the seams, the triangles whose corners
differ. A seam triangle is drawn once per layer it holds, with unshared
vertices carrying their barycentric weights and which corners hold the
draw's layer: the first corner's layer opaque, the others added over it,
each draw the sum of its corners' weights (`terrain_share` in
`rook-render`'s `terrain.wgsl`), so the shares add up to the whole. Each
draw lights its own layer and the results are summed; Studio mixes the
maps and lights once (Approximation, alike where the layers' normals
agree).

*Approximations and gaps*:

- The random turn and shift come from a 4-stud grid, not the mesh's
  vertices ([materials.md](materials.md)).
- MaterialVariants on terrain blend the same way (not checked in Studio).

*Costs* (2026-10-28, release builds, `--bench 300 --fly 8` from cameras
over each place's terrain, the build before against this one, alternating,
twice, on the archive's three largest voxel places, with this change's
shores and far water): frames, triangles and GPU passes unchanged within
noise (the 25-million-water-voxel place: median frame 2.61 and 2.49 ms
before, 2.60 and 2.45 after; the 2.6-million-triangle mountain place:
GPU main pass 8.45 and 2.77 ms before, 8.59 and 2.07 after, its frames
its content loading in both builds).
