# DoubleSided

Checked against Studio 0.746 on 2026-11-06, with test meshes (a square of
two triangles and a sphere) in a blank place, ClockTime 14,
`PrioritizeLightingQuality` on unless noted, `EditQualityLevel` Automatic.
Rook's side is `PartInstance::double_sided`; reproduce it with a `MeshPart`
square seen from behind.

## Which classes have it

From the reflection database (`crates/rook-reflection/data`) and Studio:

| Class | Property | Drawn? |
|---|---|---|
| `MeshPart` | `DoubleSided`: read and write, saved, false by default | Yes, this page |
| `MeshImportData` | `DoubleSided`: an importer setting | No |
| `BoxHandleAdornment`, `ConeHandleAdornment`, `CylinderHandleAdornment`, `ImageHandleAdornment`, `PyramidHandleAdornment`, `SphereHandleAdornment` | Hidden `DoubleSided`, RobloxScriptSecurity: a plugin's read and write both fail with "lacking capability RobloxScript" | Not reachable from places' scripts; Rook ignores it |

`SpecialMesh`, `FileMesh` and `PartOperation` have no such property, and
Rook reads it only on a MeshPart.

## What it does

| Case | Without | With |
|---|---|---|
| A mesh's back faces | Not drawn: the sky shows through a square from behind | Drawn |
| Their light | | As if they faced the viewer: the back of a double-sided square looks the same as a one-sided square turned 180° (`gl_FrontFacing` flips the normal in `shaders_glsl3.pack`) |
| Sun shadows | Only faces towards the sun cast: a square facing down casts none from an overhead sun | Both sides cast |
| `PrioritizeLightingQuality` off | A thin square casts nothing through the 4-stud voxels | The same |
| `Transparency` | | Both sides draw in one pass, in the mesh's index order, with no back-then-front: a half see-through sphere looks different turned 180°, and Rook's matches each way |
| Glass, front | | The dark filter over what's behind, and the surface |
| Glass, back | | The faint surface alone: the filter stays on front faces |
| Wood, Neon, `Reflectance`, Highlight fill, SurfaceAppearance | | Draw on the back too |
| Decals and Textures | | Stay on the front: a `Front` Decal on a square whose front is its +Z side doesn't show from behind |

Rook builds the surface pipelines twice, culling back faces and culling
nothing, and a batch picks one by `double_sided`. The Glass filter and the
image layers have only the culled one.
