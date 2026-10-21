# M6 Terrain

977 files have a Terrain instance. First count how many hold voxels
(`SmoothGrid`) or legacy `ClusterGrid` blocks, and check what modern
Roblox does with the legacy data before deciding how Rook handles it.
Terrain comes before characters, who walk on it.

1. ✅ Decode `SmoothGrid` voxels (material and occupancy) and mesh them as
   Roblox does, measured against Studio, in the new `rook-terrain` crate;
   drawn in a flat color per material. Only 10 unique archive files hold
   voxels and 104 legacy blocks, which today's Studio doesn't load
   either. Counts and findings: [voxels](../behavior/terrain/voxels.md),
   [surface](../behavior/terrain/surface.md).
2. ✅ Terrain materials through M4's material system: `MaterialColors`,
   MaterialService overrides and MaterialVariants on terrain, and water's
   surface, from Studio's own terrain material table; then grass blades
   (`Decoration`) and water waves (swimming comes with characters).
   Findings: [terrain.md](../behavior/terrain/materials.md),
   [grass and waves](../behavior/terrain/grass.md).
3. ✅ Collision and queries: parts land on and rest on terrain's smooth
   surface, and rays, shape casts, `GetTouchingParts` and
   `IsRegion3Empty` find it as Studio does (bounds queries don't);
   colliders are built per chunk when something comes near. Findings:
   [terrain.md](../behavior/terrain/collision.md).
4. ✅ The Terrain API scripts use: the fills, regions, `ReadVoxels` and
   `WriteVoxels`, `CopyRegion`/`PasteRegion`, material colors and the
   legacy cell methods modern Roblox still honors, each measured in
   Studio (conformance case `terrain/terrain_api`). Edits reach collision
   and `SmoothGrid` at the end of the frame, and only changed chunks are
   rebuilt. Findings: [terrain.md](../behavior/terrain/api.md).

M6's exit criterion is met: archived places with terrain show it, parts
rest on it and queries find it, and scripts can build and dig it.
