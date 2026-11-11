# 3D sound

Checked against Studio 0.747 on 2026-11-10.

A Sound under a part or an Attachment, in Workspace or not, is heard from
there: quieter with distance by its
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
the lesser of Inverse and LinearSquare. By the formula a minimum of 0
makes Inverse silent (Inference, not measured: every Sound in some
archive places has a minimum of 0, so this needs checking against
Studio's output). With the default max of 10000, Inverse at 200 and 1000 studs read
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
reaching their side at 1.41, the center both sides at 1, and the side and
back speakers their side at 1, gives every level above within 0.02. Rook
pans the same way (`spatial::pan`). Directly above, below, or at the
listener, a sound came out at 0.89 on both sides. Rook leans towards that
as a sound rises above the listener (Inference between the two measured
cases). How other speaker layouts sound is unknown (Approximation: Rook
mixes the same stereo pair to them).

*What is 3D* (Confirmed): a Sound whose parent is a BasePart (heard at
its position) or an Attachment (at its world position, measured at 40
studs: 0.25 with Inverse), wherever the part is in the DataModel. A part
in ReplicatedStorage or a tool's Handle in StarterPack or a Backpack, 20
studs to the camera's right, was heard on the right only at 0.70 (0.5
from Inverse times the side's 1.41), on the left when moved left, and not
at all 5,000 studs away; an Attachment in a part in ReplicatedStorage did
the same (Studio 0.747, Play mode client, 2026-11-10, recorded from
Studio's process alone). So an unequipped tool's sounds are heard from
wherever its Handle was left, which is usually far away or silent. An
earlier note here had a part 5,000 studs away in ReplicatedStorage heard
at full volume; the 2026-11-10 runs contradict it.

A sound under anything else is global: full volume on both sides. Heard
that way (2026-10-14 and 2026-11-10): under a Model or Folder in
Workspace, a Folder inside a far part, ReplicatedStorage, SoundService,
Lighting, StarterPlayer and its StarterPlayerScripts and
StarterCharacterScripts, the player's PlayerScripts, a Tool itself (with
or without a Handle) in StarterPack or a Backpack, and a ScreenGui in
StarterGui or PlayerGui, whether it is `Enabled` or not and under a
Frame that isn't `Visible`. A global sound plays wherever it is in the
DataModel, and a Sound saved with `Playing` true starts playing at once,
the Starter templates and the copies made from them alike.

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

*Rook's side*: the engine keeps what the mixer plays between frames and
reads a sound again only when it changes (`DataModel::mix`). Each frame
it works out a left and right gain per 3D sound from where the sound and
the listener are now. The mixer ramps to new gains over one buffer, so a
moving sound doesn't click.

*Not done* (Approximation): the Doppler effect (`DopplerScale`,
`DistanceFactor`); a `PlayOnRemove` sound plays out as a global sound,
since its part is gone; sounds under parts that move fast between frames
step in level at 60 Hz.
