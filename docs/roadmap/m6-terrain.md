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
   - *Design* (2026-10-27), one per part, in the order they land:
     - *Colors and grass*: Studio's rule rather than a fit, from the
       sources the [clean-implementation policy](../design/policies.md)
       allows: the terrain and grass shaders in the install's
       `shaders_glsl3.pack` (which inputs reach a texel and a blade, and
       how), the terrain tables in Studio's settings, and Studio
       measurements in a fresh process for what those leave open, in
       both material sets. One owner for how a material's color becomes
       what's drawn, read by the ground's looks and the blades alike.
       Blades shade and cast shadows as the grass shaders say. Changes
       flow from `MaterialColors` and the lighting properties: a color
       change rebuilds the per-layer looks (23 materials by 3 facings),
       never a mesh. Cost per frame: a few shader terms per pixel and the
       blades' tiles within reach in the shadow pass; benched on the
       largest grass place against the build before.
     - *Shoreline water*: the way voxel engines keep a second fluid in
       a solid's cell (Minecraft's waterlogged blocks, Studio's own water
       occupancy per voxel beside its solid material): water gets its own
       occupancy field and its own surface, meshed from that field per
       chunk with faces against solid dropped, after measuring in Studio
       where the two surfaces meet (islands, slopes, walls, lone voxels,
       half occupancies) and how water looks from below. Changes flow
       through the same dirty-chunk list as solid terrain: a chunk's
       water is remeshed only when its voxels or a neighbor's border
       changed. Cost: water meshing on load scales with the water's
       chunks; an edit, with the chunks it touches.
     - *Edits cost only what changed*: one owner of the voxels (the
       Terrain's grid, which physics, the renderer, grass and
       replication read, as Roblox's Terrain is the one store its
       `SmoothGrid` serializes), and edits that append the chunks they
       dirty to a change list each consumer reads from its own cursor,
       as engines with chunked worlds remesh dirty chunks (Minecraft's
       dirty sections, Unity's terrain dirty regions). Colliders, meshes
       and grass tops rebuild only listed chunks; the scene list updates
       only their entries; `SmoothGrid` is encoded only when something
       reads it (saving, replication of a whole grid, a script reading
       the property), from per-chunk encodings kept until their chunk is
       dirtied. Cost: a frame with no edit costs nothing for terrain; an
       edit costs its changed voxels plus the chunks they touch, whatever
       the place's size. Benched with `--bench` on two or three terrain
       places and a script digging every frame, before and after.
     - *The rest of the Scope* (detiling per vertex, seams, height maps
       without binding arrays, the bent-quad diagonal, water frames and
       sky detail, `CountCells`, `WorldToCellPreferEmpty`, voxel
       channels, small parts at the water surface) gets its Design here
       before its code, once the parts above have landed.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 4, and no stubs row names this step. Side by side
     means every material, water from above, along its surface and at
     its shores, and grass close up and to the horizon.

Steps 1 to 4 met the old exit criterion (terrain shows, parts rest on it,
queries find it, scripts build and dig it); step 5 holds them to Studio.
