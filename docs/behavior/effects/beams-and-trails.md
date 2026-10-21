# Beams and Trails

ROADMAP M7 step 4. Trail is in 17 archive files, Beam in 15. Sources: the
creator-docs reference (Beam, Trail, TextureMode, the "Beams" guide) and
measurements in Studio 0.743 on 2026-10-19.

## What Studio does

### Beam

- **Its curve** (creator-docs): a cubic Bézier from Attachment0 to
  Attachment1, its inner points `CurveSize0` studs along Attachment0's
  Axis (RightVector) and `CurveSize1` studs back along Attachment1's,
  drawn as `Segments` straight pieces (10 by default). Colour and
  transparency change only between pieces.
- **Its width** runs from `Width0` at Attachment0 to `Width1` at
  Attachment1. Without `FaceCamera` it lies along the Attachments'
  SecondaryAxis, their up (Observed: between two Attachments at their
  default orientation, a 3-stud-wide beam was a full band seen from the
  front and a hairline from above), however the curve runs: a beam
  curving up steeply to its end is an upright band there. With
  `FaceCamera` it turns to face the camera.
- **Its image** repeats `TextureLength` times along it (`TextureMode`
  Stretch) or once every `TextureLength` studs (Wrap, Static), and
  scrolls `TextureSpeed` times a second from Attachment0 towards
  Attachment1; `SetTextureOffset` moves it the other way. Without an
  image, or while it loads, the beam is its colour alone.
- **Light**: as a ParticleEmitter's (docs/behavior/effects/particle-emitters.md),
  `Brightness` scaling its own colour when `LightInfluence` is below 1.

### Trail

- While enabled, it records where its Attachments are each frame and
  fills the strip between those places and the last ones (creator-docs).
  A new piece starts once either Attachment has moved `MinLength`;
  until then the newest piece stretches. Each piece lives `Lifetime`
  seconds (a changed Lifetime applies to those laid already); `MaxLength`
  cuts the oldest end.
- `Color`, `Transparency` and `WidthScale` (a share of the distance
  between the Attachments, about their midpoint) go over a piece's life,
  newest at the Attachments.
- Its image stretches over the whole trail (Stretch), repeats every
  `TextureLength` studs from the Attachments (Wrap), or stays where it was
  laid (Static). `Clear()` drops every piece; disabling stops new ones and
  lets the rest fade.

## Rook's fit

The player reads each into the renderer's `Beam` and `Trail`
(`crates/rook-view/src/effects/ribbons.rs`); the renderer lays and
ages the Trails' pieces and builds both as strips of quads each frame
(`crates/rook-render/src/effects/ribbons.rs`), drawn with the particles' shading
after the transparent parts and before the particles, farthest strip
first, both sides showing (`renderer/ribbons.rs`). Without FaceCamera a
Beam's width at each piece turns from Attachment0's up to Attachment1's
(Approximation).

Not done yet: Beam's Wrap and Static are drawn alike (both from
Attachment0), transparency envelopes, `LocalTransparencyModifier`, and
clearing a Trail when its Attachments change.

## Rook against Studio

The same script (two Attachments 10 studs right and 8 up of each other,
CurveSize0 6, widths 1 and 3, green to blue, no image) in Studio and
`rook-player --script`: the same curve to the pixel, leaving Attachment0
along its Axis, its width along the Attachments' up at both ends, green
to blue. A Size 1.5, Squash 1 particle in the same script drew 41 × 166
px in both.
