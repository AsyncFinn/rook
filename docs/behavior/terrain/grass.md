# Grass

Checked against Studio 0.742 on 2026-10-15.

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
- **Color**: one flat color per blade whatever it faces and on slopes,
  the ground's average color around it: by luminance 0.78 of it in the
  default color, 0.97 with Grass red. Black Grass grows black blades,
  so there's no light added of their own.
- **Sway** (no wind): tips 3 studs up swing about 0.1 studs either way
  over 2.5 to 4 seconds; blades under a stud apart move together, 1.3
  studs apart opposite, as if a wave crosses the grass.
- **Wind**: `Workspace.GlobalWind` turns the blades about their feet
  towards where it blows, their tips dropping: about 12 degrees at 5
  studs per second, 45 at 20 and 50 at 60.

Rook (`rook_terrain::grass`, `shaders/terrain.wgsl`): blades at random
points on Grass's upward triangles, fixed by each triangle's position so
the same ground grows the same blades, as above; each one triangle seen
from both sides, lit with an upward normal, in `avgAlbedo2022` tinted as
the ground is ([Colors](materials.md#colors)) at 0.7 of it in linear light (which
came out 0.82 of Rook's own ground). Measured on the same views, Rook covers
within 0.04 of Studio at every height side on, and 0.43 of the ground
from above against 0.50. The player builds blades in 32-stud tiles
within 200 studs of the camera and drops a tile's mesh when the camera
leaves; in the shader they sway (a wave of 2.6 studs crossing in 3
seconds, 0.035 studs per stud of height), turn into the wind by 50 ×
tanh(speed / 15) degrees, thin out from 60 studs away to 40% at the
edge, and sink into the ground over the last quarter. They cast no
shadow (Inference). Approximation: the planting points and sway are
Rook's own, fitted to the measurements.

*Costs* (release build, `--bench 300` on the largest terrain place, 1280 × 720,
blades and [waves](water.md#waves) against the build before them, five
runs each from the grass and two from the saved camera): no difference
beyond the noise (medians 4.5 to 9.9 ms for both, other builds running
alongside). There are 2.3 times as many blades as first built but each is
2 triangles instead of 6, and water takes a second texture sample.
