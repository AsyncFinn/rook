# Textures and decals

What Rook found while building M4 step 4: images, Decals, Textures, mesh textures, surfaces and materials, observed in Studio. The plan itself is in [ROADMAP.md](../../ROADMAP.md#m4-content-meshes-textures-decals).

**Textures and decals (step 4)**, built 2026-10-13. How images load and
draw is in `docs/architecture.md` §8 "Rendering" and "Content"; what's
still partial is in `docs/behavior/stubs.md`.

*What the archive uses* (`rook corpus-scan`, 3,058 files that load; the
scan now counts looks too):

| What | Files | Uses |
|---|---:|---:|
| Decal on a part | 1,403 | 192,261 |
| Decal on a part with Transparency 1 | 596 | 63,030 |
| Decal faces: Top, Front, Back, Right, Left, Bottom | | 48,407, 44,808, 40,926, 31,517, 23,945, 23,205 |
| Decal `Transparency` not 0 / `Color3` not white | 208 / 29 | 10,729 / 3,926 |
| Decal `ZIndex` other than 1 | 6 | 10 |
| Texture on a part | 459 | 265,749 |
| Texture `StudsPerTile`: 2 × 2 (the default), 4 × 4, 1 × 1, 8 × 8, ... | 209, 167, 161, 174 | 104,721, 39,085, 37,399, 12,006 |
| Texture `OffsetStudsU`/`V` not 0 | 6 | 4,599 |
| SpecialMesh FileMesh with a `TextureId` | 1,620 | 56,965 |
| SpecialMesh `VertexColor` not (1, 1, 1) | 673 | 19,433 |
| MeshPart with a `TextureID` | 30 | 7,721 |
| Surfaces: Studs (1.71 million on Top), Inlet (1.94 million on Bottom) | 2,307, 2,305 | 1,775,393, 2,001,938 |
| Surfaces: Weld, Universal, Glue, Hinge, Motor, SteppingMotor | 1,726, 364, 526, 863, 509, 8 | 1,588,989, 217,677, 54,308, 51,966, 9,794, 332 |
| SmoothNoOutlines; the old value 9 | 214; 28 | 844,528; 5,343 |
| Material Plastic, or none saved (old files: Plastic) | 1,701; 939 | 3,170,633; 939,990 |
| Wood, Slate, Concrete, Grass, Ice, DiamondPlate, CorrodedMetal, SmoothPlastic | 531, 502, 366, 358, 353, 351, 252, 201 | 264,342, 544,028, 516,853, 40,902, 18,674, 169,646, 96,308, 227,653 |
| Foil, Brick, Sand, Granite, Marble, Metal, Pebble, Fabric, Neon | 177, 158, 149, 127, 125, 110, 105, 101, 84 | |
| `Reflectance` above 0 | 1,405 | 377,042 |

*Image formats* (`rook content --place`, which now decodes every image
reference and totals the results by format, over 60 random archive places
and a 2016 core place, anonymous): of the images that load, 648 distinct are
PNG (1,926 uses), 16 JPEG, 1 BMP (10 uses) and 2 DDS; none is a GIF or
TGA, and none failed to decode. 996 distinct images (12,235 uses) are
refused without an account. The install's own textures are DDS (DXT1 and
DXT5), including the `.tex` sky files. So Rook reads PNG, JPEG, BMP and DDS.

*Checked in Studio 0.742* (2026-10-13, a labelled UV-grid test image with
an arrow and a corner marker, generated for the test; observed unless
marked):

- *Decal faces:* seen from outside, an image is never mirrored. On the four
  sides its up is the part's +Y; on Top and Bottom its up is +Z (towards
  Back). It is stretched over the face.
