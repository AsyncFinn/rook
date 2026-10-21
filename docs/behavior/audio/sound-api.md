# The Sound API

Checked against Studio 0.743 on 2026-10-16.

All observed in a fresh Studio on a playtest server (and its client for
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
(Inference). `Volume` is a plain linear gain on the output (Confirmed;
see *Output level* below).

*Output level and clipping* (Confirmed, Studio 0.743, 2026-10-16). Studio's
output was recorded silently: its Windows output device was set to one of
Steam's virtual devices, which nobody hears, and that device was
recorded. `Only Play Audio from Window in Focus` was off, so Studio didn't
need focus. The install's `impact_explosion_03.mp3` (mono), as a global
sound in a playtest client, peaked at:

| Played | Studio | Rook before | Rook now |
|---|---:|---:|---:|
| Volume 0.1 | 0.071 | 0.100 | 0.071 |
| Volume 1 | 0.707 | 1.000 | 0.707 |
| Volume 3 | 2.121 | 3.000 | 2.121 |
| Volume 10 | 7.070 | 9.999 | 7.071 |
| 5 at once, Volume 1 | 3.535 | 5.000 | 3.535 |
| 10 at once, Volume 1 | 7.070 | 9.999 | 7.071 |

So `Volume` and stacked sounds add up linearly, with no limiter, and
Studio hands the device levels far past full scale. A mono sound plays
3 dB down on each side (1/√2, centered); the stereo
`action_falling.ogg` played at its own level (Rook within 0.2%). Rook
used to play mono at full level and clip the mix at full scale itself,
which crackled; it now centers mono Sounds after their effects
(Inference for the order) and leaves levels past full scale to the
system, as Studio does. The 3D levels above are relative to the mono
global sound, so they keep their fit. How a stereo file is panned in 3D
is unknown (Approximation: as now). `rook-player --record-audio` mixes a
place to a WAV file the same way, for checking Rook silently.

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
