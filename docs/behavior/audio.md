# Audio

What Rook found while building M5. The plan itself is in [ROADMAP.md](../../ROADMAP.md#m5-audio).

## Decoding and playback (step 1)

**Decoding and playback (step 1)**, built 2026-10-14. A Sound's file
loads through the content loader like meshes and images
(`Want::Audio`, decoded by `rook_content::audio` on the loader's workers),
every DataModel keeps `IsLoaded`, `TimeLength` and `TimePosition` as
Studio does, and the player hears what is playing through one output
device and mixer (`rook-audio`).

*What the archive uses* (`reports/corpus/summary.md`): Sound has 735,930
instances in 3,808 files (the ROADMAP's 1,284 files is from an older,
smaller archive); `Sound.PlayCount` 777 files; `rbxasset://sounds/*` paths
by the thousands, most of them the 2009 client's (`sounds/collide.wav` in
1,007 files, `swordslash.wav` 744, `unsheath.wav` 689, ...), plus some no
install has (`sounds/uuhhh.mp3` 345 files, `action_falling.mp3` 337,
`paintball.wav` 256).

*What loads, and in which format* (`rook content --place`, which now
decodes each Sound's file, run once on five places, asset delivery on,
anonymous):

| Place | Sound uses (distinct) | Decoded: Ogg | Decoded: MP3 | Decoded: WAV | Refused anonymously | Not found |
|---|---:|---:|---:|---:|---:|---:|
| A 2021 place | 51 (31) | 42 (24) | 1 (1) | 0 | 8 (6) | 0 |
| A 2021 place | 51 (31) | 42 (24) | 1 (1) | 0 | 8 (6) | 0 |
| A large 2016 place | 503 (154) | 187 (39) | 31 (1) | 0 | 285 (114) | 0 |
| A 2008 place | 23 (23) | 21 (21) | 0 | 0 | 0 | 2 (2) |
| A large modern place | 956 (349) | 218 (75) | 43 (8) | 0 | 693 (264) | 2 (2) |

So: every sound that loads is Ogg Vorbis or MP3, at 8,000 to 48,000 Hz.
Asset delivery serves even the 2009 `sounds/*.wav` paths (through Rook's
table of the assets Roblox uploaded for them, `defaults.rs`) as Ogg. The
install's own `content/sounds` holds 7 MP3s and 4 Oggs and no WAV. WAV
only arrives from a content folder or a local file, and is decoded anyway
since Studio plays it (below). Refusals are the account-only assets of the
newer places ("Authentication required", "not authorized", "archived");
the two not found in a 2008 place are `sounds/Rubber band sling shot.mp3` and
`sounds/paintball.wav`, which no install or upload has.

*Decoders* (Rook's choice): Symphonia 0.5 (pure Rust, MPL-2.0), with only
its WAV (PCM and ADPCM), Ogg Vorbis and MP3 decoders built: one library
whose per-format decoders cover all three, where the alternatives were
three crates of differing upkeep (`hound`, `lewton`, and an MP3 decoder
that is either C, `minimp3`, or unmaintained). The format is picked by the
first bytes, never the name: `sounds/swordslash.wav` holds Ogg.
Everything decodes to one layout, 16-bit interleaved PCM in one or two
channels at the file's rate (half the memory of floats; more channels
fold to two). Damaged files fail as `Invalid` with an error, and a
decoder panic is caught and reported the same way; a file cut off part way
keeps what decoded before the cut, and a single bad frame is skipped
(Approximation: what players do).

*`TimeLength`* (Confirmed: Studio 0.743, 2026-10-14, fresh Studio, edit
mode and a playtest server; conformance case `content/sound`):

| File | Studio | Rook |
|---|---:|---:|
| `rbxasset://sounds/oof.ogg` (Ogg, 22,050 Hz) | 0.332290 | 0.332290 |
| `rbxasset://sounds/action_jump.mp3` (MP3, 44,100 Hz) | 0.339592 | 0.339592 |
| A 1.5 s, 22,050 Hz PCM WAV put in Studio's `content/sounds` for the test, then removed | 1.500000 | 1.500000 |
| `rbxassetid://12222216` (Ogg) | 0.562222 | 0.562222 |

An Ogg's length is its last page's granule position, not the samples its
decoder hands out (whole blocks: 0.354 s for `oof.ogg` before Rook trimmed
to the granule). The WAV case isn't a conformance case, since the file
exists only where it was put.

*Which `SoundId` forms load* (observed): `rbxasset://` paths in the
install, asset ids, and `http://www.roblox.com/asset/?id=` URLs, by the
same parsing rules as meshes ([content](content.md)); a backslashed
`rbxasset://sounds\swordslash.wav` loads too (it reads back with a slash).
An empty or junk `SoundId`, or a missing file, never loads. One difference
Rook can't close yet: Studio loads `rbxasset://sounds/swordslash.wav` as a
0.653061 s sound (14,400 frames at 22,050 Hz, a WAV-like round count),
while the asset Roblox uploaded for that path, which Rook loads, is the
0.562222 s Ogg. Where Studio gets the old file from is unknown (like the
few old paths outside the 2009 upload, [content](content.md)), so Rook's
legacy sounds are those assets' current versions (Approximation).

*Loading* (observed on a playtest server and in edit mode):

- A sound loads once it is in the DataModel, whether or not it plays;
  `IsLoaded` false and `TimeLength` 0 until then. One outside the
  DataModel doesn't load. Rook does the same on servers and clients
  (Inference for the client: the playtest client wasn't checked).
- `Play` before the file has loaded sets `Playing` at once; `TimePosition`
  stays 0 while it loads and starts moving when it loads (an uncached
  asset: loaded after 0.084 s with `TimePosition` 0.018, so playback
  counted from the load, not from `Play`).
- A sound whose file fails (missing, junk, empty) keeps `Playing` true
  forever, with `TimePosition` 0 and `IsLoaded` false, and is silent.
- A sound that reaches its end stops: `Playing` false and `TimePosition`
  0, with `Ended` firing. A `Looped` sound starts again (step 2 has the
  details).
- Changing `SoundId` makes the sound unloaded until the new file arrives
  (Inference).

*Time* (Rook): `TimePosition` moves by the frame's time times
`PlaybackSpeed` on every DataModel, so scripts see time pass in headless
runs too. It isn't recorded as a change, so it fires no `Changed` each
frame and isn't replicated: each side keeps its own clock, as `Playing`
replicates. Headless runs and conformance finish loading a sound file
within the frame that asks for it (`Options::wait_for_sounds`), so
simulated time can't run ahead of loading and runs repeat exactly; the
player loads in the background.

*Hearing it* (Rook): the player gives the playing, loaded sounds of the
DataModel it shows (the client in Play mode) to `rook-audio` each frame.
The mixer resamples each to the device's rate by linear interpolation and
mixes them, following the engine's `TimePosition` but keeping its own
unless the two drift more than 0.25 s apart (a seek). The output device
(`cpal`, the system default) opens on the first sound to hear; `rook
run-headless`, `--bench` and `--screenshot` never open one, and
`ROOK_AUDIO=off` keeps the player silent. Every sound is heard
non-positionally at `Volume` as a plain gain, wherever it is in the tree
(Approximation: positioned sound, `RollOffMode` and which containers are
audible are step 3; for `Volume`'s curve see step 2).

*Costs* (Rook): in Play mode the server and the client each load and
decode their sounds, as each has its own content loader; a decoded sound
is shared by every Sound using the same file.

*Left for later steps:* 3D sound (step 3);
SoundGroups and effects (step 4); the modern audio API (step 5); the 2009
sounds Studio has from elsewhere; sounds that need an account.

## The Sound API (step 2)

**The Sound API (step 2)**, built 2026-10-14. All observed in a fresh
Studio 0.743 on a playtest server (and its client for
`PlaybackLoudness`), mostly on the install's 10 s
`sounds/action_falling.ogg`. The script-visible rules are the conformance
case `content/sound_api`, recorded twice with the same result. The
methods are in `rook-lua/src/api/methods/sound.rs`, the rest in
`rook-engine`'s `sounds/playback.rs`.

*The methods* (Confirmed):

| Call | On a playing sound | On one that isn't playing |
|---|---|---|
| `Play` | Starts over from the start position (below); no `Played` | Plays from the start position; `Played` |
| `Stop` | `Playing` false, `TimePosition` 0, start position 0; `Stopped` | Nothing at all (a paused sound keeps its place) |
| `Pause` | `Playing` false, keeps its place; `Paused` | Nothing |
| `Resume` | Nothing | Plays from where it is; `Resumed` |

The lowercase `play`, `stop` and `pause` are the same methods. Setting
`Playing` directly plays or pauses in place and fires no event. Each
event passes the `SoundId` string and fires before `Changed` for
`Playing`. `Played` fires on `Play` even before the file has loaded, and
for a file that never loads. `IsPlaying` reads `Playing`, and `IsPaused`
the opposite.

*Where `Play` starts* (Confirmed): from the last `TimePosition` a script
set, whatever happened since. A script that set 2 and let it play to 2.5,
set 3 while stopped, or set 7.8 and let it end, gets 2, 3 or 7.8 from
`Play`. With no set (or after `Stop`, which sets 0) it starts from 0, even
straight after a `Pause`. Rook keeps that value per sound
(`World::sounds`, `SoundControl`).

*Seeking* (Confirmed): `TimePosition` reads back what a script set, even
past the end or negative. Past the end, a playing sound ends at once
(`Ended`, stopped and rewound); a negative one plays from the start.

*Ends and loops* (Confirmed): at its end a sound stops, rewinds and fires
`Ended`, and `Changed` doesn't fire for `Playing`. A `Looped` sound wraps
(9.7 s into a 10 s sound, 0.6 s later it reads 0.3), never fires `Ended`,
and fires `DidLoop(soundId, count)`. The count is the loops since the last
`Play` (Inference: one run counted 1, 1, 2, 3 across a `Play` made while
it was already looping).

*`Loaded`* (Confirmed): fires with the `SoundId` once the file has loaded,
after `Changed` for `IsLoaded` and `TimeLength`. It fires again when a
loaded sound leaves the DataModel (`IsLoaded` turns false, `Playing`
stays, time stops) and comes back, and for the new file when `SoundId`
changes. A sound keeps its `TimePosition` across a change of `SoundId`
and goes on from there once the new file loads. A failed file never fires
it. Whether `Loaded` or `Played` comes first for a `Play` right after
parenting depends on Studio's cache (it went both ways), so the case
doesn't check the order.

*`PlaybackSpeed` and `Volume`* (Confirmed): time moves at `PlaybackSpeed`
times real time (1 s at 2 moved 2.03 to 2.13 s), and 0 holds it still
while `Playing` stays true. `Volume` is kept within 0 to 10, and
`PlaybackSpeed` (and `Pitch`, the same property) within 0 to 20;
infinity reads 20. Rook plays faster sounds higher, as resampling does
(Inference). How loud `Volume` makes a sound can't be measured by script,
since `PlaybackLoudness` doesn't change with it, so Rook's plain linear
gain stays an Approximation.

*`PlaybackLoudness`* (Confirmed for a steady tone): a client-only reading
(0 on the server) of the sound's own level, before `Volume` and unchanged
by `PlaybackSpeed`. A 440 Hz tone of amplitude 0.25, put in Studio's
`content/sounds` for the test and removed after, read 176.6 to 177.0 at
every `Volume` from 0 to 10: its root mean square times 1000 (176.8).
Rook computes that over the samples played in the last frame (the window
is Inference) and reads 0 when not playing.

*`PlayOnRemove`* (creator-docs; scripts can't hear it): a loaded sound
with `PlayOnRemove` that leaves the DataModel (destroyed or unparented)
plays once from the start, at the `Volume` and `PlaybackSpeed` it had.
Observed: it doesn't fire `Played`.

*In the mixer* (Rook): every `Play` gives the sound a new generation, so
the mixer starts it over even when it was near the start already. A
client sees a server's `Play` of an already-playing sound only as a seek,
so a restart less than 0.25 s from where it was isn't heard as one
(Approximation).
