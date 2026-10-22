# Terrain voxels

Checked against Studio 0.742 on 2026-10-15.

`rook-terrain` decodes `Terrain.SmoothGrid` into voxels (`Grid`), which it
meshes into the [surface](surface.md). Both were measured with `WriteVoxels`,
`FillBlock`/`FillBall` and `Workspace:Raycast`, whose hits give the
collision surface's exact positions and normals. The recordings are in
`crates/rook-terrain/data/` and replayed by its tests.

*What the archive has* (`rook-tools terrain --scan`, run once over the archive's
10,939 files, 6,378 unique, 231 s):

| Terrain | Files |
|---|---:|
| No Terrain instance | 5,682 |
| A Terrain with no voxels or legacy data | 582 |
| Smooth voxels (`SmoothGrid`) | 10 |
| Legacy blocks (`ClusterGrid`, `ClusterGridV2` or `ClusterGridV3`) | 104 |
| Both | 0 |

So 696 unique files have a Terrain (the roadmap's 977 came from an older count),
and very few hold voxels. The largest smooth ones hold 25.2 million
voxels (99% of them Water), 1.18 million (Grass with
Concrete, Brick and WoodPlanks), 751,000 (Rock and
Snow) and 75,000 (Snow and Glacier); two hold water only. Over all ten, Water is 93% of voxels, then Grass, Rock and Snow. The
per-material totals were counted before Rook's decoder kept voxels with
occupancy byte 0 (below), so they are a few hundred voxels short; the file
counts are unaffected. The legacy files are mostly 2008 to 2012 places.

*Legacy terrain in modern Studio* (Confirmed, Studio 0.742, three places
opened from a copy): Studio loads **no voxels** from legacy terrain. Two
places saved with `ClusterGridV2` log "Legacy terrain data is malformed,
voxels will not be loaded: Error while decoding data: unexpected end at
offset 407" (and "offset 16"), and one with `ClusterGrid` (XML) loads
with no message. In all three `Terrain:CountCells()` is 0 and `ReadVoxels` finds
no voxel between -2048 and 2048 studs. Rook does the same: it reads only
`SmoothGrid`. (The legacy XML upgrade still restores `ClusterGrid`'s
control characters, so the file round-trips.)

*`SmoothGrid`* (Confirmed): the layout in checkraisefold's notes for
rbx-dom (pull request 444, not merged) holds, with one correction to the
occupancy. A version byte (1) and the chunk size's log2 (5), then chunks
of 32³ voxels, each after its offset from the previous chunk in chunks
(three big-endian `i32`s, bytes interleaved), its voxels Y slowest and X
fastest in runs: a flag byte (material index, and bits for an occupancy
byte and a count byte), the occupancy, and the count (0 instead says one
voxel whose shoreline water occupancy follows). A missing occupancy is full.
Occupancy byte `b` is `(b + 1) / 256` full: a place built from known bytes
reads back in `ReadVoxels` as exactly that, so byte 255 is full and byte 0
is 1/256 full (a voxel, not air). Rook decodes the 75,000-voxel place exactly as
Studio reads it: Snow 60,059, Glacier 15,194 and Grass 98 voxels, with
the same summed occupancy (55,756.99, 14,122.13, 85.02). Voxel `v` covers
studs `4v` to `4v + 4`, and chunk `c` starts at `128c`. A damaged grid
(truncated, an unknown material, a run past its chunk) loads no terrain
and prints a warning.
