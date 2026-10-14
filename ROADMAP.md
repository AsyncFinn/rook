# Roadmap

This is the one place Rook's plan lives. The README, `docs/architecture.md`
and `docs/behavior/stubs.md` refer to milestones by name and link here.

## How the order is decided

1. **Visible progress on archived places first.** Rook's first job is to
   open an archived place and show it as Roblox would.
2. **Then by what the archive uses.** Counts below come from the corpus
   scan (`reports/corpus/`): 3,056 place and model files that load.
3. **Dependencies.** A milestone that needs another's work comes after it
   (characters need physics and the head mesh).

Each milestone is a vertical slice with a measurable exit criterion, and
each release says which milestone it finishes (see `CHANGELOG.md`).

## Now, next, later

- **Now:** M4 Content: the built-in mesh shapes, asset fetching, file meshes, textures and decals and trusses are done; unions are next.
- **Next:** M5 Characters.
- **Later:** M6 GUI, M7 Audio and M8 Networking.

### Rook extensions (unscheduled)

Opt-in features beyond what Roblox allows, behind an explicit profile and off by default.
None are scheduled; each comes after the milestone it depends on.

- WebSockets available to any script (after M8 Networking)
- EditableImage and EditableMesh without platform gating (after M4 Content)
- Restoring embedded legacy textures that modern Roblox loads as empty

## Milestones

| | Milestone | Exit criterion |
|---|---|---|
| ✅ | **M0** File formats, datatypes, reflection, CLI | All test fixtures round-trip; 99.3% of the archive loads (v0.1.0) |
| ✅ | **M1** Instance tree, signals, scheduler, Luau, server Scripts, conformance | First conformance batch passes; baseline of archive scripts running (v0.1.0) |
| ✅ | **M2** Renderer v0, client/server split, LocalScripts, basic input | Archived places render recognisably; client scripts run alongside server scripts (v0.2.0) |
| ✅ | **M3** Physics | Physics conformance batch passes; unanchored places settle plausibly |
| ⬜ | **M4** Content: meshes, textures, decals | Archived places render with their meshes, decals and textures |
| ⬜ | **M5** Characters | Walk around and use tools in an archived place |
| ⬜ | **M6** GUI | GUI conformance batch passes |
| ⬜ | **M7** Audio | Archived places play their sounds |
| ⬜ | **M8** Networking: transport, headless server, multiple clients | Two clients play together |
| ⬜ | **M9+** Terrain, animation, effects, advanced lighting, Studio tooling | — |

### M3 Physics

Rapier underneath, Roblox's rules on top, every rule checked in Studio.

1. ✅ Rapier world, anchoring, write-back.
2. ✅ Joints and assemblies (welds, Snaps, Motor6D, legacy surface joints, `MakeJoints`).
3. ✅ Contacts, `Touched`, collision groups, materials.
4. ✅ BodyMovers, driven hinges (RotateV, RotateP), the most-used constraints, `Velocity` and conveyors.
5. ✅ Queries on Rapier, legacy first, by files using them: `FindPartOnRay` (205), `FindPartsInRegion3` (135), `FindPartOnRayWithIgnoreList` (123), `GetTouchingParts` (27), `FindPartOnRayWithWhitelist` (15), `Workspace:Raycast` with `RaycastParams`/`RaycastResult` (15), `FindPartsInRegion3WithIgnoreList`/`WithWhiteList` (17), `GetPartBoundsInBox`, `GetPartsInPart` and `GetPartBoundsInRadius` with `OverlapParams` (12), `Blockcast` and `Spherecast` (8); also Camera `ScreenPointToRay`/`ViewportPointToRay` (51), which scripts pair with raycasts. The mouse and ClickDetectors moved over, and `crates/rook-engine/src/raycast.rs`'s own geometry is gone. Clients got a physics world that only answers queries.

M3's exit criterion is met: the physics conformance batch (`physics/*`, six cases) passes, and unanchored places settle plausibly.

### M4 Content: meshes, textures, decals

The most visible gap: **SpecialMesh is in 65% of files** (1,988 files,
494k instances), CylinderMesh in 723 and BlockMesh in 574. Today the
renderer draws all of these as their part's plain box.

1. ✅ **Built-in mesh shapes**, no downloads needed: SpecialMesh's `MeshType`
   shapes, BlockMesh and CylinderMesh, with `Scale` and `Offset`
   (`VertexColor` waits for textures). Every type but Head reuses a shape
   Rook already has (see the table below); the size rules are defined once
   in `rook-types` (`MeshKind`, `MeshLook`). Head is a mesh file in Roblox,
   so Rook draws its own head mesh, built in `rook-render` from
   measurements of the user's copy, which is never committed (see "Content
   and assets" below); from step 2 on, the user's install can supply the
   real file instead.
2. ✅ **Asset fetching**: `rbxasset://`, `rbxassetid://` and legacy asset
   URLs through the provider chain (Rook defaults, the user's Roblox
   install, content folders, asset delivery), cached and hash-checked, in
   the new `rook-content` crate; `ContentProvider:PreloadAsync`; `rook
   content`. Findings and corpus counts are under "Asset fetching" below.
3. ✅ **File meshes**: SpecialMesh `MeshId`, FileMesh and MeshPart, Roblox
   mesh versions 1.00–5.00, loaded in the background and drawn at Studio's
   sizes. Counts and findings are under "File meshes" below.
4. ✅ **Textures and decals**: images decoded in the background (PNG, JPEG,
   BMP, DDS), Decals and Textures on every part shape and mesh, mesh
   `TextureId` and MeshPart `TextureID` with `VertexColor`, Plastic's surface
   patterns, Neon and Reflectance, drawn as Studio draws them. Counts and
   findings are under "Textures and decals" below.
5. **Unions**: draw UnionOperations with their own geometry instead of
   their bounding box (`UnionOperation.Content`, the union's geometry
   asset, is set on 70,944 unions in 99 files; `NegateOperation` 6 in 2).
   The `rbx_mesh` crate Rook already uses decodes union geometry too: its
   `union_graphics` module reads the `MeshData` a union's asset holds
   (versions 2, 4 and 5, plus the old `CSGK` stub), and `union_physics`
   reads `PhysicsData` (versions 3 to 8; version 8 needs zstd, for which
   the crate offers the pure-Rust `ruzstd`). Turning on those features
   adds no new dependency. First count which forms the archive uses
   (geometry in the file or behind an asset id, and whether those assets
   load anonymously), then decode through `rook_content::mesh` and draw
   through step 3's mesh loader and GPU cache. Check in Studio how a
   union's geometry is sized against its Size (unions can be resized),
   `UsePartColor`, and what a union draws while loading or after failing.
   Unions' collision hulls from `PhysicsData` go with MeshPart collision
   fidelity (below).

