# Materials

What Rook found while building M4 step 5: PBR shading, MaterialService, the built-in materials, MaterialVariant and SurfaceAppearance, observed in Studio. The plan itself is in [ROADMAP.md](../../ROADMAP.md#m4-content-meshes-materials-textures-decals); what's still partial is in [stubs.md](stubs.md).

Unless marked, everything here was observed in Studio 0.742 (version 0.742.0.7421053) on 2026-10-14, with test scenes built by the same Luau script in Studio (MCP `execute_luau`) and in Rook (`rook-player --script`), compared with `screen_capture` and `rook-player --screenshot` at the same camera and a narrow field of view.

## Where Studio keeps the material textures

The install holds only Plastic's textures (`PlatformContent/pc/textures/plastic/`) and the surface patterns (`studs.dds`). Every other material's textures are downloaded when a part first shows the material and kept in Studio's content cache (Confirmed by reading the cache before and after making one part of each material):

- **The cache** is `%LOCALAPPDATA%\Roblox\rbx-storage.db`, an SQLite database with one table, `files (id BLOB, content BLOB, size, hits, atime, category, score, ttl)`. Small entries are stored in `content`; larger ones have `content` NULL and live in `%LOCALAPPDATA%\Roblox\rbx-storage\<first two hex digits of id>\<id as hex>`. Studio and the player share it.
- **Each entry** is `RBXH`, a little-endian u32 version (2), a u32 URL length, the URL, then the response status, headers and body. Row ids aren't a plain MD5, SHA-1, SHA-256 or BLAKE2 of the URL.
- **Material textures** are TexturePack assets (`roblox-assettypeid: 63`): Studio asks `contentdelivery.roblox.com/v1/bytes/<hash>`, which redirects (302, target not cached) to a body cached under `https://cN.rbxcdn.com/<hash>` or under an entry with no URL. Both URLs need Roblox's signed tokens (a plain request is refused with 403), so Rook reads the cache and never fetches them.
- **The bodies are KTX2** files, Zstandard-supercompressed (scheme 2), in BC1 (`vkFormat` 131) or BC3 (137). Each texture is split into up to four mip "packs" (`pack_0` the 32 or 64 px levels down to 1 px, then 256, 512 and 1024 px), each a separate file; Studio fetches the larger packs only when the camera comes close, so a cache may hold only the small ones. Every pack's key-value data names the whole texture (`contentHash`), its colour space (`Gamma2RGB` or `Linear`), its original size (`RobloxOriginalWidth`, 512 or 1024) and its average channels.
- **Two sets of three textures per material**, the legacy set and the 2022 set (see the next section), identified by giving each material its own 20-second window in a fresh Studio (the cache writes bodies in batches every 15 seconds) on a place of each set, and confirmed for 29 of 38 legacy materials by correlating the normal map with the colour map's gradients (0.19 to 0.83; the rest, smooth or noisy materials, score near 0), and the same way in the 2022 set where a window caught two normal maps:
  - *colour* (sRGB): greyscale in the legacy set, so that the part's Color tints it; in colour in the 2022 set (pale warm bricks, blue-white marble), its alpha marking the bricks against the mortar (unused by Rook; Inference that Studio uses it for its own purposes);
  - *normal* (linear), stored DXT5nm-style: X in alpha, Y in green, red 255 and blue 0;
  - *roughness-metalness* (linear): red is metalness (Foil 255, Metal 202, DiamondPlate 168, CorrodedMetal 134, most others 0), green roughness (Glass 29, Ice 45, Brick 246), alpha unknown (often 255; height or occlusion, Inference).

  Slate's window caught two normal maps; 0.5.0 took the one whose gradients followed the colour map slightly better (`bd73df16`), but Studio's Slate shows the fine layered relief of the other (`ff16ea9c`), which 0.5.1 uses.
  Plastic, SmoothPlastic, Neon and ForceField fetch nothing in the legacy set; in the 2022 set Plastic fetches a normal map (not drawn by Rook: Approximation; Studio's table gives SmoothPlastic a normal-only pack, which may be the one seen). Placeholders (4 × 4, flat or white): in the legacy set Granite's and Pebble's normal maps and Foil's and Ice's colour maps (Foil's crumpled look is all normal map), in the 2022 set Granite's normal map. The materials added in 2022 or later (Cardboard, Carpet, CeramicTiles, ClayRoofTiles, RoofShingles, Leather, Plaster, Rubber) have one set of textures, fetched in both. The tables of which `contentHash` was which material's map were `rook-content`'s `studio_cache/materials.rs` (removed in 0.12.0, when Rook began fetching the textures itself; the hashes are in git history); each set's averages give its materials' roughness and metalness when their textures aren't drawn (`rook_types::Material::shading`).

## MaterialService and `Use2022Materials`

- Files save the property as `Use2022MaterialsXml`. Scripts can't read `Use2022Materials` ("lacking capability RobloxScript"; Rook says it's "not a valid member", a difference in the message only); the overrides are saved as `<Material>Name` strings (`WoodName`), which scripts reach through `GetBaseMaterialOverride` and `SetBaseMaterialOverride`.
- **The flag picks the set** (Confirmed): a place saved with it true fetched and drew a second set of textures (red-pink Brick, light Granite, a light herringbone Pavement on white parts), while places saved with it false, with a MaterialService that doesn't save it, or with no MaterialService at all fetched and drew the legacy set (grey Brick, dark Granite). Rook reads the saved value under either name (the class default would hide it) and draws the legacy set without one (`rook_types::MaterialSet`).
- **Corrected in 0.6.0**: 0.5.0 said Studio draws one set whatever the flag says. Its "2022" comparison place was a session that had already loaded materials, and both of its test places turned out to be legacy places (one saved the flag under the wrong name, the other false), so the set mapped then, and the scales below, are the legacy set's. 0.5.0 drew that set for every place, which is right for the archive's places but not for places saving the flag on.
- Of the archive's XML places, five save the flag false  and two true; older places don't save it, so the archive is almost all legacy.

