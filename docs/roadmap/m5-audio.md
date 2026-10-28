# M5 Audio

Sound is in 1,284 files (113,199 instances); today it only keeps its
state. It's self-contained, and every place sounds wrong without it.

1. ✅ Decoding (OGG, MP3, WAV) and playback through the content loader (see [audio](../behavior/audio/playback.md)).
2. ✅ `Play`, `Stop`, `Pause`, `Resume`, `TimePosition`, `Looped`,
   `PlaybackSpeed`, `Volume`, and `Played`, `Ended`, `Loaded`, `IsLoaded` (see [audio](../behavior/audio/sound-api.md)).
3. ✅ 3D sound: a Sound under a part or Attachment is positioned, with
   `RollOffMode` and its distances measured in Studio (see [audio](../behavior/audio/3d-sound.md)).
4. ✅ SoundService, SoundGroups and the four most used sound effects (see [audio](../behavior/audio/soundgroups-and-effects.md)).
5. ✅ The audio API modern places use (AudioPlayer, AudioEmitter,
   AudioListener, AudioDeviceOutput, Wire and the Audio effects), on the
   same engine as Sound (see [audio](../behavior/audio/audio-api.md)).
6. **Match Studio**: close every difference left in what steps 1 to 5
   built.
   - *Scope*: how loud `Volume` is (a plain gain today); the Doppler
     effect; ReverbSoundEffect and PitchShiftSoundEffect as Studio sounds
     them, not Rook's own matched to its levels; the `AmbientReverb`
     presets measured; AudioEcho with feedback (29% too loud) and
     AudioCompressor (0.6 as loud); `PlaybackRegion` and `LoopRegion`;
     7.1 layouts and near-field spread; attenuation curves saved in
     files; AudioChannelMixer wired. Out: sound effects and audio
     classes not heard yet (M16).
   - *Evidence*: the [stubs](../guide/stubs.md) rows taken over by this
     step, and the open Approximations and Inferences
     on the audio pages.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 5, and no stubs row names this step.