Physics is to pick up the real shapes and volumes of MeshParts
(collision fidelity) and unions (`PhysicsData`), replacing today's boxes;
it wasn't done in step 3.

**What each SpecialMesh `MeshType` draws** (observed in Studio 0.742,
2026-10-12, one 4 × 4 × 4 part per type, set by script because the
Properties panel hides the deprecated types):

| MeshType | Drawn as | Rook geometry |
|---|---|---|
| Brick | The part's box | The existing block |
| Torso | Identical to Brick (Roblox made it so in 2018; the old sloped torso is gone) | The existing block |
| Wedge | A wedge | The existing wedge |
| CornerWedge (deprecated) | The same shape as a CornerWedge part (checked by overlapping the two) | The existing corner wedge |
| Cylinder | A cylinder along X, like a Cylinder part | The existing cylinder |
| Sphere | A sphere (creator-docs: unlike a Ball part, it stretches on every axis) | The existing ball, scaled per axis |
| Head | The head mesh (below), fitted to the part | Rook's own head mesh, matching the measured one |
| Prism, Pyramid, ParallelRamp, RightAngleRamp (deprecated) | Nothing | None: draw nothing |

The existing shapes live in `rook-types/src/shape.rs` (`PartShape`), so
SpecialMesh types reuse them rather than defining their own. A SpecialMesh
changes only how a part looks: the invisible types still collide and are
hit by queries. Sphere and Cylinder meshes showed the part's surface studs,
which matters for the textures step.

**The Head MeshType**, everything measured so far:

- *What it draws:* the mesh file `content/avatar/heads/head.mesh` in
  Studio's install (a version 2.00 mesh, 28,789 bytes, about 1.2 studs each
  way at its own size). Proof: with that file renamed, setting `MeshType`
  to Head changed the enum but not the drawing (a Sphere stayed a sphere,
  with no error), and the executable names `rbxasset://avatar/heads/head.mesh`
  among the render mesh manager's strings.
- *How it's sized:* the mesh is scaled by (min(X, Z), Y, min(X, Z)) / 1.25
  of the part's Size. So it's round from above and never stretched along the
  longer of X and Z (on 2 × 1 × 50 and 50 × 1 × 2 it is a 2-stud head).
  Found by overlapping it with a FileMesh of the same mesh: on 2 × 2 × 2 the
  file at scale 1.60 sat just inside it and at 1.61 just covered it, with
  no rim anywhere; on 3 × 1 × 2 and 10 × 1 × 2 the file at
  (1.61, 0.805, 1.61) covered it the same way, seen from the side and from
  above.
- *Its shape* (measured from the user's copy with a throwaway tool,
  2026-10-12; 517 vertices, 846 triangles): a solid of revolution about Y
  with 36 segments, the first 5 degrees from +X. Its profile is a cylinder
  of radius R = 0.6012 whose straight side runs from -R/2 to R/2, each rim
  rounded by a quarter circle of radius R/2 in 4 steps of 22.5 degrees,
  and flat round caps (the top a fan, the bottom a grid). Normals are
  smooth around and all along the profile, so the rims blend into the
  caps. Rook's head is built from exactly these facts.
- *With `Scale`:* the Size rule applies to Size × Scale (see below). The
  classic R6 head, a 2 × 1 × 1 part with Scale 1.25, is therefore
  min(2.5, 1.25) = 1.25 wide and 1.25 tall before the ÷ 1.25, so it draws
  at the mesh's own 1.2 studs.

**`rbxasset://fonts/*.mesh`**, the legacy paths archived places use for
the character meshes (head, torso, arms, legs, VikingHelmet, the Composit*
clothing meshes): these are no longer files in the install. Roblox
uploaded each one in 2009 as an ordinary asset by @Roblox whose
description is its old path (`head.mesh`, asset 12221462, description
`\fonts\head.mesh`, created 2009-06-24, updated 2017-08-23; `leftarm.mesh`
12221505, `rightleg.mesh` 12221626), and Studio's executable holds a table
pairing each old path with its asset id (the id comes just before its
path). Asset 12221462 is byte-identical to the install's `head.mesh`: two
entries in Studio's download cache (`rbx-storage`) are a 37-byte `RBXH`
header followed by exactly that file. One of them was written while
`fonts/head.mesh` FileMeshes were first loaded, with the local file still
present, and `fonts/head.mesh` kept loading after the local file was
renamed, so the old path is fetched as its asset (Inference from the
timing; not yet proved by forcing a fresh download). Step 2 confirmed it
and built Rook's own path → asset id table (see "Asset fetching" below).

So the Head MeshType, `rbxasset://fonts/head.mesh` and asset 12221462 are
the same mesh. They differ only in sizing: the Head type is fitted to the
part's Size as above, while a FileMesh ignores Size and is sized by `Scale`
alone.

Two traps when comparing meshes in Studio: a FileMesh ignores the part's
Size (only `Scale` sizes it, so `head.mesh` stays about 1.2 studs on any
part), and an unexpected `Scale` on the mesh (one test had X at about 3.07)
changes the shape completely. Check both before concluding two meshes
differ. To compare shapes, size the FileMesh by hand to where the other one
should be and overlap them in two colours; to compare sizing, give both the
same Size and Scale.

**How `Scale` combines with Size** (measured 2026-10-12: every type on a
2 × 3 × 4 part, at Scale 1 and at Scale (0.5, 1.5, 0.75), front and top
views): Scale multiplies the part's Size per axis first, and each type's
shape rule then works on that scaled size. With S = Size × Scale:

| Type | Drawn size | Measured at Scale (0.5, 1.5, 0.75): S = (1, 4.5, 3) |
|---|---|---|
| Brick, Wedge, CornerWedge, Sphere, BlockMesh | S | 1 × 4.5 × 3 |
| Cylinder (SpecialMesh; along X) | length S.X, diameter min(S.Y, S.Z) | 1 long, 3 across, round |
| CylinderMesh (along Y) | height S.Y, diameter min(S.X, S.Z) | 4.5 tall, 1 across, round |
| Head | the mesh × (min(S.X, S.Z), S.Y, min(S.X, S.Z)) / 1.25 | 0.95 × 4.29 × 0.95 (predicted 0.96 × 4.32 × 0.96) |

Applying Scale after the shape rule would have given a 1.5-stud CylinderMesh
across Z and a 1.44-stud Head across Z, so the order is settled. Note the
two cylinders differ: SpecialMesh `Cylinder` lies along X like a Cylinder
part; `CylinderMesh` stands along Y. At Scale 1, every type's sizes equal
the part's Size (2 × 3 × 4, Cylinder 3 across, CylinderMesh 2 across, Head
1.9 × 2.9 × 1.9).

