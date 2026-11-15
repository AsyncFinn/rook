# M13 Lighting and post-processing

Make places look the way Studio shows them today, at both ends of its
graphics quality: the default (`EditQualityLevel` Automatic, what
archive places are mostly seen at) and the top (Level21, with
`PrioritizeLightingQuality` on). Post effects in the corpus: Bloom 49
files, ColorCorrection 44, SunRays 20, Blur 13, DepthOfField 11;
Atmosphere 16, Clouds 8.

Rook has no quality setting today and draws Automatic everywhere
([user settings](../behavior/user-settings.md): Graphics Quality is
saved with nothing behind it). Step 4 adds it; steps 1 to 3 measure
what they build at both Automatic and Level21, so nothing has to be
measured twice.

1. **The sun, moon and stars**, `SkyboxOrientation`, Atmosphere and
   Clouds.
   - *Scope*: the night sky Rook draws black today (Studio's dark
     gradient, its purple horizon glow and the stars,
     [sky](../behavior/lighting/sky.md#not-done)); `CelestialBodiesShown`,
     `StarCount`, `SunTextureId`, `MoonTextureId`, `SunAngularSize`,
     `MoonAngularSize`; Atmosphere's `Density`, `Offset`, `Color`,
     `Decay`, `Glare` and `Haze` over parts, terrain and the sky; Clouds'
     `Cover`, `Density` and `Color` and their drift with `GlobalWind`.
     Out: fog's existing rules ([tone mapping](../behavior/lighting/tone-mapping.md#fog)).
   - *Studio questions*: whether the sun disc, the stars, Atmosphere and
     Clouds draw differently at Level21 than at Automatic (detail, steps
     through the haze, cloud shadows on the ground); how Atmosphere and
     the old fog combine when a place has both.
   - *Done when*: day, dusk and night captures of the Baseplate, with and
     without an Atmosphere and Clouds, match Studio at Automatic and at
     Level21 by the [Match Studio](README.md#match-studio) bar.
2. **Post-processing** on M7's HDR target: Bloom, ColorCorrection,
   SunRays, Blur, DepthOfField, and Neon's glow.
   - *Scope*: each effect's properties and `Enabled`; several of one kind
     at once (whether they stack or one wins); effects under the Camera
     as well as under Lighting; the order they apply in against tone
     mapping. Neon glows at high quality levels
     ([antialiasing](../behavior/rendering/antialiasing.md) notes it
     blurs edges there), so whether Neon's glow is Bloom or its own pass
     belongs here.
   - *Studio questions*: which effects change or turn off by quality
     level (DepthOfField and SunRays in particular), and the level each
     one starts at; Neon's glow with no Bloom in Lighting, at Automatic
     and at Level21.
   - *Done when*: one capture per effect at a few settings each, at
     Automatic and Level21, matches Studio.
3. **Studio's one lighting system** (`Lighting.Technology` is gone):
   `LightingStyle`, `PrioritizeLightingQuality`, `ShadowSoftness`,
   `EnvironmentDiffuseScale` and `EnvironmentSpecularScale` for M4's
   PBR materials.
   - *Scope*: sun shadows both ways
     ([sun](../behavior/lighting/sun.md#shadows)): with
     `PrioritizeLightingQuality` on, Studio's sharper edges and reach of
     about 580 studs where Rook's one 2048² cascade reaches 300; with it
     off, the light voxels' blocker fit (a 4-stud slab 20 studs up
     darkens about 133 levels in Rook against 50 in Studio) and parts'
     real shapes in the voxels. Also the sky light blocked by parts'
     shapes instead of their boxes, the indoor reflection cube and the
     reflections' fade with distance, and terrain lit as Studio lights
     it (the stubs rows naming this step).
   - *Studio questions*: how shadow reach, cascades, edge sharpness and
     the light voxel grid change with the quality level; whether
     `PrioritizeLightingQuality` does anything at low levels; what
     `LightingStyle` Realistic changes against Soft.
   - *Done when*: reference places and the sun shadow test scenes match
     Studio with `PrioritizeLightingQuality` off at Automatic and on at
     Level21, and no stubs row names this step.
4. **Graphics quality**: a quality setting that follows Studio's
   levels, set from the in-game Graphics Quality slider, and everything
   it switches.
   - *Scope*: the levels from Automatic to Level21 and the slider's 1 to
     10 onto them. What is known to switch: antialiasing (none below
     Level17, 2× MSAA at Level17, 4× from Level18,
     [antialiasing](../behavior/rendering/antialiasing.md#for-rook));
     water's screen-space reflections from Level16
     ([water](../behavior/terrain/water.md#reflections-at-high-quality));
     Glass's full shader at Level21, with refraction and a sharp round
     glint ([materials](../behavior/rendering/materials.md)). Plus
     whatever steps 1 to 3 find switching. Out: view distance and how far
     small parts are culled at lower quality
     ([file meshes](../behavior/meshes/file-meshes.md); M21 step 2).
   - *Studio questions*: how the slider's 1 to 10 map onto the levels;
     levels 11 to 15 and 20, assumed from their neighbors; what else
     changes between Level16 and Level21 (texture and shadow
     resolution, particle counts, grass reach), by side-by-side
     captures of one scene at each level.
   - *Done when*: the Baseplate test scenes and reference places match
     Studio at Automatic, Level10, Level16 and Level21, and the slider
     moves Rook between them while playing, without reloading the place.
5. **Match Studio at Level21**: close every difference left in what
   steps 1 to 4 built at the top quality level.
   - *Scope*: the Approximations in the lighting, sky, materials and
     water pages that only show at high quality; reflections, shadows
     and post effects together in one frame (their order and how they
     interact).
   - *Evidence*: the [stubs](../guide/stubs.md) rows naming this
     milestone, and open Approximations and Inferences on those pages.
   - *Done when*: screenshots of reference places at Level21 with
     `PrioritizeLightingQuality` on match Studio's by the
     [Match Studio](README.md#match-studio) bar, as they do at
     Automatic, and no stubs row names this milestone.
