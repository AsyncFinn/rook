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
     - ✅ Colors (2026-10-28): Studio's recoloring rule, measured
       per pixel on every material in both sets, and the legacy textures
       in every place. Left: the textures filtered in gamma space as
       Studio's YCoCg array is (Rook draws them about 7 of 255 brighter
       at their default colors).
     - ✅ Shoreline water (2026-10-28): the water solid voxels hold is
       drawn and solids count as inside water's surface, so shores round
       off where the surfaces cross, as Studio's do
       ([water](../behavior/terrain/water.md#shorelines)). Left: a dry
       solid's crossing in water's surface (its own occupancy, an
       Inference).
     - ✅ Far rings (2026-10-28): squares on the ground, far surfaces
       lying where near ones do, skirts hung straight down from upward
       faces only, so no walls, seams or stray triangles show where
       rings meet ([detail](../behavior/terrain/detail.md)). Left: a
       slab thinner than a coarse voxel drops to the voxel's middle in
       that ring (Studio's thin slabs past 2,048 studs unmeasured;
       settled by the slab's side at a narrow field of view).
     - ✅ Water's look (2026-10-28): its underside drawn, its normal
       frames, reflection, sky detail, glint and distance as Studio's
       water shaders have them, and `WaterReflectance` reaching the
       shader at all ([water](../behavior/terrain/water.md#what-studios-water-shaders-say)).
       Left: the waves' shape (Studio's two diagonal sines, their
       wavelength, height and fade with distance, read off a side view
       of a long frozen strip close up) and how far the glint fades.
     - ✅ Grass (2026-10-28): blades in the ground's average, lit by
       the grass shaders' terms and casting shadows
       ([grass](../behavior/terrain/grass.md)). Left: the foot-to-tip
       light isolated rather than read off pixels.
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
     - *Shoreline water* (refined 2026-10-28 by measurement): voxel
       engines keep a second fluid in a solid's cell beside the solid
       (Minecraft's waterlogged blocks; Roblox's own `LiquidOccupancy`
       channel beside `SolidMaterial` and `SolidOccupancy`), and mesh it
       as its own surface. Studio, as observed: water's surface is
       surface nets over every non-air voxel, each crossing placed by the
       water the voxel holds, whatever its solid occupancy; solids count
       as inside it, so water meets solid with no face at all (a camera
       inside a rock wall sees none, a dry rock in a slab's top leaves no
       hole); and dry terrain away from water has no water surface (none
       under a thin dry slab). Studio's own saving is the faces it never
       makes: water against solid and water over dry land. Rook does the
       same in the chunk's water pass: a cell has a water vertex only
       when a corner holds water, a quad only when its four cells do, and
       a chunk whose voxels and border hold no water skips the pass. The
       water pass is part of the chunk's mesh, so it's remeshed through
       the same dirty-chunk list as solid terrain, only when its voxels
       or a neighbor's border change. Cost: load meshes water only in
       chunks that have some; an edit, the chunks it touches.
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
     - *Far rings* (2026-10-28, measured on a hill-and-lake place from
       the same cameras): clipmaps (Losasso and Hoppe) and CDLOD
       (Strugar) select levels by distance on the ground; Studio, as
       observed, does too (straight above, it draws the ground below in
       4-stud cells from 4,000 studs up), so the rings become squares of
       chunk columns, the same nesting and hysteresis as before. Engines
       downsampling a density field keep the iso-surface in place
       (Transvoxel's coarse cells sample the same field); Roblox's
       crossing rule puts a surface half a voxel past its solid, so a
       coarse voxel crosses where its 4-stud voxels' surface would, half
       a 4-stud voxel past the solid it holds, and Studio's far ground
       lies where its near ground does (a slab's side runs straight to
       the horizon). Skirts hang vertically as in chunked LOD (Ulrich),
       one voxel, from upward faces only: hung from undersides they rose
       through thin slabs as walls and shards (the stray triangles).
       Studio shows no morphing between levels in stills; geomorphing
       isn't added (the rings now meet without a step to blend). Cost:
       a ring move meshes whole columns of the chunks whose level
       changed, their empty chunks skipped as before.
     - *Water's look* (2026-10-28, from Studio's smooth water shaders in
       `shaders_glsl3.pack`, which say what Studio does, measured where
       they leave numbers open): engines draw water double-sided with one
       shader whose terms take the view's side into account (Unity's and
       Unreal's two-sided translucent water); Studio's water shader has
       no side of its own (its reflection weight takes `abs` of the
       view's cosine), so its underside is the same surface seen from
       below, reflecting the sky's lower half. Rook gives water its own
       transparent pass with no culling. Its normal frames are laid as
       Studio's are: 20 studs a repeat on the plane the surface faces
       most (of the 18 terrain picks), each vertex shifting them by a
       random number in tenths, blended across the triangle, the
       detiling Rook's terrain textures already do on their 4-stud
       lattice; the rippled normal is the frames' full slope, a quarter
       of it in the reflection; the reflection's weight is Studio's
       `clamp(0.78 - 2.5 |n.v|) + 0.3` times `WaterReflectance` (it fits
       the angles measured before, which Rook's fitted Fresnel matched
       within 0.06); the sky is the skybox at full detail along the
       reflection, below the horizon too; the glint is Studio's GGX term
       with the frames' red channel as roughness; and water grows opaque
       with distance as Studio's blended shader makes it, by 250 studs.
       Cost: the same draws, with the water's triangles drawn from both
       sides; one more pipeline.
     - *The rest of the Scope* (detiling per vertex, seams, height maps
       without binding arrays, the bent-quad diagonal, water's waves,
       `CountCells`, `WorldToCellPreferEmpty`, voxel
       channels, small parts at the water surface) gets its Design here
       before its code, once the parts above have landed.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 4, and no stubs row names this step. Side by side
     means every material, water from above, along its surface and at
     its shores, and grass close up and to the horizon.

Steps 1 to 4 met the old exit criterion (terrain shows, parts rest on it,
queries find it, scripts build and dig it); step 5 holds them to Studio.
