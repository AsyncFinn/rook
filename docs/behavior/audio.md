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
| A 2021 disaster place | 51 (31) | 42 (24) | 1 (1) | 0 | 8 (6) | 0 |
| A 2021 disaster place | 51 (31) | 42 (24) | 1 (1) | 0 | 8 (6) | 0 |
| A 2016 core place | 503 (154) | 187 (39) | 31 (1) | 0 | 285 (114) | 0 |
| A classic 2008 place | 23 (23) | 21 (21) | 0 | 0 | 0 | 2 (2) |
| A modern core place | 956 (349) | 218 (75) | 43 (8) | 0 | 693 (264) | 2 (2) |

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
at `Volume` as a plain gain (Approximation; see step 2), 3D sounds by
distance and direction (step 3), and through their effects and
SoundGroups (step 4).

*Replication* (Rook): a Sound's `Playing`, `TimePosition` and
`PlaybackSpeed` are NotReplicated in the API dump. Roblox sends them under
hidden twins (`PlayingReplicator`, `TimePositionReplicator`, and the old
name `Pitch`), and a server's `Play` is heard on every client at its
speed. Rook replicates the three properties, and the client keeps its own
clock from there. Until 0.4.4 the player heard nothing in Play mode, and
then everything at speed 1. A recording of `rook-player`'s output now has
the test tone at full level for exactly as long as the script played it.
Engine tests (`tests/sounds.rs`) check that the client gets every
property it plays by. The rule covers every property replicated this way,
not only sounds (`rook-net`'s `replicated_by_twin`).

*Costs* (Rook): in Play mode the server and the client each load and
decode their sounds, as each has its own content loader; a decoded sound
is shared by every Sound using the same file.

*Left for later steps:*
SoundGroups and effects (step 4, below); the modern audio API (step 5); the 2009
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

## 3D sound (step 3)

**3D sound (step 3)**, built 2026-10-14. A Sound under a part or an
Attachment in Workspace is heard from there: quieter with distance by its
`RollOffMode`, and to the side it is on.

*How it was measured.* Scripts can't hear: `PlaybackLoudness` is taken
before `Volume` and distance. So Studio's own output was recorded. A small
WASAPI loopback recorder (cpal, kept outside the repository) captured the
system's output while a fresh Studio 0.743 playtest client played a 1 kHz
test tone. The tone was put in Studio's `content/sounds` for the test and
removed after. The recorder measured only the tone's frequency, so other
sounds on the machine didn't count, every 20 ms on each side. The camera
was fixed, a global sound at Volume 1 gave the reference level, and the
script logged each step's time from that reference. Studio is silent
whenever its window isn't focused, so it was brought to the front for the
recording. Two runs gave the same levels. The output device was stereo.

*Roll-off* (Confirmed; creator-docs `RollOffMode` gives the same
formulas). Each level is relative to the global sound, for `min` 10 and
`max` 100:

| Distance | Inverse | Linear | LinearSquare | InverseTapered |
|---:|---:|---:|---:|---:|
| 5, 10 | 1.00 | 1.00 | 1.00 | 1.00 |
| 20 | 0.50 | 0.88 | 0.75 | 0.50 |
| 40 | 0.24 | 0.67 | 0.44 | 0.25 |
| 60 | 0.17 | 0.43 | 0.19 | 0.17 |
| 80 | 0.13 | — | 0.04 | 0.05 |
| 95 | 0.10 | 0.06 | 0.00 | 0.00 |
| 120 | 0.00 | 0.00 | 0.00 | 0.00 |

So: full volume within `RollOffMinDistance`, and nothing past
`RollOffMaxDistance`, even when the minimum is larger (min 20, max 3, at
10: silent). Between them, Inverse is `min / d`, Linear
`(max - d) / (max - min)`, LinearSquare that squared, and InverseTapered
the lesser of Inverse and LinearSquare. A minimum of 0 makes Inverse
silent. With the default max of 10000, Inverse at 200 and 1000 studs read
0.06 and 0.01. `SoundService.RolloffScale` stretches the distance past
the minimum, `d = min + scale * (distance - min)`: Inverse at 20 and 40
with scale 2 read 0.33 and 0.14, with 0.5 at 40 read 0.40, and Linear at
40 with 2 read 0.33. Rook: `rook_audio::spatial::attenuation`, with these
levels as its test.

*Left and right* (Confirmed for a stereo device). The sound at 10 studs,
by angle clockwise from straight ahead:

| Angle | 0 | 30 | 45 | 60 | 90 | 120 | 150 | 180 | 240 | 270 | 300 | 330 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Left | 1.00 | 0 | 0 | 0 | 0 | 0 | 0 | 0.71 | 1.42 | 1.00 | 1.71 | 1.41 |
| Right | 1.00 | 1.41 | 1.67 | 1.71 | 1.00 | 1.41 | 1.00 | 0.70 | 0.04 | 0 | 0 | 0 |

Studio pans in 7.1 and folds that down to stereo. Constant-power panning
between speakers at 0, ±30, ±90 and ±150 degrees, with the front pair
reaching their side at 1.41, the centre both sides at 1, and the side and
back speakers their side at 1, gives every level above within 0.02. Rook
pans the same way (`spatial::pan`). Directly above, below, or at the
listener, a sound came out at 0.89 on both sides. Rook leans towards that
as a sound rises above the listener (Inference between the two measured
cases). How other speaker layouts sound is unknown (Approximation: Rook
mixes the same stereo pair to them).

*What is 3D* (Confirmed): a Sound whose parent is a BasePart (heard at
its position) or an Attachment (at its world position, measured at 40
studs: 0.25 with Inverse), in Workspace. A sound under a Model or Folder
in Workspace, under a Folder inside a far part, in ReplicatedStorage,
SoundService or Lighting, or under a part in ReplicatedStorage 5,000 studs
away, is global: full volume on both sides. Every one of those was heard,
so a global sound plays wherever it is in the DataModel.

*The listener* (Confirmed): the camera by default, facing as it faces. A
camera turned 90 degrees left heard a sound ahead of the world on its
right. `SoundService:SetListener(ListenerType, ...)` moves it:

- `CFrame`: a CFrame 40 studs away, facing the other way from the sound,
  heard it at 0.25 × 0.71 (behind).
- `ObjectCFrame`: a part's CFrame (a part turned 90 degrees heard the sound
  on its left).
