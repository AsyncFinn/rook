# M21 Performance and scale

1. A benchmark set: the archive's largest and heaviest places, timed in
   CI with `rook-player --bench`.
2. Level of detail and culling for huge part counts.
   - Evidence (2026-10-27, dev build, a 103k-instance place with 85,674
     parts and terrain in 10,975 meshes, holding still): the main pass
     draws 9.4M triangles in 5,445 draws, everything inside the view at
     full detail, terrain included. Mesh files now draw their own lower
     levels where Studio does ([file meshes](../behavior/meshes/file-meshes.md#levels-of-detail),
     2026-10-27), but that place's 136 mesh files are versions 1.00 to
     2.00 with one level each: its triangles are terrain and unions.
     Culling tests every run of 16 instances in the place against the
     view each frame (1.2 to 1.3 ms of CPU recording), with no hierarchy.
     Particles step every emitter each frame, seen or not (small there).
     What scales with what changes instead, and is fine: scene updates
     (858 parts moving each frame, 2.4 ms with the particles' step), the
     light voxels ([local lights](../behavior/lighting/local-lights.md#how-rook-does-it)),
     world GUI binding ([world GUI](../behavior/gui/world.md#drawing)).
   - Before building any: what Studio draws far away (view distance by
     graphics quality, small parts culled at a distance), measured in
     Studio. `RenderFidelity` is measured (file
     meshes, above); the levels Roblox makes for old MeshPart meshes
     aren't fetched yet.
   - Terrain (built 2026-10-27): far terrain is drawn from coarser voxels
     in rings around the camera, as Studio does, and crossing a ring
     meshes only the chunks whose level changed, off the frame; the same
     place draws 4.3M triangles in 782 draws holding still
     ([terrain level of detail](../behavior/terrain/detail.md)). Left:
     how far Studio's rings reach at lower graphics quality.
3. StreamingEnabled as Roblox does it, server and client.
4. Script performance where the corpus run shows it matters.