- *Round and sloped shapes:* a Decal is projected straight along its face's
  axis onto the part's box, over the points whose normal is closest to that
  axis: a Front decal covers the front of a ball, a cylinder's curved side
  shows its Front, Top, Back and Bottom decals, and on a 45-degree wedge
  slope the Front decal wins over the Top one. The same on a SpecialMesh
  Sphere and on a MeshPart (projected onto the mesh's box).
- *Decals on file meshes:* on a SpecialMesh FileMesh, a FileMesh and the
  Head, a Decal is drawn through the mesh's own UVs over the whole mesh,
  whatever its `Face` (a Top decal on a cube mesh showed on every face). The
  head mesh's UVs map only its front quarter (projected straight on), and
  the rest of it to UV (0, 0), so a face decal's top-left pixel covers the
  back of the head: transparent for a real face, the black corner marker of
  the test image. Measured from the user's `head.mesh` for Rook's own head:
  the 100 vertices facing within 45 degrees of -Z have UV
  (0.5 - x / 2R, 0.5 - y / 2R), the other 417 (0, 0), with seams.
- *Decal properties:* `Color3` multiplies the image, `Transparency` fades
  it, and a higher `ZIndex` draws on top. With equal `ZIndex` the order
  wasn't creation order (the same image won either way round). A Decal on a
  part with Transparency 1 still shows, and on a half-transparent part it is
  drawn whole, not seen through the part. Defaults (conformance case
  `content/decal`): Face Front, Color3 white, Transparency 0, ZIndex 1;
  Texture StudsPerTile 2 × 2, offsets 0; SpecialMesh `VertexColor` 1, 1, 1.
- *Textures:* tiled from the face's top-left corner (on a 5 × 3.5 face with
  2 × 1.5 tiles, whole tiles at the top left and part tiles at the right and
  bottom); the image coordinate is (studs + `OffsetStudsU`/`V`) /
  `StudsPerTile`, so an offset of 0.5 moves the image 0.5 studs left.
- *Mesh textures:* `TextureId` draws only on file meshes (SpecialMesh
  FileMesh and FileMesh, including the empty-`MeshId` box, which shows the
  image on every face); a Head, Sphere or Brick SpecialMesh ignores it. The
  texture replaces the part's colour, `VertexColor` multiplies it (red made
  green and blue black), and its alpha is ignored. `VertexColor` on a mesh
  without a texture, and per-vertex colours in a 2.00 mesh, changed nothing.
  MeshPart `TextureID` draws the same way (a MeshPart loaded from a test
  model).
- *Version 1.00 UVs:* a cube mesh written as 1.00 and as 2.00 with the same
  UV numbers drew upside down relative to each other, so 1.00's V is flipped
  (as `rbx_mesh` does): step 3's open question is settled.
- *Setting `TextureId` or `MeshId`* (non-empty) on a SpecialMesh sets its
  `MeshType` to FileMesh, which scripts see (conformance case
  `content/decal`). Setting `Texture` on a Decal or Texture (or a Decal's
  `ColorMap`) to any lower-case `http://` or `https://` URL with a non-empty
  `id` query value stores `rbxassetid://` and that value as written, whatever
  the host or path (`example.com/foo?id=5`, `asset.aspx`), dropping
  `version`; an upper-case scheme or a leading space is kept as it is. Step
  2 had seen only the lower-case roblox.com form.
- *Loading and failure:* while a Decal's new image loads it draws nothing (it
  doesn't keep the old image); a mesh whose new `TextureId` is loading is
  drawn black; a failed or empty `TextureId` leaves the mesh in the part's
  colour. A failed Decal image draws nothing (step 2). A capture caught the
  loading state by queueing 1,200 thumbnail requests first.
- *Surfaces:* drawn only on Plastic: Studs, Inlet and Universal, and one
  diagonal lattice for both Weld and Glue, at one stud per stud, tiled from
  each face's top-left corner like a Texture. SmoothPlastic and the 2022
  materials show none. Hinge, Motor and SteppingMotor draw a small yellow
  pin instead (SmoothPlastic too). On balls, cylinders, wedge slopes and the
  Sphere and Cylinder SpecialMeshes the patterns follow the surface (more
  studs across a ball than a projection gives); a BlockMesh (at any
  `Scale`), a CylinderMesh, file meshes and MeshParts show none, while a
  Brick SpecialMesh keeps them (checked after release, when a modern core place's
  BlockMesh parts showed studs in Rook but not in Studio).
- *Where the textures live:* Studio's install holds only Plastic's material
  textures (`PlatformContent/pc/textures/plastic/diffuse.dds`, `normal.dds`,
  `normaldetail.dds`) and `PlatformContent/pc/textures/studs.dds`: a
  128 × 2048 DXT1 strip of sixteen 128-pixel tiles, four variants each of
  studs, the lattice, inlets and universal, each tile 2 × 2 studs, grey
  around 128 with lit bevels (Plastic's `diffuse.dds` has the same layout,
  flatter). The 2022 materials' textures (Wood, Brick, Grass, ...) aren't in
  the install: they appeared a few seconds after the parts were made, so
  Studio downloads them at runtime, from a source the install doesn't name.
