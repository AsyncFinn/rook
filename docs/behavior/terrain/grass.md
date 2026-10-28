# Grass

Checked against Studio 0.743 on 2026-10-28.

With `Terrain.Decoration` on, Studio grows animated grass blades on
Grass's tops (scripts can't change the property). Of the archive's ten
smooth-terrain places only one has it on.

Measured on the Baseplate place with a 64 × 4 × 64 Grass slab and a
64 × 4 × 12 Grass strip, each beside a ruler part,
screenshots taken with narrow fields of view (2 to 8 degrees) so the
ruler gives studs per pixel, and compared by their pixels (coverage,
autocorrelation, FFT matching). Rook's renders came from a copy of the
same place holding the same voxels, from the same cameras.

*What Studio's settings say*: nothing about the blades' size.
`StudioAppSettings.json` has only switches
(`FFlagRenderGrassUseBarycentricPlantingPoints`, `FFlagGrassGPUDensityCull`,
`FIntGrassLengthCustomization_FRMDensityScaling_UpperBound` 6), and Grass's
average texture color in `FStringTerrainColorTable` (`avgAlbedo2022`:
(196, 196, 189), tint mask alpha 241). `Terrain.GrassLength` isn't a
member in Studio 0.742.

## Blades

Observed:

- On Grass's tops, with `Decoration` on (the Baseplate has it on). Off,
  Rook draws none, as the property's documentation says; Studio wasn't
  checked with it off, since scripts can't change it (Inference).
  LeafyGrass grows none in Rook (Inference: the property is for Grass).
- **Shape**: each blade is one flat triangle with straight sides, about
  half a stud wide at its foot, seen magnified at 230 px per stud.
- **Size and density**: side on, how much of the view blades cover at
  each height (0.97 of it up to half a stud, 0.68 at 1.1 to 1.5 studs,
  0.22 at 2 to 2.6, 0.06 at 2.6 to 3.1, nothing above 3.6), and from
  above (half the ground), fit about 2.3 blades per square stud, 0.72 to
  3.6 studs tall, leaning every way up to about 50 degrees. Still as dense
  100 studs away, sparser but drawn 200 studs away.
- **Color** (2026-10-28, Studio 0.743, legacy place, a Grass slab seen
  side on at blade height beside gray SmoothPlastic parts): under
  ambient light alone every blade draws one flat color, the ground's
  average: (80, 86, 27) in Grass's legacy default color, a gray 146 in
  white (the ground's 146), and green 74 and blue 152 in red (the
  ground's 78 and 152; red was past the brightest gray part), so blades
  take Grass's
  [recoloring](materials.md#colors). In the default color their blue
  came out 12 under the ground's (Approximation).
- **Light** (Studio's grass shaders in `shaders_glsl3.pack`: `GrassVS...`
  and `GrassFS...`, 0.743): one per-frame color (`grassColor0`) times the
  ambient light, plus the sun on the blade by (|n·l| + 0.9) / 1.9 (so a
  blade edge-on to the sun keeps about half), times a per-vertex
  exp2((−foot × a + b) × c) from three uniforms the shaders don't fix,
  and shadowed. Their high quality variant adds light through a blade
  seen against the sun, (|n·l| / 2 + 0.5)³ times the view's cosine to the
  sun, and a highlight, pow(|h·n|, 12) × pow(h.y, 8), both fading over
  the last 40 studs of the grass's reach. Shadow shaders
  (`GrassShadowVS_`, `GrassShadowPS_`) draw blades into the sun's shadow
  map. Observed under the sun alone (`Brightness` 2, noon): blades in a
  clump draw in different shades by how they face the sun, each one
  shade across, its foot darker than its tip (about half as lit along
  a blade, read off its pixels: Approximation for the uniforms). This
  settles the user's 2026-10-27 close-up, where shades varied and the
  ground under a clump was darker: facing and shadows.
- **Culling** (the same shaders): each draw of blades has one density
  (a uniform), and a blade whose hash of its index is above it puts all
  its vertices at one point, so it's never rasterized, in the frame and
  in the shadow map alike (`FFlagGrassGPUDensityCull` in Studio's
  settings); blades also sink into the ground by their distance from
  the camera. How the density falls with distance isn't in the shaders
  (observed: as dense at 100 studs, sparser at 200; Approximation).
  The vertex shaders also pick a color per blade from a 64-entry table
  (`CB3`), but no GLSL grass fragment shader reads it (Unknown whether
  another API's does; the shades seen are explained without it).
- **Sway** (no wind): tips 3 studs up swing about 0.1 studs either way
  over 2.5 to 4 seconds; blades under a stud apart move together, 1.3
  studs apart opposite, as if a wave crosses the grass.
- **Wind**: `Workspace.GlobalWind` turns the blades about their feet
  towards where it blows, their tips dropping: about 12 degrees at 5
  studs per second, 45 at 20 and 50 at 60.

Rook (`rook_terrain::grass`, `shaders/terrain.wgsl`): blades at random
points on Grass's upward triangles, fixed by each triangle's position so
the same ground grows the same blades, as above; each one triangle seen
from both sides, in Grass's top texture's average recolored as the ground
is (`blade_color`), lit as Studio's shaders light them (`blade_surface`,
the foot taking half the tip's sun), and casting into the shadow map
where they're drawn (the shadow pass sways and sinks them as the frame
does), and culled as Studio culls them: one density per 32-stud tile
from its distance (full out to 60 studs, 0.4 at the edge) against each
blade's number, a culled blade collapsed to a point in both passes
(`blade_drawn`). Measured on the same views, Rook covers
within 0.04 of Studio at every height side on, and 0.43 of the ground
from above against 0.50. The player builds blades in 32-stud tiles
within 200 studs of the camera and drops a tile's mesh when the camera
leaves; in the shader they sway (a wave of 2.6 studs crossing in 3
seconds, 0.035 studs per stud of height), turn into the wind by 50 ×
tanh(speed / 15) degrees, and sink into the ground over the last
quarter of the reach. Approximation: the
planting points and sway are Rook's own, fitted to the measurements
(Studio's vertex shaders sway by value noise on the blade's position,
from uniforms they don't fix).

*Costs* (release build, `--bench 300` on the largest terrain place, 1280 × 720,
blades and [waves](water.md#waves) against the build before them, five
runs each from the grass and two from the saved camera): no difference
beyond the noise (medians 4.5 to 9.9 ms for both, other builds running
alongside). There are 2.3 times as many blades as first built but each is
2 triangles instead of 6, and water takes a second texture sample.
 Blades lit by
Studio's terms and casting into the shadow map, culled per tile as
Studio culls per draw (2026-10-28, `--bench 300` on the Baseplate with a
1024-stud Grass field, the camera at blade height, shadow map on,
alternating with the build before): the shadow pass from 0.13 to 0.28
ms, the main pass unchanged at 0.35. Casting every blade without the
cull took the shadow pass from 2.8 to 10.4 ms on a loaded machine.