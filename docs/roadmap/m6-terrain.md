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

5. **Match Studio**: close every difference left in what steps 1 to 4
   built. Found 2026-10-27, side by side with Studio on a large terrain
   place: Rook's grass ground reads gray-olive where Studio's is
   saturated, lava draws pink, water is more saturated and its shoreline
   steps with the voxels where Studio's rounds off, and grass blades are
   flat with no darkening under them.
   - *Scope*:
     - Colors: CrackedLava's tint (the fitted matrix turns its default
       (255, 24, 67) pink, blue above green); the tint rule read rather
       than fitted; the default Grass color against Studio.
     - Water: shoreline water inside solid voxels drawn, so shores round
       off; water seen from below; how Studio lays its water frames and
       the sky detail its water reflects.
     - Grass: blades shaded as Studio's (each blade's shade varies and
       the ground darkens under a clump,
       [grass](../behavior/terrain/grass.md)).
     - Surface: the per-vertex detiling numbers; seams as wide as
       Studio's; height maps without binding arrays; Roblox's diagonal
       for bent quads.
     - API: `CountCells`, `WorldToCellPreferEmpty` inside solid terrain,
       `ReadVoxelChannels` and `WriteVoxelChannels`; fast small parts
       against the water surface.
     - Edits costing only what they change (today the scene list is
       rebuilt and the whole `SmoothGrid` encoded and decoded).
     - Out: buoyancy (M15 step 3); terrain lit as Studio lights it (M13
       step 3).
   - *Evidence*: the [stubs](../guide/stubs.md) rows taken over by this
     step, and the open Approximations and Inferences
     on the terrain pages.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 4, and no stubs row names this step. Side by side
     means every material, water from above, along its surface and at
     its shores, and grass close up and to the horizon.

Steps 1 to 4 met the old exit criterion (terrain shows, parts rest on it,
queries find it, scripts build and dig it); step 5 holds them to Studio.
