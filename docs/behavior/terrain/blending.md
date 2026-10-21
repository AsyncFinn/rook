# Terrain blending

Checked against Studio 0.742 on 2026-10-16.

Where two materials meet, or a material's top texture meets
its side one, Studio blends them over about a cell instead of switching
along the voxels' stair steps, the higher texture (by its height map)
showing through the lower in a ragged edge. Read from Studio 0.742's
terrain shaders (`shaders/shaders_glsl3.pack`, the texture-array
`SmoothCluster` vertex and fragment shaders, plain GLSL) and checked on
screenshots in Studio and Rook of the same fills.

*How Studio does it* (Confirmed, from its shaders):

- Each vertex of a triangle carries one texture layer (a material's top,
  side or bottom texture), and every vertex of the triangle carries all
  three corners' layers, so the triangle samples three layers. A corner's
  barycentric weight (1 at its own vertex) is how much of its layer shows.
- Each weight is multiplied by its layer's height there: the alpha of
  Studio's specular texture, from the pack's `<height>` image, scaled and
  offset by two numbers per layer and clamped to 0 to 1. A corner's share
  is its weighted height minus the largest of the other two, plus half the
  softness, over the softness (clamped to 0 to 1), the softness being the
  corners' per-layer values blended by the weights; the shares are then
  normalized. The color, normal and specular maps are mixed by the shares.
- Where the first layer's texture coordinates change by more than a
  seventh of a repeat per pixel (far away), the shares fade to the plain
  weights, and so they do near an edge between two corners of the same
  layer (its fade term is `20 × (weights of corners whose layer another
  corner shares) - 19`).
- The vertex shader also turns and shifts each layer's coordinates by a
  per-vertex random number (`detiling`, `rotation`; see Breaking up
  repeats above), so each corner's height is read where its copy lies.
  Rook reads each layer's height through the same 4-stud grid as its
  color.

*The table's numbers* (Inference): each texture's `blend` in Studio's
material tables is `{min, max, scale}`. `scale` is the softness. `min`
and `max` are read as the range the height map spans (the height map's 0
to 1 standing from `min` to `max`), so a texture with a low range (Sand
top: -0.297 to 0.798) gives way to its neighbors. The other reading, `min`
to `max` stretched to 0 to 1, drew grass/rock seams about 2 studs wide in
Rook where Studio's are 5 to 6; this one draws them about a cell (4
studs), with the same ragged edge. Textures without a `blend` take their
material's (the legacy table's Ice).

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
vertices naming its three corners' layers: the first corner's layer
opaque, the others added over it, each draw its layers' share of
Studio's blend (`terrain_share` in `rook-render`'s `terrain.wgsl`), so the
shares add up to the whole. Every draw works the shares out alike (the
triangle's own normal picks the projection planes for them), from a
per-frame table of each layer's height map and blend numbers. Rook keeps
the pack's height in the blue of its roughness-metalness image. Each draw
lights its own layer and the results are summed; Studio mixes the maps
and lights once (Approximation, alike where the layers' normals agree).

*Approximations and gaps*:

- Rook's grass/rock seam is about a cell wide, Studio's 5 to 6 studs.
- Adapters without binding arrays (`images_bound.wgsl`) bind only a
  draw's own images, so every layer counts as full height there: seams
  blend by the corners' weights alone, softened, without the ragged edge.
- MaterialVariants on terrain blend without a height map, at their
  material's softness (what Studio does there isn't known).
- The random turn and shift come from a 4-stud grid, not the mesh's
  vertices (above).

*Costs* (release build, `--bench 300`, the build before against this
one, two runs each, other builds idle): on a 33k-part place whose
terrain gains seams (25 terrain entries become 82), the render medians
stay within the noise (13 to 37 ms before, 17 after). On a 133k-part
place the GPU's main pass went from 8.8 to 9.4 ms and the shadow pass
from 3.2 to 3.4 ms with the same terrain entries: the larger fragment
shader, about 6%.
