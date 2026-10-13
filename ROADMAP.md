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

- **Now:** M4 Content: the built-in mesh shapes and asset fetching are done; file meshes are next.
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
3. **File meshes**: SpecialMesh `MeshId`, FileMesh, MeshPart (78 files,
   126k instances), Roblox mesh versions 1–5.
4. **Textures and decals**: `TextureId`, Decal (1,756 files), Texture (459),
   part materials.

Physics picks up the real shapes and volumes of MeshParts here too
(collision fidelity), replacing today's boxes.

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
| A 2017 place | 198 (102) | 5 (3) | 141 (73) | 51 (26) | 1 (1) |
| A large modern place | 9,284 (1,655) | 229 (18) | 1,440 (418) | 7,610 (1,215) | 5 (5) |

Most refusals are "Authentication required" (a large place: 7,471 uses, 1,141
assets). A second run needs no network: a large place resolved all 1,655 targets
from the install and cache in 0.2 s. The player draws the Head MeshType
with the install's `head.mesh`, and `rook run-headless --seconds 10` on a large modern place, a small place
and a 2008 place gave identical output in the same time as before (a large place
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
lighting, Studio tooling, and opt-in extensions.

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
