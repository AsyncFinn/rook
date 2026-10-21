# The audio API

Checked against Studio 0.742 on 2026-10-15.

AudioPlayer, the Audio
effects, AudioEmitter, AudioListener, AudioDeviceOutput and
AudioAnalyzer, joined by Wires, play on the same engine as Sound: a
player plays through the same voice as a Sound, and effects through the
same chains. The rules scripts see (pins, which wires connect, where
emitters and listeners are, audibility, ranges) are `rook-lua`'s
`audio` module. The engine builds the graph from them each frame
(`rook-engine`'s `sounds/graph.rs`), and `rook-audio`'s `graph` renders
it in the mixer. All observed in Studio 0.742, Baseplate, unless marked.

*What the archive uses* (`reports/corpus/corpus.json`): two files, with
AudioPlayer (4 instances), AudioEmitter (10), AudioListener (6),
AudioFader (8), AudioReverb (4), AudioEcho, AudioEqualizer and
AudioDeviceOutput (2 each), and 30 Wires. The archive is mostly older
than the API, so this step follows creator-docs and Studio rather than
archive use, for the places made since.

*How it was measured* (all silent). `GetAudibilityFor` is pure maths,
so distances and curves were read from it, waiting about 0.06 s after a
move (Studio works it out once an audio tick). Levels came from
AudioAnalyzers on a playtest client with nothing wired to an
AudioDeviceOutput. `RmsLevel` reads exactly `Sound.PlaybackLoudness` /
1000 for the same stream, so the API's effects could be compared with
step 4's. The emitter and listener tests used their own
`AudioInteractionGroup` and unwired the camera's listener meanwhile.
The steady source was `sounds/action_falling.ogg` through an
AudioDistortion at `Level` 1 (about 0.967 rms).

*AudioPlayer* (Confirmed):

- Defaults: `Asset` "", `Volume` 1 (0 to 10; NaN becomes 10),
  `PlaybackSpeed` 1 (0 to 20), `TimePosition` not clamped, `AutoLoad`
  true, `AutoPlay` false. `Asset` turns `\` into `/`. `IsPlaying` can't
  be set ("lacking capability RobloxEngine"; Rook's `RobloxSecurity`
  capability now has Studio's name).
- It loads only in the DataModel, through the same cache as Sounds:
  `IsReady`, then `TimeLength` changes. With `AutoLoad` false it loads
  on `Play`. A file that fails leaves `IsReady` false, and the player
  "plays" at 0 for ever.
- `Play` returns nothing and plays from `TimePosition`; while playing it
  does nothing. `Stop` keeps `TimePosition`, so `Play` resumes. A
  negative position plays from 0; one at or past the end (by `Play` or
  by seeking while playing) ends at once: `Ended`, `IsPlaying` false,
  `TimePosition` 0. `Looping` wraps and fires `Looped`. Speed 2 moves
  2.03 s in a second; speed 0 holds.
- `Play(atTime)` and `Stop(atTime)` on a server error
  "AudioPlayer:Play(atTime) cannot be used on the server", and `Cancel`
  returns false. On a client they are scheduled on `GetMixerTime`'s
  clock, which in Rook is the DataModel's own (Inference).
- `Volume` is a plain linear gain (0.5 halved a peak exactly).

*Wires* (Confirmed). `Connected` is true when both ends are set, the
pins exist (names are case-sensitive), both ends and the wire are in the
DataModel (ServerStorage is fine), the wire doesn't join an instance to
itself, and it doesn't close a loop. Of two wires making a loop, the
second reads false (Inference for longer loops: the ones made first
win). Wiring one output into the same input twice counts once; several
wires into one input add up. The pins, as `GetInputPins` and
`GetOutputPins` list them:

- AudioPlayer: `Output`;
- AudioEmitter: `Input`;
- AudioListener: `Output`;
- AudioDeviceOutput and AudioAnalyzer: `Input`;
- effects: `Input` and `Output`, plus `Sidechain` on AudioCompressor.

*Emitters and listeners* (Confirmed unless marked):

- Where they are: the parent's world position, for a BasePart,
  Attachment, Camera or Model (its pivot). Under a Folder they are
  nowhere and hear or are heard at 0. They needn't be in Workspace (a
  part in ReplicatedStorage worked). `PositionType` Instance changed
  nothing in 0.742, so Rook ignores `PositionInstance`.
- A listener hears only emitters with the same `AudioInteractionGroup`.
- Distance: with no curve set, InverseTapered from 4 to 10,000 studs
  (4/d), whatever the bounds. `SetDistanceAttenuation` points are joined
  by straight lines and flat beyond the ends; volumes are kept within 0
  to 1, and a non-number key errors "AudioEmitter:SetDistanceAttenuation:
  numeric key expected.". A listener's curve multiplies the emitter's
  (none is 1). Angle curves multiply too (creator-docs); by default
  direction changes nothing.
- A listener's output is half the audibility times the source, in every
  direction. Split to stereo it pans as a Sound does (60 degrees to the
  right at 4 studs: right 1.71, left 0, as Sound's panner gives). Rook
  pans a listener's output the same way (Inference). Nearer than about
  8 studs Studio also spreads a sound over more speakers (1.04 at 8
  studs ahead, 1.14 at 4, 1.61 on top of the listener); Rook doesn't
  (Approximation).
- Curves are kept in the `DistanceAttenuation` and `AngleAttenuation`
  properties so they replicate. Studio's encoding of them isn't known,
  so Rook writes its own (Approximation: a curve Studio saved in a file
  is lost; no archive place has one).
- On a client, Studio makes `Workspace.Camera.AudioListener`, with a
  Wire inside it to `SoundService.AudioDeviceOutput` (its `Player`
  nil); a server gets none. Rook does the same for
  `DefaultListenerLocation` Camera, and under the character for
  Character (creator-docs). Baseplate reads Camera, though the
  reflection database says Default.
- An AudioDeviceOutput is heard when its `Player` is nil or the local
  player (creator-docs).

*AudioAnalyzer* (Confirmed). `RmsLevel` and `PeakLevel` are linear, 1 at
full scale. Spectrum windows: Small 128 bins, Medium 512, Large 2048,
bin k at k × 24000 / N Hz. `SpectrumEnabled` false keeps returning the
last spectrum. Only clients measure; on a server the levels stay 0.
Rook measures the last 1024 frames (Inference: Studio's "last buffer"
has no stated length). Studio's spectrum adds up to 1.05 to 1.11 times
the rms.

*Channel layouts* (Approximation). Studio's streams have their own
layouts. An AudioEcho or AudioCompressor turns a mono file into 7.1,
with -3 dB on left and right, so an analyzer after one reads 0.354 of
the rms at neutral settings; a listener's output is 8 channels too.
Rook's streams are all stereo, so it reads those higher. The gains were
compared through the layout (the table below divides it out).

*Effects.* On the same file at the same time, AudioEqualizer,
AudioDistortion, AudioPitchShifter, AudioReverb and AudioChorus gave
the same level as their step 4 SoundEffect within 1% (Confirmed), so
they share Rook's code. AudioFader is a linear gain (Confirmed: peaks
0.49998 and 1.9998). Nothing clips inside the graph. AudioEqualizer's
`MidRange` moves the crossovers (200 to 20,000 Hz, Confirmed).
AudioReverb's properties that ReverbSoundEffect lacks (`DecayRatio`,
the delays, the low shelf, `ReferenceFrequency`) aren't played
(Approximation). The new effects, by
`cargo run -p rook-audio --example studio_effect_levels -- <file>`
(Studio's level is its ratio to the plain sound, times 180.5):

| Setting | Studio | Rook |
|---|---:|---:|
| AudioChorus defaults | 188.1 | 173.9 |
| AudioChorus Mix 1, Depth 1 | 181.4 | 191.7 |
| AudioEcho defaults | 261.9 | 337.3 |
| AudioEcho wet only, 0.2 s | 175.8 | 180.1 |
| AudioEcho wet only, 0.2 s, Feedback 0.7 | 237.4 | 289.2 |
| AudioCompressor defaults | 14.8 | 8.9 |
| AudioCompressor -20 dB, 4:1 | 78.7 | 79.7 |
| AudioCompressor -20 dB, 4:1, Attack 0.001 | 44.4 | 42.3 |
| AudioCompressor -20 dB, 50:1 | 61.7 | 63.8 |
| AudioCompressor -20 dB, 50:1, fastest | 35.4 | 32.6 |
| AudioCompressor -10 dB, 2:1, fastest | 112.3 | 105.9 |

- **AudioChorus** (Approximation). Three copies swinging around 20 ms
  by `Depth` × 10 ms at `Rate`; the copies' gain is fitted to Studio's
  level, within 8%. Its shape isn't measured.
- **AudioEcho** (Approximation). A plain feedback delay. Without
  feedback it is within 2% of Studio; with it Rook is up to 29% louder.
  `RampTime` isn't used: a new delay applies at once.
- **AudioCompressor** (Approximation). Feed-forward, following both
  sides' level together, from its `Sidechain` input when one is wired.
  Within 8% at five settings, but 0.6 times as loud at the defaults.
  `MakeupGain` is exact (10 dB was 3.16 times).

*Ranges* (Confirmed; Rook keeps them as Studio does): AudioFader
`Volume` 0 to 3; AudioEqualizer gains -80 to 10; AudioDistortion
`Level` 0 to 1; AudioPitchShifter `Pitch` 0.5 to 2; the rest of
AudioReverb, AudioEcho, AudioChorus, AudioFlanger, AudioCompressor,
AudioFilter and AudioLimiter are in `rook-lua`'s `audio/ranges.rs`.

The script-visible rules are the conformance case `content/audio_api`
(recorded in Studio). The engine test `tests/audio_api.rs` checks
playback, the default listener, scheduled calls and analyzers on a
client.

*Not done:*

- AudioFilter, AudioFlanger, AudioTremolo, AudioLimiter, AudioGate and
  AudioChannelSplitter pass sound through (the splitter on every output
  pin). AudioChannelMixer and the other audio classes aren't wired at
  all.
- AudioPlayer `PlaybackRegion` and `LoopRegion` keep their values but
  don't change playback.
- The near-field spread, 7.1 layouts and Studio's curve encoding,
  above.
