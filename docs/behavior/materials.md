# Materials

What Rook found while building M4 step 5: PBR shading, MaterialService, the built-in materials, MaterialVariant and SurfaceAppearance, observed in Studio. The plan itself is in [ROADMAP.md](../../ROADMAP.md#m4-content-meshes-materials-textures-decals); what's still partial is in [stubs.md](stubs.md).

Unless marked, everything here was observed in Studio 0.742 (version 0.742.0.7421053) on 2026-10-14, with test scenes built by the same Luau script in Studio (MCP `execute_luau`) and in Rook (`rook-player --script`), compared with `screen_capture` and `rook-player --screenshot` at the same camera and a narrow field of view.

## Where Studio keeps the material textures

The install holds only Plastic's textures (`PlatformContent/pc/textures/plastic/`) and the surface patterns (`studs.dds`). Every other material's textures are downloaded when a part first shows the material and kept in Studio's content cache (Confirmed by reading the cache before and after making one part of each material):

- **The cache** is `%LOCALAPPDATA%\Roblox\rbx-storage.db`, an SQLite database with one table, `files (id BLOB, content BLOB, size, hits, atime, category, score, ttl)`. Small entries are stored in `content`; larger ones have `content` NULL and live in `%LOCALAPPDATA%\Roblox\rbx-storage\<first two hex digits of id>\<id as hex>`. Studio and the player share it.
- **Each entry** is `RBXH`, a little-endian u32 version (2), a u32 URL length, the URL, then the response status, headers and body. Row ids aren't a plain MD5, SHA-1, SHA-256 or BLAKE2 of the URL.
- **Material textures** are TexturePack assets (`roblox-assettypeid: 63`): Studio asks `contentdelivery.roblox.com/v1/bytes/<hash>`, which redirects (302, target not cached) to a body cached under `https://cN.rbxcdn.com/<hash>` or under an entry with no URL. Both URLs need Roblox's signed tokens (a plain request is refused with 403), so Rook reads the cache and never fetches them.
- **The bodies are KTX2** files, Zstandard-supercompressed (scheme 2), in BC1 (`vkFormat` 131) or BC3 (137). Each texture is split into up to four mip "packs" (`pack_0` the 32 or 64 px levels down to 1 px, then 256, 512 and 1024 px), each a separate file; Studio fetches the larger packs only when the camera comes close, so a cache may hold only the small ones. Every pack's key-value data names the whole texture (`contentHash`), its colour space (`Gamma2RGB` or `Linear`), its original size (`RobloxOriginalWidth`, 512 or 1024) and its average channels.
- **Three textures per material**, identified by giving each material its own 20-second window in a fresh Studio (the cache writes bodies in batches every 15 seconds) and confirmed for 29 of 38 materials by correlating the normal map with the colour map's gradients (0.19 to 0.83; the rest, smooth or noisy materials, score near 0):
  - *colour* (sRGB), mostly grey so that the part's Color tints it;
  - *normal* (linear), stored DXT5nm-style: X in alpha, Y in green, red 255 and blue 0;
  - *roughness-metalness* (linear): red is metalness (Foil 255, Metal 202, DiamondPlate 168, CorrodedMetal 134, most others 0), green roughness (Glass 29, Ice 45, Brick 246), alpha unknown (often 255; height or occlusion, Inference).

  Plastic, SmoothPlastic, Neon and ForceField fetch nothing. Granite's normal map and Foil's colour map weren't caught in their windows. The averages above give each material's roughness and metalness when its textures aren't drawn (`rook_types::Material::shading`).

## MaterialService and `Use2022Materials`

- Files save the property as `Use2022MaterialsXml` (a test place saved with `Use2022Materials` loaded as a 2022 place). Scripts can't read `Use2022Materials` ("lacking capability RobloxScript"); the overrides are saved as `<Material>Name` strings (`WoodName`), which scripts reach through `GetBaseMaterialOverride` and `SetBaseMaterialOverride`.
- **Studio 0.742 draws the 2022 materials whatever `Use2022MaterialsXml` says.** A place saved with it false fetched exactly the same 114 textures as a place saved with it true, and drew the same Brick, Grass, DiamondPlate and Wood patterns once they had loaded (an early capture looked "legacy" only because the large packs hadn't arrived). No other files were read. Of the archive's XML places, five save it false (two 2016 core places, a modern one, a lab place, a 2008 helicopter place); they draw the 2022 set in Studio today, so Rook draws the 2022 set for every place, including places with no MaterialService (Inference from the same evidence: the flag changes nothing).

## Shading

Measured on a row of red (196, 40, 28) balls and blocks, one per material, under fixed Lighting (Ambient 70, OutdoorAmbient 128, Brightness 2, ClockTime 14, latitude 0), with `EnvironmentSpecularScale` at 0 and at 1:

- A place saved without `EnvironmentSpecularScale` or `EnvironmentDiffuseScale` reads 0 for both (Confirmed: a blank place's values).
- **At `EnvironmentSpecularScale` 0** nothing reflects the sky, and every material's sunlit face is about as bright as Plastic's (Plastic 184, SmoothPlastic 183, Glass 180, Ice 180, Foil 179 in red); Metal (134) and DiamondPlate (112) are darker by their colour maps' average (193 and 162 of 255). So metalness doesn't darken diffuse light here. Smooth materials (SmoothPlastic, Ice, Glass, Metal) show a small sharp sun highlight; Plastic a soft one.
- **At 1**, everything picks up the sky: Plastic's sunlit face goes from (184, 46, 36) to (183, 59, 57), more at grazing angles; Foil's face goes to (140, 39, 29), which is the sky's colour times Foil's red with no diffuse light left.
- Rook's model (`shaders/common.wgsl`, `shade_pbr`; Approximation): the old diffuse light (sun, Ambient, OutdoorAmbient) on the albedo times `1 - metalness × EnvironmentSpecularScale`, a GGX sun highlight, and the sky's average colour reflected with Schlick's Fresnel (base reflectance 0.04, or the albedo on metals), times `EnvironmentSpecularScale`. Rook's sunlit faces then come within about 10 levels of Studio's at both settings; Foil reflects darker (87 against 140 in red) because Rook reflects the skybox's average colour rather than the sky around the horizon. Shaded sides are darker than Studio's (86 against 111 to 117), as before this step: the ambient model is M7's.
- Neon draws brighter and more saturated than its Color in this lighting ((196, 40, 28) drew (254, 21, 7)); Rook still draws it at its Color, unlit, as step 4 observed.
- Reflectance is unchanged from step 4: it mixes towards the skybox's average colour, and still shows a part's surfaces.