**More rules found while building step 1** (Studio, 2026-10-12):

- *Several meshes on one part:* the last child counts. A SpecialMesh
  Sphere then a CylinderMesh drew the cylinder; the other order drew the
  sphere; changing the losing mesh changed nothing; removing the winner let
  the other draw.
- *A mesh replaces the part's own shape:* a Brick SpecialMesh on a
  Cylinder part draws a box.
- *A file mesh with an empty `MeshId`* (the FileMesh class, or SpecialMesh
  `MeshType` FileMesh) draws a 1 × 1 × 1 box times `Scale`, whatever the
  part's Size, without the part's Neon glow.
- *`Offset`* moves the mesh in the part's own axes (a part turned 90
  degrees about Z with Offset (0, 2, 0) drew 2 studs along world -X), as
  documented. `VertexColor` wasn't measured; it waits for textures.

**Checked against Studio** (2026-10-12): every type at Scale 1 and at
(0.5, 1.5, 0.75) on 2 × 3 × 4 parts, plus Heads on
2 × 1 × 2, 2 × 1 × 50 and the R6 head, rendered in Studio and in
`rook-player --screenshot` with the same cameras (FieldOfView 2, 600 studs
away, about 26 px/stud, front and top) and compared as red silhouette
masks: no pixel differs more than 2 px from the silhouette edges (the rest
are one-pixel rasterisation rims, about 2,700 px over six 308 × 538
views). Close-ups of the three Heads (about 103 px/stud) match Studio's
outline to a pixel in width and height.

**Asset fetching (step 2)**, built 2026-10-12. How it works is in
`docs/architecture.md` §8 "Content"; what's still partial is in
`docs/behavior/stubs.md`.

*What the archive uses* (`rook corpus-scan`, 3,058 files that load): 1.10
million non-empty content property values (36,938 distinct), plus 1.69
million empty ones. By what `rook-content` makes of them:

| Kind | Files | Uses |
|---|---:|---:|
| Asset id (`rbxassetid://` or an asset URL) | 1,803 | 990,851 |
| Asset id with `version=` | 657 | 5,615 |
| `rbxasset://` path in today's install | 324 | 5,491 |
| `rbxasset://` path of the 2009 client, now a public asset (table below) | 1,516 | 77,222 |
| `rbxasset://` path found nowhere (`textures/rocket.png`, `fonts/paintballgun.mesh`, ...) | 419 | 2,138 |
| `rbxasset://../../../shareddata/...` (2006-2008 server paths) | 470 | 5,375 |
| No content (other URLs, junk) | 411 | 9,422 |
| `rbxthumb://` | 1 | 9 |

By URL shape (digits shown as `#`): `http://www.roblox.com/asset/?id=#`
533,190 uses in 1,739 files; `rbxassetid://#` 355,463 in 313;
`http://www.roblox.com/asset/?version=#&id=#` 5,607 in 656 (skyboxes,
mostly `version=1`); `www.roblox.com//asset/?id=#` with a doubled slash
72,364 in 135; `asset?id=` without the slash 19,130 in 237; case variants
(`Asset?ID=`, `asset/?ID=`, `Asset/?id=`, ...) 6,613 in about 370; `roblox.com`
without `www` 2,079 in 70; a value with spaces around it (`?id= 123`,
`...123 `) about 3,300 in about 330; `assetdelivery.roblox.com/v1/asset/?id=`
2,888 in 12; misspelt and third-party hosts (`rblox.com`, `ww.roblox.com`,
`subter.org`, `xdiscuss.net`) 592 in 34; `asset.aspx?id=` 120 in 15; old
non-asset endpoints (`thumbs/avatar.ashx`, `Game/Tools/ThumbnailAsset.ashx`,
`asset/Default.ashx?hash=`) about 1,100. `rbxasset://` paths come with
backslashes (`Textures\*.png` 4,715 uses in 813 files, `sounds\*.wav`
6,153 in 547) as often as with slashes. The properties: Decal `Texture`
(1,666 files), SpecialMesh `MeshId` (1,718) and `TextureId` (1,620), Sound
`SoundId` (1,258), Sky (920), Tool `TextureId` (860), ImageLabel `Image`
(429), Texture `Texture` (409 files, 308k uses), MeshPart `MeshId` (67
files, 133k uses), Animation `AnimationId` (265), and script `LinkedSource` in
836 (Script and LocalScript, counted apart). Scripts write asset URLs too: `http://www.roblox.com/asset/?id=`
in 832 files, `rbxasset://textures\...` in 1,084.

*Which forms Studio loads* (observed in Studio 0.742: a FileMesh per form
with the head mesh's id, drawn or not). Loads: `rbxassetid://5`, any
`http`/`https` URL whose path ends in `asset` (any host, even
`example.com` or `subter.org`; `asset` in any case, with or without the
trailing slash, doubled slashes, `v1/asset`), query keys in any case,
spaces around the id, a `+`, leading zeros, extra parameters, the last
`id=` when there are two, a non-numeric `version` ignored, trailing spaces,
leading spaces before `http`, and `rbxassetid://5&x=1` (so
`rbxassetid://<rest>` works like `?id=<rest>`). Doesn't load: upper-case
`rbxasset://` or `rbxassetid://` schemes, a leading space before
`rbxassetid://`, `rbxassetid://5/`, an id followed by anything else
(`5abc`, `5;`, `5#x`), a bare number, `asset.aspx`, `asset/Default.ashx`,
other paths, and no scheme. `version=` is honoured: version 1 of asset
12221462 is a different, much larger mesh. `rook-content` parses by
exactly these rules (`reference.rs`, with each form as a test).

*A file mesh that doesn't load draws nothing* (observed: not found,
forbidden, a missing `rbxasset://` file and junk all drew no mesh and
printed nothing), unlike an empty `MeshId`, which draws the 1-stud box.

*An image that doesn't load draws nothing either* (observed 2026-10-13,
Decals on the front of blue parts). Two checks:

- *Malformed bytes:* two `rbxasset://` test files put in Studio's
  `content/textures` (and removed afterwards): a PNG cut in half, and a
  valid PNG signature and header followed by random bytes. Both drew
  nothing, and `PreloadAsync` reported `Failure`. So did a missing asset,
  a forbidden one, a missing file, junk, and a mesh used as a texture.
- *Replacing a loaded image:* Decals first showed a face (loaded and drawn,
  `PreloadAsync` `Success`), then had `Texture` set to each failing value
  (the two malformed PNGs, a missing asset, a mesh, a missing file). Every
  one cleared to the plain face within the same check, while an unchanged
  control kept its face. A failed image replaces the old one with nothing;
  it doesn't keep it, and there's no placeholder or checkerboard.