- `ObjectPosition`: a part's position, facing as the camera does
  (Inference: in the test the camera's and the world's facing were the
  same).

`GetListener` returns the type and what was given (nil for the camera).
The script-visible part is the conformance case `content/sound_3d`,
together with:

- the defaults (min 10, max 10000, Inverse);
- the deprecated `EmitterSize` and `MaxDistance` setting the minimum and
  maximum;
- a negative minimum reading 0;
- `SoundService`'s `RolloffScale` 1, `DistanceFactor` 3.33 and
  `DopplerScale` 1.

*Rook's side*: the engine says where each playing 3D sound is and where
the listener is (`DataModel::playing_sounds`, `sound_listener`). The
player turns that into a left and right gain per sound each frame. The
mixer ramps to new gains over one buffer, so a moving sound doesn't
click.

*Not done* (Approximation): the Doppler effect (`DopplerScale`,
`DistanceFactor`); a `PlayOnRemove` sound plays out as a global sound,
since its part is gone; sounds under parts that move fast between frames
step in level at 60 Hz.

## SoundService, SoundGroups and effects (step 4)

**SoundService, SoundGroups and effects (step 4)**, built 2026-10-15. Each
of these now changes what the player hears: a Sound's own effects, its
SoundGroup (and the groups that one is nested in) with that group's
effects, `SoundService.AmbientReverb`, and `SoundService:PlayLocalSound`.
The engine reports them in `rook-engine`'s `sounds/effects.rs`
(`PlayingSound::effects` and `group`, `DataModel::sound_groups` and
`Listener::ambient_reverb`). `rook-audio`'s mixer runs them (`effects`,
`mixer/bus.rs`).

*What the archive uses* (class counts from `reports/corpus/summary.md`;
no new scan):

| Class | Files | Instances | Rook |
|---|---:|---:|---|
| EqualizerSoundEffect | 335 | 9,368 | heard |
| ReverbSoundEffect | 248 | 2,155 | heard |
| PitchShiftSoundEffect | 240 | 2,949 | heard |
| DistortionSoundEffect | 229 | 6,263 | heard |
| SoundGroup | 222 | 735 | heard |
| ChorusSoundEffect | 125 | 1,508 | kept, not heard |
| EchoSoundEffect | 123 | 1,508 | kept, not heard |
| CompressorSoundEffect | 70 | 512 | kept, not heard |
| FlangeSoundEffect | 61 | 1,716 | kept, not heard |
| TremoloSoundEffect | 42 | 441 | kept, not heard |

Each of the four most used effects is in about twice as many files as
any of the rest. Those four are done; the other five pass sound through
unchanged. Scripts call `SoundService:PlayLocalSound` in 38 files (327
calls), so it is done too.

The corpus doesn't count SoundService's property values, so five places
were looked at with `rook dump`:

- A 2021 disaster place, both core places and a classic 2008 place save `AmbientReverb`
  NoReverb.
- A disaster place and the 2016 core place save `RespectFilteringEnabled` false.
- A modern core place has four PitchShift effects (`Octave` 0.5) under sounds.

Scripts that set `AmbientReverb` are rare: helpers like `CheckReverb` in
2 files.

