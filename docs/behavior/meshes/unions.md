# Unions

Checked against Studio 0.742 on 2026-10-14.

A union (`UnionOperation`,
`NegateOperation`, `IntersectOperation`) draws its own geometry, decoded
by `rook_content::decode::mesh::union` into the same `Mesh` as mesh files and
loaded, uploaded and drawn through the same path (`Want::Mesh`, the GPU
mesh cache). What unions collide as (their saved hulls, since M15) is in
[collision](../physics/collision-shapes.md#unions).

*What the archive uses* (`rook-tools corpus-scan`, which now counts unions by
form; 6,245 files loaded from a larger archive than step 3's, so the
counts aren't comparable with the tables above):

| Kind | Files | Uses |
|---|---:|---:|
| UnionOperation | 1,895 | 2,037,188 |
| ... with an `AssetId` (the geometry's asset) | 1,751 | 2,020,095 |
| ... without one | 407 | 17,093 |
| ... `MeshData` in the file: empty | 1,818 | 2,030,554 |
| ... `MeshData` in the file: `CSGK` (no geometry, only an id) | 158 | 6,616 |
| ... `MeshData` in the file: `CSGMDL2` geometry | 6 | 18 |
| ... geometry in the file and no `AssetId` | 1 | 1 |
| ... `PhysicsData` `CSGK` / `CSGPHS` 3 and 5 | 582 / 1 / 2 | 446,243 / 1 / 2 |
| ... with `ChildData` | 194 | 33,263 |
| ... `UsePartColor` true | 1,617 | 1,142,650 |
| ... resized (Size isn't `InitialSize`) | 1,634 | 996,075 |
| ... under another union | 150 | 4,904 |
| NegateOperation (with an `AssetId`) | 18 (17) | 108 (106) |
| IntersectOperation | 7 | 36 |
| Decals / Textures on unions | 669 / 590 | 44,031 / 109,428 |
| SpecialMesh / BlockMesh / CylinderMesh under a union | 52 / 21 / 5 | 249 / 202 / 18 |

Newer files also save `MeshData2` (a shared string) and
`PhysicalConfigData`; the decoder reads `MeshData2` too, but the scan that
counts it was stopped, so how often it holds geometry isn't known. Rook
draws only geometry behind an `AssetId`: in-file geometry without one is a
single union in the archive.

*Which assets load* (`rook-tools content --place`, which now decodes union
assets; a sample of 3 named places and about 20 random ones, stopped
partway): every union asset that loaded anonymously holds `CSGMDL2`
geometry, from 2015-2016 places (a small place 4 of 4
distinct, a large 2016 place 49, a third large place 50). Union assets of newer
places are refused without an account (most of them
of the random sample), so their `MeshData` versions are unknown. No Open
Cloud key was available to count what a key unlocks. A union asset is a
model holding one `PartOperationAsset` with `MeshData`, `PhysicsData`
and `ChildData` (the parts it was made from).

*`MeshData` forms* (little-endian): `CSGK` and a 32-character id hold no
geometry. `CSGMDL` 2 and 4 are XORed, from the first byte, with a
repeating 31-byte key; under it are the magic and version, a 32-byte
hash, the vertex count and size (84), the vertices (position, normal,
color, the box face as 1 to 6 for Right, Top, Back, Left, Bottom, Front,
UV, then a tangent between zeros) and the index count and indices; 4 adds
a list after them. `CSGMDL` 5 is plain after its magic: positions,
normals (three u16s offset by 32,767, over 32,767), colors, box faces (a
byte each), UVs and tangents, each list after its own u16 count, then
the indices as differences from the one before (a byte for -64 to 63,
else three bytes) and range markers, of which the first two bound the
drawn triangles (Inference: the rest are levels of detail).

*The geometry* (`rook-tools content` on a small place's assets): its bounds equal the
union's `InitialSize`, centered on its origin. Each vertex carries a
color (that of the part it came from), the `NormalId` of the box face it
was cut from, and UVs that are planar and in studs from the union's
center.

*Checked in Studio 0.742* (2026-10-14, a test place of one place's unions plus
variants, opened in a fresh Studio and laid out by the same script in
Studio and in `rook-player --script`, compared at FieldOfView 20 from the
same cameras). Observed unless marked:

- *Size:* the geometry is drawn at its vertices × Size / `InitialSize`
  about the union's position, the MeshPart rule (`mesh_part_scale`): a
  Turbine resized from (4, 5, 5) to (8, 5, 2.5), and a small place's rings saved at
  sizes other than their `InitialSize`, matched Studio's outlines.
  Scripts read `InitialSize` as `MeshSize`, and a new union reads
  (1, 1, 1) (conformance case `content/union`).
- *`UsePartColor`:* off, the union draws its vertices' colors whatever
  its Color (a union set to red still drew its parts' gold); on, its
  Color.
- *No geometry:* a union whose asset isn't a union (asset 1) and one with
  neither `AssetId` nor `MeshData` both draw nothing. While the asset
  loads Rook draws nothing, as for a new mesh file (Inference: Studio
  loads too fast to catch it). Assets that need an account draw nothing
  in Rook; Studio, signed in, draws them.
- *Decals* go on the geometry cut from their face's side of the box,
  stretched over the box face (a face decal on a cylinder union covered
  its front only, not its rims). *Textures* tile from each face's center,
  the origin of the union's UVs: on a Bar resized to 12.4 × 4.5 × 4.5 the
  tiles sat a quarter stud from where corner tiling puts them, and moved
  there when Rook tiled from the center. Rook tiles material textures the
  same way (Inference).
- *Surfaces:* a Plastic union with Studs on its Front drew none.
- *Materials* show on unions (a Wood union drew wood grain over its
  faces); Studio's looks lighter and finer than Rook's (Approximation, as
  for parts in step 5).
- *NegateOperation:* draws its geometry in a fixed orange-red, whatever
  its Color and `UsePartColor` (a gold one and a blue one alike; Neon made
  it white, so its material still applies). The color, (252, 140, 90),
  is fitted against plain parts under the same light (Approximation). A
  new NegateOperation is named `NegativePart`, a new union `Union` and an
  IntersectOperation `Intersection` (conformance case).
- *Unions inside unions* each draw their own geometry where they are.
- *Mesh children* still replace the geometry with a 1-stud shape, as
  observed in step 3.

*Performance* (release builds, base = the build before this step, one `rook-player --bench
300` run each): whole-frame medians a small place 1.86 against 1.93 ms, a large 2016 place
19.0 against 18.7 ms, a third large place (182 union uses) 17.7
against 19.8 ms, where the server step, which unions don't touch, moved
by as much (8.6 to 9.6 ms): within run-to-run noise.

Left for later: unions whose assets need an account; in-file geometry
without an asset; `MeshData` versions 4 and 5 are decoded but no archive
asset Rook can fetch uses them, so they're unchecked against Studio; the
NegateOperation color's exact value. `PhysicsData` hulls came with M15
([collision](../physics/collision-shapes.md#unions)).