- *Non-image local files* (`rbxasset://avatar/heads/head.mesh`, an `.mp3`,
  an `.ogg`, a `.ttf`): blank and `Failure` too, both fresh and replacing
  a loaded face. Neither a local file nor an asset id brings back the
  checkerboard.

Old Studio did differ. The same test in a 2013 Studio (run by the
maintainer) drew a black and white checkerboard for a local `.mesh` and
`.wav` used as a texture, and no checkerboard for the two malformed PNGs
or the missing file. (Its parts drew black, which hides the black-lined
face, so whether those cleared or kept the face needs a recheck on a
light-coloured part.) So the old client seems to have shown a placeholder
for files it didn't recognise as images at all; modern Roblox draws
nothing in every case.

*Setting content properties* (observed): every content property turns
backslashes in an `rbxasset://` path into slashes, keeping the case.
Decal's and Texture's `Texture` and Decal's `ColorMap` also rewrite a
lower-case `http://www.roblox.com/asset/?id=5` (and `roblox.com/asset?id=5
&version=2`, dropping the version) to `rbxassetid://5`; `MeshId`,
`SoundId`, `Image`, the Sky faces and the rest keep the URL. Rook does the
first, not yet the second. Files store these values in the `Content`
properties (`MeshContent`, `TextureContent`, `AudioContent`), and Rook's
scripts read `MeshId` and the others through them.

*The 2009 client's paths* (observed): modern Studio loads
`rbxasset://textures/SpawnLocation.png`, `sky/null_plainsky512_bk.jpg` and
`fonts/sword.mesh` although no install has them. On 2009-06-24 Roblox
uploaded every file of that client's `fonts`, `music`, `sky`, `sounds` and
`textures` folders as a public asset whose description is the old path, in
alphabetical order (ids 12220916 to 12224353, 262 files);
`MarketplaceService:GetProductInfo` over that range gave Rook's table
(`rook-content/src/defaults.rs`). Asset 12221462 is byte-identical to the
install's `avatar/heads/head.mesh` (same BLAKE3). Studio also loads a few
old paths that aren't in that upload (`textures/rocket.png`,
`TrowelTexture.png`, `paintballicon.png`, `rocketboots.png`; 2,138 uses
together with the rest); how is unknown.

*PreloadAsync* (observed on a playtest client, a server and in edit mode;
the server part is the conformance case `content/preload`): on a client it
yields, calls back `(contentId, Enum.AssetFetchStatus)` for each entry in
the order they finish, and returns nothing after the last. Instances count
with their descendants (a Model holding a Decal reported the Decal's
texture). Strings load as images: `rbxasset://` images succeeded, `.mesh`
and `.wav` paths failed, and every `rbxassetid://` or asset URL string
failed, even an image that loaded as a Decal's texture. Empty strings are
skipped; a repeated entry gets a callback each time. On a server it never
returned (over 30 s) once anything was to be fetched, even `"abc"`, and
returned at once for `{}`, `{""}` or a Folder. Argument errors: `Unable to
cast string to Array`, `Unable to cast double to ContentId`, `Unable to
cast double to Function`. `RequestQueueSize` read 0 on an idle server and
rose to 5-6 during a client preload, not tracking its entries exactly.

*Asset delivery today* (tested 2026-10-12): Roblox announced in March 2025
that every assetdelivery.roblox.com endpoint would need authentication,
with Open Cloud's `apis.roblox.com/asset-delivery-api/v1/assetId/<id>` and
an API key (scope `legacy-asset:manage`) as the replacement. In practice
anonymous `GET assetdelivery.roblox.com/v2/assetId/<id>` still works for
some public assets: it answers with HTTP 200 and JSON either way, a signed
CDN URL (served gzip-encoded) or an error: 404 "Request asset was not
found", 409 "User is not authorized to access Asset." (not public), or
"Authentication required to access Asset." (public, but now behind an
account), and lookups by asset version id are refused as deprecated. The
Open Cloud endpoint refuses requests without a key. Rook asks anonymously
and tries a user's key (`ROOK_OPEN_CLOUD_API_KEY`) only for refused assets;
that path is untested for lack of a key, and whether a key opens other
creators' public assets is unknown. Roblox's own 2009 assets (the table
above) come anonymously.

*Results* (`rook content --place`, first run, network on):

| Place | References (distinct targets) | Install | Asset delivery or cache | Refused anonymously | Not found or no content |
|---|---|---:|---:|---:|---:|
| A 2017 disaster place | 198 (102) | 5 (3) | 141 (73) | 51 (26) | 1 (1) |
| A modern core place | 9,284 (1,655) | 229 (18) | 1,440 (418) | 7,610 (1,215) | 5 (5) |

Most refusals are "Authentication required" (a core place: 7,471 uses, 1,141
assets). A second run needs no network: a core place resolved all 1,655 targets
from the install and cache in 0.2 s. The player draws the Head MeshType
with the install's `head.mesh`, and `rook run-headless --seconds 10` on a modern core place, a disaster place
and a 2008 place gave identical output in the same time as before (a core place
22.3 s against 22.5 s, four alternating runs each).

