# Asset fetching

Checked against Studio 0.742 on 2026-10-18.

What Rook found while building [M4](../../roadmap/m4-content.md) step 2: where archived places' content comes from, and how Rook fetches it.

How it works is in
[architecture](../../design/architecture/content.md); what's still partial is in
`docs/guide/stubs.md`.

*What the archive uses* (`cargo xtask corpus-scan`, 3,058 files that load): 1.10
million non-empty content property values (36,938 distinct), plus 1.69
million empty ones. By what `rook-content` makes of them:

| Kind | Files | Uses |
|---|---:|---:|
| Asset id (`rbxassetid://` or an asset URL) | 1,803 | 990,851 |
| Asset id with `version=` | 657 | 5,615 |
| `rbxasset://` path in today's install | 324 | 5,491 |
| `rbxasset://` path of the 2009 client's 2009-06-24 upload (all 262, counted before the table below took Studio's 200) | 1,516 | 77,222 |
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
other paths, and no scheme. In `rbxasset://` paths, backslashes and
doubled slashes are fine and case doesn't matter. `version=` is honored: version 1 of asset
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
light-colored part.) So the old client seems to have shown a placeholder
for files it didn't recognize as images at all; modern Roblox draws
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

*The 2009 client's paths* (observed 2026-10-28, Studio 0.744): modern
Studio loads `rbxasset://textures/SpawnLocation.png`,
`sky/null_plainsky512_bk.jpg` and `fonts/sword.mesh` although no install
has them, by fetching a public asset for each. Which asset:
- *The uploads.* Public asset details (`economy.roblox.com/v2/assets/<id>/details`)
  over the ids near asset 12221720 show that on 2009-06-24 Roblox (user 1)
  uploaded every file of the 2009 client's `fonts`, `music`, `sky`,
  `sounds` and `textures` folders as an asset whose description is its old
  path, in alphabetical order (ids 12220916 to 12224353, 262 files), and
  13 more on 2009-06-25 (12229442 to 12229518, `textures/PaintballIcon.png`,
  `TrowelTexture.png`, `J_Key.png` and others). On 2017-07-11 and 12 it
  uploaded some of the sounds and cursor textures again, named by file
  without the extension (`bass` 911769342 to `wrench` 911894116).
- *What Studio fetches.* `ContentProvider:GetAssetFetchStatus("rbxassetid://<id>")`
  turns from None to Success when Studio fetches an asset for any reason,
  an old path's included. Loading each old path alone in a Studio that
  hadn't loaded any (one `PreloadAsync` per path, a SpecialMesh, Sound or
  Decal by extension) and checking its candidates: 155 of the 2009-06-24
  paths fetched their 2009 asset, 35 their 2017 one (every 2017 sound and
  cursor, `textures/CloneOverCursor.png` the one whose public name is
  filtered), and 8 of the 2009-06-25 paths theirs (`HopperPanel.png` the
  first of its two). The bytes agree: Studio's cached `fonts/sword.mesh` is
  asset 12221720's file byte for byte, every mesh's face count matches its
  asset's, 133 images match their asset pixel for pixel (2 JPEGs within
  decoding, `Snowflake.png` in size and alpha), and the sounds' CDN hashes
  in `rbx-storage` are their assets'. When a fetch fails, Studio's Output
  names the asset (`ContentId: rbxassetid://911864050 failed due to: Asset
  is not approved for the requester` for `sounds/swoosh.wav`).
- *What it doesn't.* 18 paths are still files in the install
  (`textures/face.png`, `ArrowCursor.png`, `FlatCursor.png` and other
  cursors, `sky/sun.jpg`), and Studio draws those, not their assets, which
  differ. 53 fail to load though their 2009 asset is deliverable: the seven
  `.rbxm` models, `music/bass.wav`, `sounds/pageturn.wav`, `swoosh.wav`,
  both `Rubber band` sounds, and 41 textures (the `Camera*`, `Chat*`,
  `ReportAbuse*`, `Surfaces*` and `nosurfaces*` families, the `.dds` files,
  `GameTool.png`, `script.png`, `WoodFallback.png` and a few more).