*How effects were measured* (Confirmed, Studio 0.742, playtest client).
`PlaybackLoudness` is taken after a Sound's own effects but before
`Volume` and its SoundGroup:

- an equalizer at -80 dB on every band read 0;
- a full distortion read 971;
- a group at Volume 0, or with an equalizer under it, changed nothing.

So a sound at Volume 0 shows what its effects do, without making a
sound. The test sound was the install's `sounds/impact_explosion_03.mp3`
(2.586 s, the loudest of its sounds), looped. For each setting its
`PlaybackLoudness` was averaged over two loops. Rook's effects run over
the same file the same way
(`cargo run -p rook-audio --example studio_effect_levels -- <file>`):

| Setting | Studio | Rook |
|---|---:|---:|
| No effect | 180.5 | 180.2 |
| EQ low -80 / mid -80 / high -80 | 54.3 / 167.2 / 180.1 | 54.5 / 166.3 / 178.9 |
| EQ defaults (-20, -10, 0) | 28.0 | 28.5 |
| EQ +10 on every band / +30 | 571.6 / 569.6 | 569.7 / 569.7 |
| EQ high band only | 9.3 | 9.7 |
| Distortion 0.25 / 0.5 / 0.75 / 1 | 236.5 / 307.3 / 416.6 / 955.3 | 234.7 / 305.9 / 417.3 / 954.3 |
| Reverb defaults | 249.4 | 270.2 |
| Reverb wet only, decay 0.3 / 1.5 / 5 s | 224.8 / 246.5 / 270.5 | 244.7 / 247.2 / 295.6 |
| Reverb wet only, Density and Diffusion 0 | 218.4 | 193.6 |
| PitchShift 0.5 / 2 | 188.5 / 142.1 | 188.8 / 142.1 |
| PitchShift 2, then EQ high only | 15.9 | 18.3 |

*The effects:*

- **EqualizerSoundEffect** (Confirmed). Three bands: below 400 Hz, 400 to
  4000 Hz, and above (creator-docs), with gains in dB. Studio plays them
  within -80 to 10 dB, though the property reads back whatever was set
  (+30 was as loud as +10). Rook splits the bands with 24 dB-per-octave
  Linkwitz-Riley crossovers (Inference: FMOD's three-band EQ, which has
  the same bands). The table matches within 4%.
- **DistortionSoundEffect** (Confirmed by the table). A soft clipper,
  `(1 + k) x / (1 + k |x|)` with `k = 2 Level / (1 - Level)`, for `Level`
  0 to 1; at 1 it gives a square wave. The default is 0.75: creator-docs
  says 0.5, but Studio reads 0.75.
- **ReverbSoundEffect** (Approximation). Its ranges (creator-docs):
  `DecayTime` 0.1 to 20 s, `Density` and `Diffusion` 0 to 1, `DryLevel`
  and `WetLevel` -80 to 10 dB. Rook's reverb is an eight-line feedback
  delay network, with all-pass diffusers, that falls 60 dB in
  `DecayTime`. Studio's tail gets only a little louder as the decay gets
  longer, so Rook's wet level is fitted to Studio's three decay times.
  It comes within 10% of Studio's levels. How it sounds (early
  reflections, colour) isn't measured.
- **PitchShiftSoundEffect** (Approximation). `Octave` is the pitch ratio,
  0.5 to 2, at the same speed (creator-docs). Rook reads a delay line at
  two crossfaded points. Studio's shifter makes a sound louder shifted
  down and quieter shifted up; Rook's level change is fitted to the two
  settings above. That it shifts the pitch was checked through an
  equalizer: shifting up before cutting the low bands kept 15.9 of the
  explosion, while cutting first kept 6.4.

*Order* (Confirmed). Higher `Priority` runs first, and `Priority` 0 means
the effect's own base priority (creator-docs). The base priorities were
found by bisection, pairing one non-linear effect with another so the
order shows in the level: Equalizer 8, Reverb 8, Distortion 4,
PitchShift 3. For example, a Distortion at Priority 9 ran before an
Equalizer at 0, and one at 8 ran after it. Negative priorities sort the
same way.

In the three ties measured, the effect added later ran first, and Rook
breaks ties that way (Inference; creator-docs calls the order
undetermined). Equalizer and Reverb are both linear, so their order
doesn't change the sound. The rule is `rook_audio::effects::ordered`,
tested with the measured cases.

*Which effects apply* (Confirmed):

- Only enabled ones directly under the Sound: one inside a Folder under
  it does nothing, and `Enabled` false turns one off.
- One added while the sound plays applies at once.

An effect's state (its filters, a reverb's tail) carries on when its
settings change. A sound's effects end with the sound (Confirmed by
recording, below: a sound's own reverb went silent the moment its tone
ended).