*Mesh files are read with the `rbx_mesh` crate* (krakow10, MIT or
Apache-2.0, a Rojo contributor's), versions 1.00 to 5.00; Rook keeps only
the highest level of detail and checks face indices. 6.00 and 7.00 aren't
read yet. `rook content` decodes meshes it shows: `fonts/sword.mesh` (1.00)
is 198 triangles, 1.15 x 0.34 x 4.37 studs; version 1 of asset 12221462
(1.00) is 56 x 40 x 45 studs, the giant mesh Studio drew for it.

*Left for steps 3 and 4:* drawing file meshes at `Scale` and drawing
nothing when one fails to load (checking 1.00's halved size and the V flip
against Studio while at it), mesh versions 6.00 and 7.00; image decoding
(PNG, JPEG, DDS, the old `.tex`) and textures, decals and materials;
`VertexColor`. Also open: thumbnails (`rbxthumb://`), the old paths outside
Rook's table, Decal's URL rewrite, `require(assetId)` and
`InsertService:LoadAsset` (asset fetching can download the model; loading
it isn't done), and assets that need an account.

**File meshes (step 3)**, built 2026-10-13. How loading and drawing work
is in `docs/architecture.md` §8 "Rendering"; what's left is in
`docs/behavior/stubs.md`.

*What the archive uses* (`rook corpus-scan`, 3,058 files that load; the
scan now counts mesh instances by kind):

| Kind | Files | Uses |
|---|---:|---:|
| SpecialMesh `MeshType` FileMesh with a `MeshId` | 1,716 | 99,940 |
| SpecialMesh FileMesh with an empty `MeshId` | 162 | 1,838 |
| SpecialMesh of another type with a `MeshId` (ignored) | up to 25 | 444 |
| FileMesh class with a `MeshId` | 6 | 8 |
| MeshPart with a `MeshId` | 74 | 133,246 |
| MeshPart with an empty `MeshId` | 19 | 235 |
| CharacterMesh with a `MeshId` | 166 | 7,254 |
| A mesh under a union | up to 5 | 56 |
| A mesh under a truss | up to 7 | 21 |
| A mesh under a MeshPart | 0 | 0 |

Every MeshPart in the archive saved `InitialSize`. 13 of the 74 MeshPart
files are models from a 2016 Roblox install that saved `MeshID` (capital D)
instead of `MeshContent`; Rook reads either. *Mesh versions*, from `rook
content --place` (which now decodes every mesh reference and totals the
results) over 60 random archive places plus three with many MeshParts, 41
of them with meshes: of the meshes that load anonymously, 358 distinct are
version 1.00, 368 are 2.00, 12 are 1.01, 6 are 4.00, 6 are 4.01, 5 are 3.00
and 1 is 3.01; none is 5.00, 6.00 or 7.00, so reading 6.00 and 7.00 can
wait. 2,833 distinct meshes (34,329 uses, mostly MeshParts) are refused
without an account, so their versions are unknown. A 2017 disaster place: 40 mesh
uses, 39 decode (1.00 and 2.00), 1 refused. A modern core place: 827 uses, 788
decode (1.00, 1.01, 2.00), 39 refused, including all 7 of its MeshParts'
meshes.

*Checked in Studio 0.742* (2026-10-13; observed unless marked):

- *Where a file mesh sits:* the mesh's own origin is at the part's
  position, not its bounds' centre. Version 1 of asset 12221462 is
  centred 11.6 studs off its origin and drew 11.6 studs off the part, as a
  FileMesh and as a MeshPart alike. `Offset` moves it in the part's axes as
  for the other types (checked again: Offset (0, 2, 0) on a head.mesh at
  Scale (5, 3, 5)).
- *MeshPart sizing:* a MeshPart made from a mesh has Size and `MeshSize`
  equal to the mesh's bounds (head.mesh: 1.1978 × 1.2024 × 1.1978). The
  mesh is drawn at its vertices × Size / `MeshSize`, about its own origin:
  setting Size to (0.5, 0.5, 0.25) of `MeshSize` shrank it by exactly that.
  `MeshSize` is what files save as `InitialSize` and doesn't change when a
  script sets Size or when the mesh loads. A MeshPart saved without
  `InitialSize` reads `MeshSize` (0, 0, 0) and draws the mesh at its
  vertices × Size (a 1.2-stud mesh on a 2 × 4 × 8 part drew 2.4 × 4.8).
  A new MeshPart is 4 × 1.2 × 2 with `MeshSize` 0; with an empty `MeshId`
  it draws its Size box (with a placeholder checker texture). Scripts can't
  write `MeshId` ("lacking capability NotAccessible"); conformance case
  `content/meshpart`.
- *Loading:* a new mesh draws nothing until its file arrives. When
  `MeshId` changes, what the mesh drew before stays until the new file
  arrives: 24 loaded heads stayed heads while their new meshes queued
  behind 1,500 other requests, then changed, and 12 empty-`MeshId` boxes
  stayed boxes. A file that fails keeps the old mesh for good: a loaded
  head given a missing asset, a missing file, `abc`, a PNG or asset 1
  stayed a head, while `""` gave the empty-`MeshId` box. A new mesh that
  fails draws nothing (step 2), for MeshParts too (not found, missing file
  and junk drew nothing). Once, with `Scale` changed in the same frame as
  `MeshId`, the meshes still loading drew nothing; Rook doesn't copy that.
  Most loads finished before a capture could catch them (144 uncached
  meshes all appeared within about half a second).
- *PreloadAsync on a client* checks a `MeshId` is a mesh: a PNG reported
  `Failure`, mesh files of versions 1.00 and 2.00 `Success`.
- *Meshes under other parts:* a MeshPart ignores mesh children (a Sphere
  under one left its own mesh drawn). On a union, a mesh child is drawn as
  if Size and `Scale` were 1: a Sphere mesh and a Brick mesh at Scale
  (2, 3, 1) each drew a 1-stud shape, untinted, on 4 × 4 × 4 and 8 × 2 × 2
  unions. A truss treats a mesh child like any part (a Sphere filled a
  2 × 10 × 2 truss; a head.mesh FileMesh drew at its own size).
- *Version 1.00 meshes:* `fonts/sword.mesh` and version 1 of asset
  12221462 drew at the size `rbx_mesh` gives them (half the stored
  positions), within 2 px at 26-50 px per stud. The V flip only shows with
  textures (step 4).

*Compared with Studio* (silhouette masks as in step 1, Studio captures
against `rook-player --screenshot --fov 20` from the same cameras, 1270 ×
549): a test scene (the off-centre mesh front and top, the sword, the head
with Offset, a MeshPart at Size ≠ `MeshSize` and one without
`InitialSize`): every outline within 2 px, 4 px on the sword's tip at 3.9
px per stud. A 2017 disaster place, its maps loaded into Workspace by the same script
in both: a beach map's 8 umbrellas (top within 1 px), a fort's
8 rotated missiles (top within 2 px) and Coastal Quickstop's lighthouse lamp
(within 1 px) and buckets; a modern core place's Christmas tree, 54 rotated
ornaments from front and top. Where masks differ by more (up to 12 px) it
is on sub-pixel fins, antlers and wires, which Studio's antialiasing fades
below the mask's threshold. A modern core place's MeshParts couldn't be compared:
their meshes need an account.

*Performance* (release builds, base = the previous commit): `rook
run-headless --seconds 10` gave identical output and time (a modern core place 20.73
s against 20.75 s, a disaster place 0.36 against 0.37, a 2008 place 0.27 against 0.28).
`rook-player --bench 300`: a disaster place and a 2008 place unchanged (0 and 1 mesh files;
render 1.49 against 1.47 ms); a modern core place draws 86 mesh files and its frame
grew by about 2.8 ms (upload 3.6 to 4.9 ms, render 11.8 to 12.9 ms).

*Left for step 4 and later:* textures on meshes (`TextureId`, MeshPart
`TextureID`), `VertexColor` and per-vertex colours, materials and the
checker on empty MeshParts; unions' own geometry (step 5); MeshPart collision and
mass per `CollisionFidelity` (the loader can now hand physics a mesh);
CharacterMesh (with characters); mesh versions 6.00 and 7.00 if they turn
up; meshes that need an account.

**Textures and decals (step 4)**, built 2026-10-13. How images load and
draw is in `docs/architecture.md` §8 "Rendering" and "Content"; what's
still partial is in `docs/behavior/stubs.md`.

*What the archive uses* (`rook corpus-scan`, 3,058 files that load; the
scan now counts looks too):

| What | Files | Uses |
|---|---:|---:|
| Decal on a part | 1,403 | 192,261 |
| Decal on a part with Transparency 1 | 596 | 63,030 |
| Decal faces: Top, Front, Back, Right, Left, Bottom | | 48,407, 44,808, 40,926, 31,517, 23,945, 23,205 |
| Decal `Transparency` not 0 / `Color3` not white | 208 / 29 | 10,729 / 3,926 |
| Decal `ZIndex` other than 1 | 6 | 10 |
| Texture on a part | 459 | 265,749 |
| Texture `StudsPerTile`: 2 × 2 (the default), 4 × 4, 1 × 1, 8 × 8, ... | 209, 167, 161, 174 | 104,721, 39,085, 37,399, 12,006 |
| Texture `OffsetStudsU`/`V` not 0 | 6 | 4,599 |
| SpecialMesh FileMesh with a `TextureId` | 1,620 | 56,965 |
| SpecialMesh `VertexColor` not (1, 1, 1) | 673 | 19,433 |
| MeshPart with a `TextureID` | 30 | 7,721 |
| Surfaces: Studs (1.71 million on Top), Inlet (1.94 million on Bottom) | 2,307, 2,305 | 1,775,393, 2,001,938 |
| Surfaces: Weld, Universal, Glue, Hinge, Motor, SteppingMotor | 1,726, 364, 526, 863, 509, 8 | 1,588,989, 217,677, 54,308, 51,966, 9,794, 332 |
| SmoothNoOutlines; the old value 9 | 214; 28 | 844,528; 5,343 |
| Material Plastic, or none saved (old files: Plastic) | 1,701; 939 | 3,170,633; 939,990 |
| Wood, Slate, Concrete, Grass, Ice, DiamondPlate, CorrodedMetal, SmoothPlastic | 531, 502, 366, 358, 353, 351, 252, 201 | 264,342, 544,028, 516,853, 40,902, 18,674, 169,646, 96,308, 227,653 |
| Foil, Brick, Sand, Granite, Marble, Metal, Pebble, Fabric, Neon | 177, 158, 149, 127, 125, 110, 105, 101, 84 | |
| `Reflectance` above 0 | 1,405 | 377,042 |

*Image formats* (`rook content --place`, which now decodes every image
reference and totals the results by format, over 60 random archive places
and a 2016 core place, anonymous): of the images that load, 648 distinct are
PNG (1,926 uses), 16 JPEG, 1 BMP (10 uses) and 2 DDS; none is a GIF or
TGA, and none failed to decode. 996 distinct images (12,235 uses) are
refused without an account. The install's own textures are DDS (DXT1 and
DXT5), including the `.tex` sky files. So Rook reads PNG, JPEG, BMP and DDS.

*Checked in Studio 0.742* (2026-10-13, a labelled UV-grid test image with
an arrow and a corner marker, generated for the test; observed unless
marked):

- *Decal faces:* seen from outside, an image is never mirrored. On the four
  sides its up is the part's +Y; on Top and Bottom its up is +Z (towards
  Back). It is stretched over the face.