- *Neon* draws unlit at full colour; the empty-`MeshId` box with Neon draws
  lit, without the glow. *Reflectance* tints a part towards its
  surroundings (a red part at 0.2 looked pink, at 0.5 lavender, under a blue
  sky) and still shows its studs.
- *Empty MeshPart:* a new one drew its plain box, with no checker (step 3
  had seen a checker once).

*Roblox's round primitives* (measured afterwards from Studio's OBJ export
of a Ball and a Cylinder part; the export itself is not kept): a Ball is a
cube whose six faces are each split into a 6 × 6 grid of squares, every
point pushed out onto the sphere, each face with its own vertices (294
vertices, 432 triangles); a Cylinder's round side is a square split into 6
segments per side (24 in all, at atan(1/3), atan(2/3), 45 degrees, ...)
pushed out onto the circle, with flat ends (114 vertices, 96 triangles);
normals are smooth. Their UVs map each patch linearly from the box face it
came from, 1/8 of a unit per stud, starting at each face's top-left corner
(a block's faces likewise). So a decal on a ball or cylinder covers its
face's whole patch, out to the 45-degree seams; Rook's earlier projection
straight along the face's axis reached only 85% of the way there, which
stretched the image across a CylinderMesh barrel by about 13%. Rook's Ball
and Cylinder are now built from these rules, each vertex keeping its box
point and face for images and surfaces, and the barrel's face lines up
with Studio's to a pixel.

*Compared with Studio* (`rook-player --screenshot` against Studio's
`screen_capture`, same cameras, field of view 20, one test scene run by the
same script in both, the test files given to Rook through
`ROOK_CONTENT_DIRS`): decals on every face of a block and on a ball,
cylinder, wedge, corner wedge, Sphere mesh, cube mesh and both heads; mesh
textures in 2.00 and 1.00, `VertexColor`, the empty-`MeshId` box; ZIndex,
tint, decals on invisible and half-transparent parts; Textures with
non-integer tiling and offsets; surfaces on Plastic and SmoothPlastic, and
Neon. Placement, orientation and tiling match to the pixel by eye, apart from
lighting. A 2017 disaster place, one of its maps loaded into
Workspace in both: the chairs' weld lattice and studs, the studded Plastic
and the decals match; Wood and Concrete are plain in Rook. A 2016 core place
from its saved camera: the blast-door decals, the logo, the warning signs
and the floor grate Texture match, once fetched with an Open Cloud key (see
below), and the doors' Reflectance gives Studio's pink.

*Asset delivery with an Open Cloud key*, first tried here: with
`ROOK_OPEN_CLOUD_API_KEY` set, a 2016 core place resolved 544 of its 704
distinct references (4,742 of 5,304 uses); the other 89 are refused to
anyone ("User is not authorized", "Asset is not approved for the
requester") or don't exist. A cached anonymous refusal no longer stops the
key being tried.

*Performance* (release builds, base = the previous commit, one run each):
`rook run-headless --seconds 10` gave identical output on a 2016 core place,
a disaster place and a 2008 place (15.05 against 14.98 s, 0.50 against 0.46 s, 0.45
against 0.48 s). `rook-player --bench 300`: a disaster place (3 images) and a 2008 place
(5 images) unchanged (render 1.49 against 1.53 ms, 1.71 against 1.71 ms);
a 2016 core place now draws 3,473 more entries (its decals and Textures) and
310 images, and its render grew from 3.2 to 5.6 ms. The windowed player
(`--uncapped --frame-times`, from the saved camera): a disaster place 0.32 ms median
both, a 2008 place 0.39 ms both, a 2016 core place 0.75 to 1.22 ms (1,050 to 655
FPS), no frame over 16.7 ms in any.

*Left for later:* the 2022 materials' textures (not in the install; Rook
draws them plain), Plastic's own material texture and normal maps; surface
patterns that follow round and sloped surfaces, and the Hinge and Motor
pins; the sky's sun, moon and stars (skyboxes are drawn: 920 files have a Sky, each image placed as Studio places it, measured with six labelled images; Left is at +X and Right at -X, Up's top towards +X, Down's towards -X);
SurfaceAppearance and MaterialVariant (10 files); CharacterMesh textures
(with avatars, M11); the order of decals with equal `ZIndex`;
`rbxthumb://` images.
