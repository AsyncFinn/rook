# Asset fetching

What Rook found while building M4 step 2: where archived places' content comes from, and how Rook fetches it. The plan itself is in [ROADMAP.md](../../ROADMAP.md#m4-content-meshes-textures-decals).

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