- *Round and sloped shapes:* a Decal is projected straight along its face's
  axis onto the part's box, over the points whose normal is closest to that
  axis: a Front decal covers the front of a ball, a cylinder's curved side
  shows its Front, Top, Back and Bottom decals, and on a 45-degree wedge
  slope the Front decal wins over the Top one. The same on a SpecialMesh
  Sphere and on a MeshPart (projected onto the mesh's box).
- *Decals on file meshes:* on a SpecialMesh FileMesh, a FileMesh and the
  Head, a Decal is drawn through the mesh's own UVs over the whole mesh,
  whatever its `Face` (a Top decal on a cube mesh showed on every face). The
  head mesh's UVs map only its front quarter (projected straight on), and
  the rest of it to UV (0, 0), so a face decal's top-left pixel covers the
  back of the head: transparent for a real face, the black corner marker of
  the test image. Measured from the user's `head.mesh` for Rook's own head:
  the 100 vertices facing within 45 degrees of -Z have UV
  (0.5 - x / 2R, 0.5 - y / 2R), the other 417 (0, 0), with seams.
- *Decal properties:* `Color3` multiplies the image, `Transparency` fades
  it, and a higher `ZIndex` draws on top. With equal `ZIndex` the order
  wasn't creation order (the same image won either way round). A Decal on a
  part with Transparency 1 still shows, and on a half-transparent part it is
  drawn whole, not seen through the part. Defaults (conformance case
  `content/decal`): Face Front, Color3 white, Transparency 0, ZIndex 1;
  Texture StudsPerTile 2 × 2, offsets 0; SpecialMesh `VertexColor` 1, 1, 1.
- *Textures:* tiled from the face's top-left corner (on a 5 × 3.5 face with
  2 × 1.5 tiles, whole tiles at the top left and part tiles at the right and
  bottom); the image coordinate is (studs + `OffsetStudsU`/`V`) /
  `StudsPerTile`, so an offset of 0.5 moves the image 0.5 studs left.
- *Mesh textures:* `TextureId` draws only on file meshes (SpecialMesh
  FileMesh and FileMesh, including the empty-`MeshId` box, which shows the
  image on every face); a Head, Sphere or Brick SpecialMesh ignores it. The
  texture replaces the part's colour, `VertexColor` multiplies it (red made
  green and blue black), and its alpha is ignored. `VertexColor` on a mesh
  without a texture, and per-vertex colours in a 2.00 mesh, changed nothing.
  MeshPart `TextureID` draws the same way (a MeshPart loaded from a test
  model).
- *Version 1.00 UVs:* a cube mesh written as 1.00 and as 2.00 with the same
  UV numbers drew upside down relative to each other, so 1.00's V is flipped
  (as `rbx_mesh` does): step 3's open question is settled.
- *Setting `TextureId` or `MeshId`* (non-empty) on a SpecialMesh sets its
  `MeshType` to FileMesh, which scripts see (conformance case
  `content/decal`). Setting `Texture` on a Decal or Texture (or a Decal's
  `ColorMap`) to any lower-case `http://` or `https://` URL with a non-empty
  `id` query value stores `rbxassetid://` and that value as written, whatever
  the host or path (`example.com/foo?id=5`, `asset.aspx`), dropping
  `version`; an upper-case scheme or a leading space is kept as it is. Step
  2 had seen only the lower-case roblox.com form.