Rook's table (`rook-content/src/resolver/defaults.rs`) holds the 200 paths
Studio fetches an asset for, with that asset; the rest resolve to the
install or fail, as in Studio. `sounds/snap.wav` and `uuhhh.wav` load in
Studio but fetched neither their 2009 nor any 2017 asset, so the 2009 one
stands in (Inference: the asset Studio fetches for them would settle it;
`snap.wav`'s CDN hash in `rbx-storage` is `sc0/82bd55bb...`).
`textures/rocket.png`, `rocketboots.png`, `fonts/headf.mesh` and
`paintballgun.mesh` load in Studio from assets not found yet, so Rook
can't load them.

*PreloadAsync* (observed on a playtest client, a server and in edit mode;
the server part is the conformance case `content/preload`): on a client it
yields, calls back `(contentId, Enum.AssetFetchStatus)` for each entry in
the order they finish, and returns nothing after the last; failures that
need no download came within a few frames. Instances count
with their descendants (a Model holding a Decal reported the Decal's
texture). Each property loads as what it holds (2026-10-13): a
SpecialMesh whose `MeshId` was a PNG reported `Failure`, while mesh files
(versions 1.00 and 2.00, by path or asset id) reported `Success`; a mesh,
a sound or a damaged PNG in a Decal's `Texture` reported `Failure`.
Strings load as images: `rbxasset://` images succeeded, `.mesh`
and `.wav` paths failed, and every `rbxassetid://` or asset URL string
failed, even an image that loaded as a Decal's texture. Empty strings are
skipped; a repeated entry gets a callback each time. On a server it never
returned (over 30 s) once anything was to be fetched, even `"abc"`, and
returned at once for `{}`, `{""}` or a Folder. Argument errors: `Unable to
cast string to Array`, `Unable to cast double to ContentId`, `Unable to
cast double to Function`. `RequestQueueSize` read 0 on an idle server and
rose to 5-6 during a client preload, not tracking its entries exactly.

*Published models* (observed in Studio 0.742 on a playtest server and
client, 2026-10-18): `require(id)` loads the asset into a Model named
`required_asset_<id>` with no parent and requires its child `MainModule`
(so the module's full name is `required_asset_<id>.MainModule`, and
`script.Parent.Parent` is nil). Each id loads once: requiring it again
returns the same value at once, and a failure is remembered as well (the
second call failed in no time with the same message). A missing asset,
one that isn't a model, or an id of 0 or below raises `Downloading asset
failed for asset id <id>.  Is the asset id correct and is the asset type
"Model"?`; a model without `MainModule` raises `Unable to find module for
asset id <id>.  Does the asset have a ModuleScript named "MainModule"?`.
A fractional id rounds to the nearest, and a string is Luau's path
require, not an asset id. `InsertService:LoadAsset(id)` returns a new
parentless Model named `Model` holding the asset's top-level instances, a
fresh copy every call; it takes numeric strings (`Unable to cast string
to int64` otherwise), a missing asset raises
`Request asset was not found`, and Studio refused assets the place's
creator doesn't own (`User is not authorized to access Asset.`, `Asset is
not trusted for this place`). On a client both refuse: `require(assetId)
cannot be called from a client.  assetId = <id>` and `InsertService cannot
be used to load assets from the client`. Both wait for the download (0.2
to 0.4 s in Studio). Rook does all of this, fetching through the same
providers and cache as other content and reading the file with
rook-formats; it doesn't check ownership, so it loads whatever asset
delivery gives it. Asset delivery is the limit: of the archive's modules
tried, three widely used ones
now need an account, so `require` fails with the download message
Studio gives for a missing asset, while an older one
loads and runs. A playtest server has a `NetworkServer` under the DataModel (parent locked, holding a ServerReplicator) and a client a `NetworkClient` (holding a ClientReplicator), each without the other; Rook creates both, without the replicators. A second `require` of a module still loading on another thread waits and gets the same value. Headless runs leave asset delivery off
unless `ROOK_ASSET_DELIVERY=on`; the player has it on.

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
and tries a user's key (`rook key set`) only for refused assets. Tested
on 2026-11-23 with a key holding only `legacy-asset:manage`: it opens other
creators' public images that anonymous requests are refused ("Authentication
required"), and the upload it gives for one of them, a 4096 × 4096 decal,
is a 1024 × 1024 PNG; its transcode is 4096 (see Textures below). Roblox's own 2009 assets (the table
above) come anonymously.

*Results* (`cargo xtask content --place`, first run, network on):

| Place | References (distinct targets) | Install | Asset delivery or cache | Refused anonymously | Not found or no content |
|---|---|---:|---:|---:|---:|
| A 2017 place | 198 (102) | 5 (3) | 141 (73) | 51 (26) | 1 (1) |
| A large modern place | 9,284 (1,655) | 229 (18) | 1,440 (418) | 7,610 (1,215) | 5 (5) |

Most refusals are "Authentication required" (a large place: 7,471 uses, 1,141
assets). A second run needs no network: a large place resolved all 1,655 targets
from the install and cache in 0.2 s. The player draws the Head MeshType
with the install's `head.mesh`, and `cargo xtask run-headless --seconds 10` on a large modern place, a small place
and an early place gave identical output in the same time as before (a large place
22.3 s against 22.5 s, four alternating runs each).

*Mesh files* are read by Rook's own decoder, versions 1.00 to 7.00
([layouts](../meshes/file-meshes.md#file-layouts)); Rook keeps every
level of detail ([file meshes](../meshes/file-meshes.md#levels-of-detail))
and checks face indices. `cargo xtask content` decodes meshes it shows: `fonts/sword.mesh` (1.00)
is 198 triangles, 1.15 x 0.34 x 4.37 studs; version 1 of asset 12221462
(1.00) is 56 x 40 x 45 studs, the giant mesh Studio drew for it.

## Textures

Checked against Studio 0.748 on 2026-11-23, by logging a second Studio's
requests (`FLogContentProviderRequests` and `FLogTextureManager2Residency`
in a local `ClientAppSettings.json`, removed afterwards) and asking asset
delivery the same questions.

Studio never fetches an image's upload (`source`) to draw it. It asks
`GET assetdelivery.roblox.com/v1/asset?id=<id>&contentRepresentationPriorityList=<list>&doNotFallbackToBaselineRepresentation=true`,
where the list is base64 of
`[{"format":"ktx2","majorVersion":"8","fidelity":"/////AE="}]`
(Studio sometimes adds a second entry, major version `7rdo`).
`POST /v1/assets/batch` takes the same `contentRepresentationPriorityList`
and `doNotFallbackToBaselineRepresentation` per item and answers each with
a `location` and the `contentRepresentationSpecifier` it matched, so
several can be asked at once; `/v2/assetId` ignores the list and always
answers `source`. Asked anonymously, it answers for the images `source`
does. Open Cloud's `asset-delivery-api/v1/assetId/<id>` (with a key) takes
`contentRepresentationPriorityList` and
`doNotFallbackToBaselineRepresentation` in its query and answers as a batch
item does, one pack a request.

What comes is a KTX2 file of BC1 (`vkFormat` 131, opaque) or BC3 (137)
blocks, each level Zstandard-compressed, smallest level first, with
key-value data: `RobloxOriginalWidth`/`Height` (the upload's size),
`transcodedWidth`/`Height`, `contentHash`, `acrVersion` 8, average colors,
`constantColor` (a flat image is transcoded to 4 × 4), `packIndex`, and
`pack_<n>` = `first,count` for each mip pack. The texture is split in
packs, each its own file on the CDN: the `fidelity` is base64 of
`FF FF FF FC` and the pack plus one (`/////AE=` pack 0, `/////AI=` pack 1,
...); other masks are refused, apart from one that answers ETC2 for
mobile GPUs. A 4096 × 4096 upload (BC3) comes in six:

| Pack | `fidelity` | Levels | Largest |
|---:|---|---|---:|
| 0 | `/////AE=` | 7 to 12 | 32 |
| 1 | `/////AI=` | 4 to 6 | 256 |
| 2 | `/////AM=` | 3 | 512 |
| 3 | `/////AQ=` | 2 | 1024 |
| 4 | `/////AU=` | 1 | 2048 |
| 5 | `/////AY=` | 0 | 4096 |

A 1024 × 1024 one comes in four (64 and below, 256 and 128, 512, 1024), a
256 × 256 one in two. Sizes that aren't powers of two are rounded down for
the transcode (a 1264 × 632 upload is 1216 × 576). Studio fetches pack 0
first, then larger packs only as the texture grows on screen
([streaming](#streaming)). An image not transcoded yet answers 406
"Asset content representation is being generated" (`customErrorCode` 21)
and Roblox starts making it: Studio retries after two seconds, then draws
the upload (`[{"format":"Original","majorVersion":"Original","fidelity":"Original"}]`,
up to its full size: one was a 2048 × 2048 JPEG). A pack the texture
doesn't have answers the same 406; a mesh asked for as a texture, 404.

Only images on surfaces in the world come this way: Studio asked for
transcodes for Decals, mesh textures and ParticleEmitter textures, and for
the upload (`/v1/asset?id=<id>`, nothing more) for ImageLabels and Sky
faces, which it drew whole at once, without streaming.

Rook does the same for any asset id it loads as a texture
(`Want::Texture(n)`: Decals, Textures, mesh textures, effects and
adornments): pack 0 through the batch endpoint, then the larger packs it
needs, several in one batch request, joined into one KTX2 file of their
levels and drawn in its blocks. Each pack is cached on its own
(`asset-<id>-ktx2-p<n>`), so a texture growing on screen fetches only the
packs it adds. Refused anonymously, with a key saved, it fetches each pack
from Open Cloud instead (a 4096 texture's six in about 4 s). GUI images,
the sky and SurfaceAppearance maps come as uploaded (`Want::Image`),
fitted to 1024 × 1024 with the aspect kept, the size GUI measures them in
(below). When there is no transcode it falls back to the upload as
before: a refusal or an image that isn't one at once, remembered like
other failures; a transcode being made after asking once more two seconds
later, as Studio does (the loader puts it back in its queue meanwhile
rather than holding a fetcher), and it's asked for again next run. Kept assets
stay as kept. An upload drawn on a surface is decoded up to 4096 a side,
and kept in blocks past 1024.

GUI measures `ImageRectOffset` and `ImageRectSize` in neither the
texture's size nor the upload's, but the upload's fitted to 1024 × 1024,
the aspect kept (Studio 0.748, ImageLabels of 400 × 200 or 300 × 300 over
a magenta background): with a 4096 × 4096 upload a 512 × 512 rect showed
its top-left quarter, and with a 1264 × 632 upload (served 1216 × 576) a
1024 × 512 rect showed the whole image while 632 × 316 showed part of it.
Rook halved images past 1024, which measured that one as 632 × 316. That
size is the upload's as Roblox stores it: Open Cloud's upload of the
4096 × 4096 decal is a 1024 × 1024 PNG, which is what ImageLabels draw.

## Streaming

Checked against Studio 0.748 on 2026-11-23: a 16 × 16 stud decal of a
4096 × 4096 image in a second Studio logging `FLogTextureManager2Residency`,
the camera (`Scriptable`, 70°, a 1760 × 990 view) stopped for five seconds
at each distance in turn, nearer each time:

| Distance (studs) | Decal across (pixels) | Level Studio wanted | Its size |
|---:|---:|---:|---:|
| 400 | 28 | 6 | 64 |
| 200 | 57 | 6 | 64 |
| 100 | 113 | 5 | 128 |
| 50 | 226 | 4 | 256 |
| 25 | 452 | 3 | 512 |
| 12.5 | 905 | 2 | 1024 |
| 8 | 1414 | 1 | 2048 |
| 5 | 2262 | 0 | 4096 |

So a texture is wanted at the smallest power of two at least as many
pixels across as the surface takes on screen, and no smaller than 64. A
1024 × 1024 decal (its color map, 9920484153) went the same way, to 1024
at 940 pixels. Studio steps one level at a time (`setResidency resMinMip:4
-> reqMinMip:3`), about 0.35 s each, fetching each pack as its first
level is wanted. A texture out of view keeps its smallest pack (both
decals sat at pack 0 while the camera looked elsewhere), and nothing
shrank again: after 40 s far away and then looking away, the 4096 decal
was still whole. Studio lowers textures only under memory pressure
(`ReduceMipLevels`, `TextureStreaming` budget), which this didn't reach.

Rook does the same ([`content/streaming.rs`](../../../crates/rook-scene/src/content/streaming.rs)):
a texture loads at pack 0, and ten times a second the scene sizes on
screen each texture in view that can still grow (a decal's face, a
texture's tile, even one larger than its face, a mesh's texture over the
whole part, seen from the part's nearest point). Each grows towards that
size a level every 0.35 s, fetching the packs a level needs when it gets
there, so a 4096 texture seen up close from pack 0 reaches 4096 in seven
steps, about 2.1 s. A texture growing when it leaves view stops at the
level it reached, and larger levels that fail for now (a network failure,
a pack being made) are asked for again two seconds later. The scene keeps
the entries whose textures can still grow as they're written, and looks
over only those: once a texture holds all its packs, or its entry stops
drawing it, it's left out, and a place with nothing left to grow pays
nothing. The larger version replaces the
smaller in the renderer under the same image, so nothing is placed again.
A screenshot goes straight to the sizes its camera wants. Effects and
adornments load whole. Rook never lowers a texture and has no memory
budget.

Thumbnails, `rbxthumb://` and the old `thumbs/`, `ThumbnailAsset.ashx` and `*-thumbnail/image` URLs, are in [thumbnails](thumbnails.md).

*Not done yet:* the old paths outside
Rook's table, Decal's URL rewrite, and assets that need an account.