*SoundGroups.* None of this shows in `PlaybackLoudness`, so Studio's
output was recorded (2026-10-15, Studio 0.742, playtest client), as for
3D sound but capturing only Studio's own process (WASAPI process
loopback), so other programs' sounds stayed out. A 1 kHz tone at Volume
0.1 was put in the install's `content/sounds` for the test and removed
after. Levels are relative to the same tone played with no group:

| Setup | Level |
|---|---:|
| In a group at 0.5 | 0.500 |
| In a group at 0.5, inside a group at 0.5 | 0.250 |
| In a group at 1, inside a group at 0.5 | 0.500 |
| Parented under a group at 0.5, `SoundGroup` not set | 1.000 |

- A Sound plays through the group its `SoundGroup` property names.
  Parenting it under a group doesn't count (Confirmed).
- A group's sounds are mixed together, run through the group's effects,
  and scaled by its `Volume` (Confirmed). Studio keeps `Volume` within 0
  to 10 (Confirmed).
- A group parented under another plays into it, and their volumes
  multiply (Confirmed).
- A group's reverb rings on after its last sound stops (Confirmed). With
  a group ReverbSoundEffect (dry and wet 0 dB, decay 3 s), the tail went
  on for about 1.5 s above the recording's floor after a 0.25 s tone
  ended, after a looped tone was stopped, and after one was destroyed.
  The same reverb under the Sound itself was cut the moment the tone
  ended.

`Sound.SoundGroup` takes SoundGroups only (Confirmed: "Expected
SoundGroup got Part for Sound.SoundGroup."). It still points at a group
that has been destroyed, and Rook then ignores it.

*`SoundService`:*

- **`AmbientReverb`** (Approximation, its level measured). Creator-docs:
  "A reverb preset that should be applied to all Sounds in the
  experience". `ReverbType` lists FMOD's reverb presets by name, in
  FMOD's order. So Rook adds a reverb to the whole mix with each
  preset's published decay, diffusion, density and high cut
  (`rook_audio::effects::ambient`). The default is NoReverb.

  Studio's output was recorded as for SoundGroups: a 0.25 s 1 kHz tone
  at Volume 0.1, then 5 s of tail, under each preset. The tail's energy
  over the 2 s after the tone, relative to the tone:

  | Preset | Studio, global | Studio, 3D | FMOD wet level in Rook | Rook now |
  |---|---:|---:|---:|---:|
  | Cave | 0.00016 | 0.00007 | 0.0062 | 0.00016 |
  | Hallway | 0.00034 | 0.00015 | 0.0171 | 0.00043 |
  | ConcertHall | 0.00033 | 0.00016 | 0.0093 | 0.00023 |
  | NoReverb | 0 | 0 | — | 0 |

  So Studio's ambient reverb is much quieter than the presets' published
  wet levels give: 14.5 to 17.0 dB. Rook lowers every preset's wet level
  by 16 dB (`AMBIENT_WET_DB`), which brings these three within 1.6 dB.
  Both global sounds and 3D ones get the reverb. A 3D sound 5 studs in
  front of the camera got about 3.5 dB less of it than a global one;
  Rook reverbs the whole mix alike (Approximation). Studio's tails fell
  at 15, 31 and 15 dB a second (Cave, Hallway, ConcertHall). That is a
  little slower than the presets' 2.9, 1.5 and 3.9 s decays, which Rook
  keeps (21, 41 and 15 dB a second measured the same way).
- **`RespectFilteringEnabled`**. The default is true (Confirmed;
  creator-docs). The reflection database says false, and Rook corrects
  it. It decides whether a client's sound playback replicates. Rook only
  replicates from the server, so for now it changes nothing (networking,
  M10).
- **`PlayLocalSound(sound)`** (Confirmed for what scripts see). On a
  client, the sound plays once from the start, for that client only.
  `Playing`, `TimePosition`, `PlaybackLoudness` and the events stay as
  they were. A Sound outside the DataModel is accepted. The errors:
  - a non-Sound or nil: "SoundService:PlayLocalSound expects a Sound
    object.";
  - any call on a server: "SoundService:PlayLocalSound only works on a
    client."

  Rook plays it from where the sound was when it was called (Inference),
  loading the file if nothing had loaded it yet.

The script-visible rules are the conformance case `content/sound_groups`:

- SoundService's defaults;
- SoundGroup `Volume` and its limits;
- `Sound.SoundGroup`;
- every effect class's `Enabled`, `Priority` and defaults;
- effect properties reading back unclamped;
- `PlayLocalSound` on a server.

The engine test `effects_groups_and_local_sounds` checks the rest on a
client.

*Not done:*

- Chorus, Compressor, Echo, Flange and Tremolo pass sound through
  unchanged (Approximation, left out by archive use).
- A sound that is playing and is also given to `PlayLocalSound` is heard
  only once.
- `PlaybackLoudness` doesn't reflect the sound's group, as in Studio.