- *Loading and failure:* while a Decal's new image loads it draws nothing (it
  doesn't keep the old image); a mesh whose new `TextureId` is loading is
  drawn black; a failed or empty `TextureId` leaves the mesh in the part's
  colour. A failed Decal image draws nothing (step 2). A capture caught the
  loading state by queueing 1,200 thumbnail requests first.
- *Surfaces:* drawn only on Plastic: Studs, Inlet and Universal, and one
  diagonal lattice for both Weld and Glue, at one stud per stud, tiled from
  each face's top-left corner like a Texture. SmoothPlastic and the 2022
  materials show none. Hinge, Motor and SteppingMotor draw a small yellow
  pin instead (SmoothPlastic too). On balls, cylinders, wedge slopes and the
  Sphere and Cylinder SpecialMeshes the patterns follow the surface (more
  studs across a ball than a projection gives); a BlockMesh (at any
  `Scale`), a CylinderMesh, file meshes and MeshParts show none, while a
  Brick SpecialMesh keeps them (checked after release, when a modern core place's
  BlockMesh parts showed studs in Rook but not in Studio).
- *Where the textures live:* Studio's install holds only Plastic's material
  textures (`PlatformContent/pc/textures/plastic/diffuse.dds`, `normal.dds`,
  `normaldetail.dds`) and `PlatformContent/pc/textures/studs.dds`: a
  128 × 2048 DXT1 strip of sixteen 128-pixel tiles, four variants each of
  studs, the lattice, inlets and universal, each tile 2 × 2 studs, grey
  around 128 with lit bevels (Plastic's `diffuse.dds` has the same layout,
  flatter). The 2022 materials' textures (Wood, Brick, Grass, ...) aren't in
  the install: they appeared a few seconds after the parts were made, so
  Studio downloads them at runtime, from a source the install doesn't name.
- *Neon* draws unlit at full colour; the empty-`MeshId` box with Neon draws
  lit, without the glow. *Reflectance* tints a part towards its
  surroundings (a red part at 0.2 looked pink, at 0.5 lavender, under a blue
  sky) and still shows its studs.
- *Empty MeshPart:* a new one drew its plain box, with no checker (step 3
  had seen a checker once).

*Roblox's round primitives* (measured afterwards from Studio's OBJ export
of a Ball and a Cylinder part; the export itself is not kept): a Ball is a
cube whose six faces are each split into a 6 × 6 grid of squares, every
point pushed out onto the sphere, each face with its own vertices (294
vertices, 432 triangles); a Cylinder's round side is a square split into 6
segments per side (24 in all, at atan(1/3), atan(2/3), 45 degrees, ...)
pushed out onto the circle, with flat ends (114 vertices, 96 triangles);
normals are smooth. Their UVs map each patch linearly from the box face it
came from, 1/8 of a unit per stud, starting at each face's top-left corner
(a block's faces likewise). So a decal on a ball or cylinder covers its
face's whole patch, out to the 45-degree seams; Rook's earlier projection
straight along the face's axis reached only 85% of the way there, which
stretched the image across a CylinderMesh barrel by about 13%. Rook's Ball
and Cylinder are now built from these rules, each vertex keeping its box
point and face for images and surfaces, and the barrel's face lines up
with Studio's to a pixel.

*Compared with Studio* (`rook-player --screenshot` against Studio's
`screen_capture`, same cameras, field of view 20, one test scene run by the
same script in both, the test files given to Rook through
`ROOK_CONTENT_DIRS`): decals on every face of a block and on a ball,
cylinder, wedge, corner wedge, Sphere mesh, cube mesh and both heads; mesh
textures in 2.00 and 1.00, `VertexColor`, the empty-`MeshId` box; ZIndex,
tint, decals on invisible and half-transparent parts; Textures with
non-integer tiling and offsets; surfaces on Plastic and SmoothPlastic, and
Neon. Placement, orientation and tiling match to the pixel by eye, apart from
lighting. A 2017 disaster place, one of its maps loaded into
Workspace in both: the chairs' weld lattice and studs, the studded Plastic
and the decals match; Wood and Concrete are plain in Rook. A 2016 core place
from its saved camera: the blast-door decals, the logo, the warning signs
and the floor grate Texture match, once fetched with an Open Cloud key (see
below), and the doors' Reflectance gives Studio's pink.

*Asset delivery with an Open Cloud key*, first tried here: with
`ROOK_OPEN_CLOUD_API_KEY` set, a 2016 core place resolved 544 of its 704
distinct references (4,742 of 5,304 uses); the other 89 are refused to
anyone ("User is not authorized", "Asset is not approved for the
requester") or don't exist. A cached anonymous refusal no longer stops the
key being tried.

*Performance* (release builds, base = the previous commit, one run each):
`rook run-headless --seconds 10` gave identical output on a 2016 core place,
a disaster place and a 2008 place (15.05 against 14.98 s, 0.50 against 0.46 s, 0.45
against 0.48 s). `rook-player --bench 300`: a disaster place (3 images) and a 2008 place
(5 images) unchanged (render 1.49 against 1.53 ms, 1.71 against 1.71 ms);
a 2016 core place now draws 3,473 more entries (its decals and Textures) and
310 images, and its render grew from 3.2 to 5.6 ms. The windowed player
(`--uncapped --frame-times`, from the saved camera): a disaster place 0.32 ms median
both, a 2008 place 0.39 ms both, a 2016 core place 0.75 to 1.22 ms (1,050 to 655
FPS), no frame over 16.7 ms in any.

*Left for later:* the 2022 materials' textures (not in the install; Rook
draws them plain), Plastic's own material texture and normal maps; surface
patterns that follow round and sloped surfaces, and the Hinge and Motor
pins; the sky's sun, moon and stars (skyboxes are drawn: 920 files have a Sky, each image placed as Studio places it, measured with six labelled images; Left is at +X and Right at -X, Up's top towards +X, Down's towards -X);
SurfaceAppearance and MaterialVariant (10 files); CharacterMesh textures
(with characters, M5); the order of decals with equal `ZIndex`;
`rbxthumb://` images.

**Trusses**, built 2026-10-13 (TrussPart is in 371 archive files, 31,487
uses; until now each drew as a solid grey box). How Rook builds and
batches them is in `docs/architecture.md` §8 "Rendering"; what's left is in
`docs/behavior/stubs.md`.

*Measured from Studio's OBJ export* of a 2 × 2 × 2 and a 2 × 4 × 2 truss
(the exports are kept outside the repository; Confirmed): bars are 0.25
studs square. Four rails run the truss's whole length at its corners, flush
with its 2 × 2 box. The truss is cut into 2-stud segments; each has a rung
across each of its four sides at both ends, between the rails, and where
two segments meet their rungs are one bar 0.5 studs thick. A brace is 0.25
studs wide and 0.125 deep, in the plane of the rails' centres (0.875 from
the axis), running from one corner of its segment 0.875 below its middle
to the opposite corner 0.875 above it, its ends cut square and buried in
the rails (its outline reaches 0.9634 studs from the segment's middle).
Faces hidden where bars meet are left out and normals are flat. The 2 × 2
× 2 export has no braces (48 quads: the 12 edges of its box); the 2 × 4 ×
2 has 96 quads, 32 of them braces. The UVs are planar at 1/8 of a unit per
stud into the surface-pattern strip (`Truss1_diff.png` is the studs strip),
which Studio doesn't draw on trusses; the install has no truss texture
(only the Insert Object icons), so Rook draws the bars in the part's
colour and material.

*Observed in Studio 0.742* (2026-10-13, a dark block inside each truss so
only its near side showed, front and side views):

- *Style:* AlternatingSupports braces all four sides of every segment, the
  first segment from the bottom leaning one way round the truss and the
  next the other (checked on 2 to 5 segments). BridgeStyleSupports braces
  only the truss's two X sides, the lower floor(n / 2) segments leaning one
  way and the rest the other, the two sides alike seen through the truss.
  NoSupports draws rails and rungs only. Exact directions are in
  `rook-types/src/truss.rs`.
- *The lead that a 2 × 2 × 2 draws like NoSupports and longer ones like
  AlternatingSupports whatever Style says* held only in part: a truss of
  one segment (2 × 2 × 2, 2 × 3 × 2, 2 × 1 × 2) draws no braces whatever
  its Style (Confirmed), but longer ones follow their Style (Confirmed:
  BridgeStyleSupports and NoSupports at 4, 6, 8 and 10 studs drew their own
  look, not AlternatingSupports). The lead about merging turned out to mean
  the rungs inside one truss: where two segments meet they are one 0.5-stud
  bar (Confirmed by the export). Separate trusses never merge (Confirmed:
  side by side, end to end and at a right angle, two trusses in two colours
  each drew all their own bars).
- *Length:* floor(length / 2) segments, at least one; the drawing starts at
  the box's low end along the truss's axis and the rest of the box stays
  empty (2 × 5 × 2 draws 4 studs from its bottom). A truss shorter than 2
  draws one segment centred on the part.
- *Axis:* a truss runs along the axis that isn't 2 studs. Long in Z, its
  own X stays X and its own Z points down; long in X, its X points to the
  Front and its Z down (found from which sides carry braces and how they
  lean).
- *Size* (script-visible; conformance case `content/truss`): each axis is
  kept between 0.001 and 512. When two axes are exactly 2 the third keeps
  its length, shorter than 2 or not; otherwise the longest axis (X before Y
  before Z on a tie) keeps its length, at least 2, and the others become 2:
  (3, 3, 3) reads back (3, 2, 2), (6, 2, 10) (2, 2, 10), (1, 1, 1) (2, 2,
  2), (2, 1, 2) stays. The Size before doesn't matter.
- *Looks:* Color, Transparency, Neon and Reflectance show as on any part,
  and Wood shows its wood texture (Rook draws it plain, like every 2022
  material). Decals, Textures and surface patterns don't show. A Sphere
  SpecialMesh still replaces the truss (step 3), but Studio cuts a grid of
  holes through it; Rook draws it solid.
- *Files:* `Style` is saved as `style`, which the reflection database holds
  as a separate property, so Rook read every saved truss as
  AlternatingSupports until scripts' `Style` became a view of it (found on
  a 2016 core place, whose 596 trusses save 517 AlternatingSupports, 77
  NoSupports and 2 BridgeStyleSupports).

