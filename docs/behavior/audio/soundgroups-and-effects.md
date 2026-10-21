# SoundService, SoundGroups and effects

Checked against Studio 0.742 on 2026-10-15.

Each of these changes what the player hears: a Sound's own effects, its
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
were looked at with `rook-tools dump`:

- A 2021 place, both large places and a 2008 place save `AmbientReverb`
  NoReverb.
- A small place and the large 2016 place save `RespectFilteringEnabled` false.
- A large modern place has four PitchShift effects (`Octave` 0.5) under sounds.

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
  reflections, color) isn't measured.
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
  unchanged (Approximation, left out by archive use). Step 5 added
  chorus, echo and compressor effects for the audio API, but these
  three still need their base priorities measured.
- A sound that is playing and is also given to `PlayLocalSound` is heard
  only once.
- `PlaybackLoudness` doesn't reflect the sound's group, as in Studio.
