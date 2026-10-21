# Decoding and playback

Checked against Studio 0.743 on 2026-10-14.

A Sound's file
loads through the content loader like meshes and images
(`Want::Audio`, decoded by `rook_content::decode::audio` on the loader's workers),
every DataModel keeps `IsLoaded`, `TimeLength` and `TimePosition` as
Studio does, and the player hears what is playing through one output
device and mixer (`rook-audio`).

*What the archive uses* (`reports/corpus/summary.md`): Sound has 735,930
instances in 3,808 files (the roadmap's 1,284 files is from an older,
smaller archive); `Sound.PlayCount` 777 files; `rbxasset://sounds/*` paths
by the thousands, most of them the 2009 client's (`sounds/collide.wav` in
1,007 files, `swordslash.wav` 744, `unsheath.wav` 689, ...), plus some no
install has (`sounds/uuhhh.mp3` 345 files, `action_falling.mp3` 337,
`paintball.wav` 256).

*What loads, and in which format* (`rook-tools content --place`, which now
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
same parsing rules as meshes ([content](../content/asset-fetching.md)); a backslashed
`rbxasset://sounds\swordslash.wav` loads too (it reads back with a slash).
An empty or junk `SoundId`, or a missing file, never loads. One difference
Rook can't close yet: Studio loads `rbxasset://sounds/swordslash.wav` as a
0.653061 s sound (14,400 frames at 22,050 Hz, a WAV-like round count),
while the asset Roblox uploaded for that path, which Rook loads, is the
0.562222 s Ogg. Where Studio gets the old file from is unknown (like the
few old paths outside the 2009 upload, [content](../content/asset-fetching.md)), so Rook's
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
mixes them. Each sound starts at the engine's `TimePosition` and then keeps
its own clock. It jumps to the engine's position again only when the sound
is played again or a script (or replication) sets `TimePosition`. A sound
played to its end stays silent until then. The engine's clock falls behind
the device's whenever frames are slow (the client steps at most four 1/60 s
frames per window frame). The mixer used to jump back to it once
the two were 0.25 s apart, which stuttered. The output device
(`cpal`, the system default) opens on the first sound to hear; `rook
run-headless`, `--bench` and `--screenshot` never open one, and
`ROOK_AUDIO=off` keeps the player silent. Every sound is heard
at `Volume` as a plain gain (Approximation; see step 2), 3D sounds by
distance and direction (step 3), and through their effects and
SoundGroups (step 4).

*Replication* (Rook): a Sound's `Playing`, `TimePosition` and
`PlaybackSpeed` are NotReplicated in the API dump. Roblox sends them under
hidden twins (`PlayingReplicator`, `TimePositionReplicator`, and the old
name `Pitch`), and a server's `Play` is heard on every client at its
speed. Rook replicates the three properties, and the client keeps its own
clock from there.

*What replicates* (Confirmed in Studio 0.743, Play mode, a server Sound
at Volume 0): a client that set its own `TimePosition` 5 s ahead kept that
offset for as long as the server played on, so playing progress is never
sent. A server seek reached the client (it read the new position plus the
time since), and so did `Play` on a playing sound (a restart from the last
position a script set), `Stop` (0), and a `TimePosition` set while
stopped. Rook sends `TimePosition` only when it is set (time passing
isn't journaled), so a slow server never pulls a client back. Until 0.4.4 the player heard nothing in Play mode, and
then everything at speed 1. A recording of `rook-player`'s output now has
the test tone at full level for exactly as long as the script played it.
Engine tests (`tests/sounds.rs`) check that the client gets every
property it plays by. The rule covers every property replicated this way,
not only sounds (`rook-net`'s `replicated_by_twin`).

*Costs* (Rook): in Play mode the server and the client each load and
decode their sounds, as each has its own content loader; a decoded sound
is shared by every Sound using the same file.

*Not done yet:* the 2009 sounds Studio has from elsewhere; sounds that
need an account.
