# M21 Performance and scale

1. A benchmark set: the archive's largest and heaviest places, timed in
   CI with `rook-player --bench`.
2. Level of detail and culling for huge part counts.
   - Evidence (2026-10-27, dev build, a 103k-instance place with 85,674
     parts and terrain in 10,975 meshes, holding still): the main pass
     draws 9.4M triangles in 5,445 draws, everything inside the view at
     full detail, terrain included; mesh files' own lower levels of detail
     are dropped on load ([file meshes](../behavior/meshes/file-meshes.md)).
     Culling tests every run of 16 instances in the place against the
     view each frame (1.2 to 1.3 ms of CPU recording), with no hierarchy.
     Particles step every emitter each frame, seen or not (small there).
     What scales with what changes instead, and is fine: scene updates
     (858 parts moving each frame, 2.4 ms with the particles' step), the
     light voxels ([local lights](../behavior/lighting/local-lights.md#how-rook-does-it)),
     world GUI binding ([world GUI](../behavior/gui/world.md#drawing)).
   - Before building any: what Studio draws far away (terrain's coarser
     levels, `RenderFidelity` Automatic, view distance by graphics
     quality), measured in Studio.
3. StreamingEnabled as Roblox does it, server and client.
4. Script performance where the corpus run shows it matters.