*Compared with Studio* (`rook-player --screenshot --fov 20` against
`screen_capture`, 1270 × 795, one test scene run by the same script in
both): each Style at 2 × 2 × 2 and 2 × 10 × 2, long trusses along X and Z
in two Styles, 2 × 5 × 2, 2 × 1 × 2, a red BridgeStyleSupports 2 × 7 × 2, a
Neon truss tilted 30 degrees and a truss with a Sphere mesh, from the
front, from above and at an angle: every bar's placement and every brace's
direction match by eye, apart from lighting and the Sphere's holes.
A 2016 core place draws its 596 trusses, its NoSupports ladders without
braces.

### M5 Characters

Needs M3 (physics) and M4 (the head and body meshes). Humanoid 903 files,
Tool 1,150, SpawnLocation 1,507, Seat 640.

- Spawning, Humanoid (state machine, walking, jumping, climbing, seats,
  health and death, joint breaking), tools.
- Default camera and control scripts (Core-Scripts, Apache-2.0) and Rook's
  own Animate.
- Network ownership of a player's character, so its client simulates it.

### M6 GUI

ScreenGui 598 files, SurfaceGui 138, BillboardGui 227. Layout (`UDim2`,
`AnchorPoint`, `AutomaticSize`, the UI layouts and constraints), text
(cosmic-text), images, `ClipsDescendants`, ScrollingFrame and GUI input.

### M7 Audio

Sound is in 1,282 files. Playback, `TimePosition`, `Ended`/`Loaded`,
3D sound, SoundService and SoundGroups. Today Sound only keeps its state.

### M8 Networking

A real transport behind the in-process replicator's interface, a headless
server and several clients.

### M9+ Later

Terrain (including legacy `ClusterGrid` data; 975 files), animation
(Animation 269 files), effects (Fire 255, Smoke 249, Sparkles 303,
ParticleEmitter 84), lights (PointLight 226, SpotLight 210) and advanced
lighting, antialiasing (MSAA by quality level, measured in
[`docs/behavior/antialiasing.md`](docs/behavior/antialiasing.md)), Studio
tooling, and opt-in extensions.

## Content and assets

Rook ships its own defaults. Files from Roblox's client (meshes, textures,
shaders) are never committed. They're used two ways: as a reference that
someone measures, with only the facts written down and Rook's own version
built from them; or loaded at runtime from the user's own Roblox install
or downloaded content packages.

## Out of scope or undecided

- **Behaviour modern Roblox no longer has** is out of scope unless added
  later as an opt-in extension. Old content works as far as modern Roblox
  still runs it.
- **HopperBin** (1,107 files): modern Roblox no longer runs it. Decide
  before M5 whether to follow modern behaviour or offer an extension.
- **Wire compatibility with real Roblox clients** is not a goal;
  replication is Rook to Rook.

What's partly done or accepted but ignored today is listed member by member
in [`docs/behavior/stubs.md`](docs/behavior/stubs.md).
