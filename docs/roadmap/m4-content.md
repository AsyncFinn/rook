# M4 Content: meshes, materials, textures, decals

The most visible gap: **SpecialMesh is in 65% of files** (1,988 files,
494k instances), CylinderMesh in 723 and BlockMesh in 574. Today the
renderer draws all of these as their part's plain box.

1. ✅ **Built-in mesh shapes**, no downloads needed: SpecialMesh's `MeshType`
   shapes, BlockMesh and CylinderMesh, with `Scale` and `Offset`
   (`VertexColor` waits for textures). Every type but Head reuses a shape
   Rook already has (see [meshes](../behavior/meshes/built-in-shapes.md)); the size rules are defined once
   in `rook-types` (`MeshKind`, `MeshLook`). Head is a mesh file in Roblox,
   so Rook draws its own head mesh, built in `rook-render` from
   measurements of the user's copy, which is never committed (see the [content
   policy](../design/policies.md#content-and-assets)); from step 2 on, the user's install can supply the
   real file instead.
2. ✅ **Asset fetching**: `rbxasset://`, `rbxassetid://` and legacy asset
   URLs through the provider chain (Rook defaults, the user's Roblox
   install, content folders, asset delivery), cached and hash-checked, in
   the new `rook-content` crate; `ContentProvider:PreloadAsync`; `rook
   content`. Findings and corpus counts: [content.md](../behavior/content/asset-fetching.md).
3. ✅ **File meshes**: SpecialMesh `MeshId`, FileMesh and MeshPart, Roblox
   mesh versions 1.00–5.00, loaded in the background and drawn at Studio's
   sizes. Counts and findings: [meshes.md](../behavior/meshes/file-meshes.md).
4. ✅ **Textures and decals**: images decoded in the background (PNG, JPEG,
   BMP, DDS), Decals and Textures on every part shape and mesh, mesh
   `TextureId` and MeshPart `TextureID` with `VertexColor`, Plastic's surface
   patterns, Neon and Reflectance, drawn as Studio draws them. Counts and
   findings: [textures-decals.md](../behavior/rendering/textures-and-decals.md).
5. ✅ **Materials and PBR**: the material system modern Roblox has, which
   later work (terrain, avatars, lighting) builds on. Today only Plastic
   (its surface patterns), Neon and Reflectance are drawn; every other
   material is its plain color.
   - *PBR shading* in the renderer: color, normal, roughness and
     metalness maps, lit by the sun and ambient and reflecting the sky.
     Every material below is drawn through it.
   - *MaterialService* (62 files): `Use2022Materials`, which picks between
     the 2022 materials and the legacy set (modern Roblox still offers
     both, so Rook draws both), and its per-material overrides. Measure in
     Studio which set a place without a MaterialService gets.
   - *The built-in materials*, both sets, by files using them: Wood 531,
     Slate 502, Concrete 366, Grass 358, Ice 353, DiamondPlate 351,
     CorrodedMetal 252, SmoothPlastic 201, Foil 177, Brick 158, Sand 149,
     Granite 127, Marble 125, Metal 110 (703,662 parts), Pebble 105, Fabric
     101, WoodPlanks 51, Cobblestone 39, Glass 23, ForceField 18, and the
     rest. First find where Studio keeps the material textures it
     downloads at runtime (they aren't in the install), so Rook can load
     the user's copy the way it loads `studs.dds`; without them Rook draws
     its own, never Roblox's files committed. Measure each: texture scale in
     studs, how Color tints it, how much it reflects, Glass and ForceField,
     and which materials show surfaces (so far only Plastic does).
   - *MaterialVariant* (10 files, 746 variants): custom materials from
     color, normal, roughness and metalness maps, `StudsPerTile` and
     `Pattern`, applied by a part's `MaterialVariant` or as a
     MaterialService override.
   - *SurfaceAppearance* (13 files, 1,646 uses): PBR maps on MeshParts,
     with `AlphaMode` (Overlay, Transparency, TintMask) and `Color`.

   Findings: [materials.md](../behavior/rendering/materials.md).
6. ✅ **Unions**: draw UnionOperations with their own geometry instead of
   their bounding box (`UnionOperation.Content`, the union's geometry
   asset, is set on 70,944 unions in 99 files; `NegateOperation` 6 in 2).
   The `rbx_mesh` crate Rook already uses decodes union geometry too: its
   `union_graphics` module reads the `MeshData` a union's asset holds
   (versions 2, 4 and 5, plus the old `CSGK` stub), and `union_physics`
   reads `PhysicsData` (versions 3 to 8; version 8 needs zstd, for which
   the crate offers the pure-Rust `ruzstd`). Turning on those features
   adds no new dependency. First count which forms the archive uses
   (geometry in the file or behind an asset id, and whether those assets
   load anonymously), then decode through `rook_content::decode::mesh` and draw
   through step 3's mesh loader and GPU cache. Check in Studio how a
   union's geometry is sized against its Size (unions can be resized),
   `UsePartColor`, and what a union draws while loading or after failing.
   Unions' collision hulls from `PhysicsData` go with MeshPart collision
   fidelity (M15). Counts and findings: [meshes.md](../behavior/meshes/unions.md).
7. **Match Studio**: close every difference left in what steps 1 to 6
   built.
   - *Scope*:
     - Materials: Studio's own Wang tile picks (`wangIndex.dds`), a
       variant's emissive mask, ForceField's moving pattern and lit
       edges, Plastic's own texture and 2022 normal map.
     - Surfaces: studs on balls, cylinders and wedge slopes as Studio
       counts them; Hinge, Motor and SteppingMotor pins.
     - Decals: the order of equal `ZIndex`; whether large images are
       halved.
     - Meshes: versions 6.00 and 7.00; the Head's cap UVs; per-vertex
       colors; material looks on file meshes; meshes under unions left
       untinted; a truss's mesh child cut with holes; corner wedge
       orientation checked.
     - Unions: geometry saved in the file; `MeshData` 4 and 5 checked;
       the NegateOperation color; material tiling.
     - Content: assets that need an account, `rbxthumb://` and version
       ids, old `rbxasset://` paths; `PreloadAsync`'s `Loading`,
       `TimedOut` and timing; `RequestQueueSize`; the other
       ContentProvider members; `Changed` firing for the legacy content
       names.
     - Out: the indoor reflection cube, the reflections' fade and sky
       light blocked by real shapes (M13 step 3); collision shapes
       (M15).
   - *Evidence*: the [stubs](../guide/stubs.md) rows taken over by this
     step, and the open Approximations and Inferences
     on the content, meshes and rendering pages.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 6 (correct, edge cases measured,
     indistinguishable side by side), and no stubs row names this
     step.

Physics picks up the real shapes of MeshParts and unions in M15 (unions
done: [collision.md](../behavior/physics/collision-shapes.md#unions)).
