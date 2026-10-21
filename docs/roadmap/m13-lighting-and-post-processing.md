# M13 Lighting and post-processing

Make archived places look the way Studio shows them today.

1. The sun, moon and stars, `SkyboxOrientation`, Atmosphere (16 files)
   and Clouds (8).
2. Post-processing on M7's HDR target: Bloom (49 files), ColorCorrection
   (44), SunRays (20), Blur (13), DepthOfField (11).
3. `Lighting.Technology` (Voxel, ShadowMap, Future, and Compatibility for
   old places), `EnvironmentDiffuseScale` and `EnvironmentSpecularScale`
   for M4's PBR materials, tuned against Studio screenshots of reference
   places (it's an Approximation until then).
4. Antialiasing (MSAA by quality level, measured in
   [`docs/behavior/rendering/antialiasing.md`](../behavior/rendering/antialiasing.md)).