## Shading

Measured on a row of red (196, 40, 28) balls and blocks, one per material, under fixed Lighting (Ambient 70, OutdoorAmbient 128, Brightness 2, ClockTime 14, latitude 0), with `EnvironmentSpecularScale` at 0 and at 1:

- A place saved without `EnvironmentSpecularScale` or `EnvironmentDiffuseScale` reads 0 for both (Confirmed: a blank place's values).
- **At `EnvironmentSpecularScale` 0** nothing reflects the sky, and every material's sunlit face is about as bright as Plastic's (Plastic 184, SmoothPlastic 183, Glass 180, Ice 180, Foil 179 in red); Metal (134) and DiamondPlate (112) are darker by their colour maps' average (193 and 162 of 255). So metalness doesn't darken diffuse light here. Smooth materials (SmoothPlastic, Ice, Glass, Metal) show a small sharp sun highlight; Plastic a soft one.
- **At 1**, everything picks up the sky: Plastic's sunlit face goes from (184, 46, 36) to (183, 59, 57), more at grazing angles; Foil's face goes to (140, 39, 29), which is the sky's colour times Foil's red with no diffuse light left.
- Rook's model (`shaders/common.wgsl`, `shade_pbr`; Approximation): the old diffuse light (sun, Ambient, OutdoorAmbient) on the albedo times `1 - metalness × EnvironmentSpecularScale`, a GGX sun highlight, and the sky's average colour reflected with Schlick's Fresnel (base reflectance 0.04, or the albedo on metals), times `EnvironmentSpecularScale`. Rook's sunlit faces then come within about 10 levels of Studio's at both settings; Foil reflects darker (87 against 140 in red) because Rook reflects the skybox's average colour rather than the sky around the horizon. Shaded sides are darker than Studio's (86 against 111 to 117), as before this step: the ambient model is M7's.
- Neon draws brighter and more saturated than its Color: see [Neon](#neon-glass-and-forcefield).
- Reflectance is unchanged from step 4: it mixes towards the skybox's average colour, and still shows a part's surfaces.

## The built-in materials (part 2)

Measured on 24 × 24 stud faces of every material, white, under even light (ClockTime 12, latitude 0, `EnvironmentSpecularScale` 0), seen straight on from 400 studs at a field of view of 10 (11.4 pixels per stud), and close-ups of the uncertain ones from 140 studs (32.5 pixels per stud).

- **Where Rook gets them** (since 0.12.0): from Roblox, like any other asset. Each material's textures in each set are a TexturePack asset (asset type 63), a small XML file naming one image asset per map (`<color>`, `<normal>`, `<metalness>`, `<roughness>`; also `<usage>`, `<alphamode>`, `<tiling>` and, for CrackedLava, an emissive map). Asset delivery serves the packs and their images anonymously, so Rook fetches them through its cache like meshes and decals, and keeps them at 512 × 512 at most. Roughness goes into the map's green and metalness into its red (0 when a pack has none). Normal maps are ordinary RGB normal maps (they matched the cache's, decoded as above). A material without a pack (Plastic, SmoothPlastic, Neon, ForceField) has no textures. Offline, with nothing cached, materials draw untextured, as any missing asset does. Before 0.12.0 Rook read Studio's content cache (above) and drew its own textures in code where the cache had none. Both are gone.
- **Which pack is which material's** (`rook-content`'s `materials.rs`). Studio 0.743's executable holds two tables, `PartMaterialInfosPre2022` and `PartMaterialInfos2022`, next to a list of Material values: SmoothPlastic, Snow, Sandstone, Neon, Wood, WoodPlanks, Marble, Slate, Concrete, Granite, Brick, Pebble, CorrodedMetal, DiamondPlate, Foil, Metal, Grass, Sand, Fabric, Ice, Cobblestone, Glass, ForceField, Mud, Limestone, Basalt, Ground, LeafyGrass, Rock, Pavement, Glacier, CrackedLava, Salt, Asphalt, then the eight materials added since 2023. After them come 128 `rbxassetid://` TexturePack ids:
  - 46 for the legacy set;
  - 46 for the 2022 set;
  - 36 that are terrain's built-in table: they match the `texturePackId`s of the `FStringTerrainMaterialTable2022` flag, useful for M6.

  In each set's 46, the packs with a colour map follow the Material list in order (39 materials). The other seven are normal-only packs: a 4 × 4 placeholder, and normal maps placed before some materials. This was checked against the textures Studio itself showed, identified in its content cache per material (above), by comparing the pictures at 32 × 32 over every orientation:
  - most materials' normal maps came out the same image (differences of 0.3 to 1.5 out of 255 against several for any other pack), which pins every pack to its material;
  - for legacy Wood (0.5) and Slate (0.7) and 2022 Foil (1.2), Studio showed the normal map of the normal-only pack placed before them, not the one in their own pack, so Rook takes those three normals from there;
  - the other normal-only packs matched nothing Studio showed better than the material's own pack, and are unused (Inference: what Studio uses them for is unknown).

  The asset ids are identifiers, not Roblox's files.
- **How they lie on a face**: like a Texture, from each face's top-left corner in its image frame (`rook_types::NormalId`), unrotated and unmirrored (Confirmed: matching the textures against the captures over every rotation and mirroring, the plain orientation won for every material with a clear match).
- **Texture scale** (`rook_types::Material::studs_per_tile`), studs per repeat across and down. **The 2022 set repeats every 8 studs** (Confirmed for WoodPlanks 0.90, Brick 0.87, ClayRoofTiles 0.88, Pavement 0.86, Glass 0.72 and most others; Granite, Fabric, Foil and Salt matched weakly at any size and take 8 too). The legacy set:

  | Studs | Materials | Status |
  |---:|---|---|
  | 4 | Glass | Confirmed |
  | 5 | Wood, DiamondPlate, Metal | Confirmed |
  | 6.67 (20 / 3) | Fabric | Confirmed |
  | 8 | WoodPlanks, Brick, CrackedLava, Pavement, Cardboard, Carpet, CeramicTiles, ClayRoofTiles, RoofShingles, Leather, Plaster, Rubber, Snow | Confirmed |
  | 10 | Pebble, Marble, Sand, Ice | Confirmed for Pebble; the others from weaker matches (Inference) |
  | 10 | Grass | Approximation (fitted) |
  | 12 | Foil | Approximation (fitted) |
  | 16 | Cobblestone | Approximation (fitted) |
  | 16 | Slate, Basalt | Approximation (by eye; 8 in 0.5.0: the user saw Slate larger in Roblox, and Studio's close-up matched 16) |
  | 20 | Salt | Approximation (fitted) |
  | 24 | Granite, CorrodedMetal | Approximation (by eye) |
  | 8 | Concrete, Limestone, Rock, Sandstone, Mud, Ground, LeafyGrass, Glacier, Asphalt | Approximation (most materials' size) |

  "Confirmed" is a normalised cross-correlation of the capture with the texture tiled at that size, clearly above every other size (0.28 to 0.85). The rest repeat in no way a plain tiling explains: Studio breaks their repeats up (its Organic pattern, as MaterialVariant's `Pattern` offers), so their sizes were fitted by comparing the spread of feature sizes (power spectra) of Studio's close-ups with Rook's, or by eye. Rook repeats them plainly (Approximation).
- **Color tints by multiplying** (Confirmed): the part's Color times the texture, in linear light. White parts came out as bright as Plastic times the colour map's average (Wood predicted 184, measured 184; Brick 167 and 162; Concrete 169 and 166; Granite 84 and 86), and red ones likewise (Metal 137 and 134, DiamondPlate 116 and 112). The 2022 set the same way: fitting Studio's white Brick, WoodPlanks and Pavement against their maps gave one factor for all three channels (0.55, 0.64, 0.59), so their colour is the map's own.
- **Wedges**: a wedge's or corner wedge's faces take the material's textures in their own plane, from the face their own normal is closest to, measuring lengths along the slope. Images still go by the unit wedge's faces (a 45-degree slope belongs to Front, step 4), and following that for textures stretched a long, low wedge's slope: a large modern place's 37.4 × 6.8 × 98 Concrete wedge drew streaks 98 studs long (reported by the user, fixed in 0.5.1; Inference that Studio measures along the slope, as it shows no streaks there).
- **Surfaces** still show only on Plastic (step 4); every textured material drops them.
- **Meshes**: Rook lays a material's textures over file meshes and MeshParts as over their box's faces, like Decals on primitives (Inference: not checked in Studio). A mesh file whose normals are all zero is lit in Studio; Rook gives such vertices their triangle's normal instead of drawing them black (found with a test mesh written without normals).
- **Normal maps** are drawn at full strength. Rook's sides away from the sun are darker than Studio's (the ambient model, M7), so relief shows more there (a sunlit Brick face matched; Concrete's pits look stronger in Rook; Approximation).

## Neon, Glass and ForceField

- **Neon** (Confirmed, fitted): unlit, at 18 × Color² in linear light, capped at full brightness. Eight colours checked: grey 32, 64 and 128 drew 10, 60 and 237 (Rook 13, 62, 236); (196, 40, 28) drew (255, 20, 7) (Rook (255, 23, 9)); (255, 128, 0) drew (254, 237, 1). Step 4's "full colour" was true only of saturated colours.
- **Glass** at Transparency 0 is opaque, lit like any smooth material ((159, 31, 18) for red; Rook (152, 29, 20)). See-through Glass filters what's behind it by its Color instead of fading over it: red Glass at 0.5 over a dark blue wall drew (16, 4, 6) (Plastic at 0.5: (91, 33, 55)); at 0.8 over yellow it drew orange (197, 112, 6). Rook draws it twice: what's behind multiplied by a mix of white and its Color, fully up to Transparency 0.65 (`fs_glass_filter`), then its own surface, fading out by Transparency 0.5. Rook gives (14, 1, 2) and (174, 106, 6) (Approximation: fitted to two points).
- **ForceField** always shows what's behind it, even at Transparency 0, adding its Color: faintly where it faces the camera (red over dark blue: +0.014 in linear red), strongly at grazing angles (+0.18), less with Transparency. Rook adds Color × (0.025 + 0.6 × (1 − cos)³) × (1 − Transparency) (Approximation). Studio also brightens a block's outline edges and moves a pattern over the surface; Rook doesn't.

The measurements in this section and the previous ones (shading, scale, tint, Neon, Glass, ForceField) were taken in legacy places, except where the 2022 set is named.

## MaterialVariant and MaterialService overrides (part 3)

Checked with a test place holding two MaterialVariants and a part of each kind (`target/rook-mat-test/variants.rbxlx`, made by a script; the colour map is Roblox's public 2009 `face.png`, asset 12222850, whose see-through pixels are white), and the conformance case `content/materialservice`:

- **Which variant a part draws** (`rook_physics::materials::part_variant`): the one its `MaterialVariant` names (files save `MaterialVariantSerialized`), else the one MaterialService's override names for its material, if MaterialService has a MaterialVariant of that name *and* of the part's material among its descendants (one in a Folder counts). A variant of another base material is ignored (a Plastic part naming a Brick variant drew plain Plastic), and so is a name no variant has. An override applies to every part of that material (a Concrete part drew `ConcreteName`'s variant).
- **Drawing**: the variant's colour, normal, roughness and metalness maps (`ColorMapContent` or `ColorMap`, and so on) replace the material's textures, tiled at its `StudsPerTile` (default 10; 2 tiled an 8-stud face 4 × 4, 4 tiled it 2 × 2, as in Studio) like the built-in textures; the part's Color multiplies the colour map (black stayed black, white showed the part's red). A map it lacks leaves the material's plain value (Inference). `Pattern` Organic is drawn as Regular (Approximation).
- **The methods** (Confirmed, conformance case): `GetBaseMaterialOverride` answers the material's own name until an override is set ("Plastic"), and "" once set to ""; `SetBaseMaterialOverride` takes any name, even one no variant has; both fail with "<Method> failed: Material 'Neon' is not supported." for Neon, Glass, ForceField, Air and Water, which have no override property. `GetMaterialVariant(material, name)` finds a descendant MaterialVariant of that name and base material, or nil. Changing MaterialService or any MaterialVariant redraws the whole scene.
- Not drawn yet: a variant's `CustomPhysicalProperties` (physics still uses the material's); emissive masks.

## SurfaceAppearance (part 3)

Checked on cube MeshParts (a test mesh with each face's UVs over the whole image) with a red Color, one per `AlphaMode`, the same colour map:

- The maps go through the mesh's own UVs; the part's `TextureID` and Color don't show, except that **Overlay** shows the part's Color through the map's see-through pixels (red behind the face's black features). **Transparency** makes those pixels see-through; **Opaque** ignores the alpha (the map's white showed); **TintMask** left the see-through pixels untinted (Confirmed). That TintMask tints the opaque pixels by `Color`, and that `Color` multiplies the map in every mode, come from the creator docs (the test map's opaque pixels are black, which no tint changes).
- A SurfaceAppearance redraws its MeshPart when it changes. Only MeshParts take one (creator docs).
