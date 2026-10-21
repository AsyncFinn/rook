# Ambient and OutdoorAmbient

Checked against Studio 0.742 on 2026-10-16.

Measured on white SmoothPlastic floors,
walls and ceilings (EnvironmentDiffuseScale and EnvironmentSpecularScale
0), in the Baseplate (`Technology` ShadowMap) and again in a fresh Studio
on a copy saved without `Technology`, as archive places are. The pixels
read are the faces the sun doesn't reach.

**The ambient is the brighter of the two, per channel, not their sum**
(Confirmed):

| Ambient | OutdoorAmbient | Brightness | White drew (ShadowMap) | (no Technology) |
|---|---|---|---|---|
| 171 gray | 127 gray | 0 and 0.5 | 176 | |
| 171 gray | black | 0 and 0.5 | 176 | |
| black | 127 gray | 0 and 2 | 131 | 127 |
| 100 gray | 100 gray | 0 and 2 | 96 | 100 |
| black | white | 2 | 224 (as Ambient white alone) | |
| (171, 60, 0) | (0, 127, 127) | 0 | (175, 131, 130) | |

A sum would have drawn 100 + 100 gray near 148, and 171 + 127 near 207,
which is what Rook drew before.

- **No Brightness or time of day in it** (Observed): OutdoorAmbient 127
  alone draws the same at Brightness 0 and 2, and at ClockTime 14.5 and
  0 (127 in the place without `Technology`).
- **No fade with the normal** (Observed): walls, floors and a tunnel's
  ceiling draw the same.
- **No sky occlusion** (Observed, both Technologies): inside a closed
  40 × 20 × 40 room, with 1-stud walls or 8-stud ones, and inside a
  12 × 12 tunnel 120 studs long, OutdoorAmbient alone lights every face
  as it does outdoors. The archive tunnels that looked too bright in Rook
  (Brightness 0, Ambient 171, OutdoorAmbient 127) are explained by the
  max alone.
- The two Technologies draw the same ambient a few levels apart (127
  gray: 131 under ShadowMap, 127 without `Technology`); without
  `Technology`, a gray ambient on white draws its own value, as Rook's
  does. The Ambient ramp above (128 drew 134) was read in a place without
  `Technology` too, so one of the two is off by a few levels; not
  chased.

Rook (`ambient_color` in `shaders/lighting.wgsl`) takes the per-channel
max of the two colors on every face, and the local lights' tone curve
starts from it. In the measured scene (Brightness 0, Ambient 171,
OutdoorAmbient 127) `rook-player --screenshot` draws the wall and floor at
171, where it drew about 207.
