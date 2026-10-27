# Overhead names and health

Checked against Studio 0.744 on 2026-10-26.

The name and health bar over a Humanoid's head: `rook_gui`'s
`world::overhead` (which tags show, and where) and `draw::overhead` (how
they look). Studio's engine draws them, not the CoreScripts (they only
set `DisplayDistanceType` None on copies of characters, for screenshots
and the selfie view), and makes no instances for them. Studio's
`screen_capture` leaves them out, so they were measured in screenshots of
a playtest from a pinned camera (FieldOfView 70, 1270 × 793), with
anchored test Humanoids at set distances and health.

## Which show

- Every Humanoid in Workspace whose parent Model has a `Head` part shows
  a tag to each other player; Inference: a player never sees its own.
- The text is the Humanoid's `DisplayName`, or its Model's name when
  that's empty. A player's character's Humanoid gets the player's
  `DisplayName` when it spawns.
- Defaults (Humanoid): `DisplayDistanceType` Viewer, `HealthDisplayType`
  DisplayWhenDamaged, `NameDisplayDistance` and `HealthDisplayDistance`
  100, `NameOcclusion` OccludeAll. Player and StarterPlayer also have
  `NameDisplayDistance` and `HealthDisplayDistance`, 100 each.
- **Distance** is from the viewer's character to the head, not from the
  camera: with the character 60 studs behind the camera, a head 52 studs
  from the camera and 110 from the character showed nothing. Inference:
  from the camera when the viewer has no character.
- `DisplayDistanceType`: None never shows; Subject uses its own Humanoid's
  distances (one at 5 studs, seen from 16, showed nothing); Viewer, the
  default, uses the viewer's Humanoid's (raising the viewer's to 1000
  showed tags 300 studs off).
- **Fading**: tags 75, 80, 85 and 90 studs off were opaque, 95 about half
  see-through, 99 faint, 105 gone. Rook fades over the last 10 studs of
  the distance (Inference: 10 studs, not a share of the distance, as only
  the default 100 was measured). The name and the bar fade by their own
  distances.
- **Health bar**: DisplayWhenDamaged shows it below `MaxHealth`, AlwaysOn
  always, AlwaysOff never; a dead Humanoid (Health 0) keeps its name and
  an empty red bar. Without a bar the name stays where it would be.
- **Occlusion**: with OccludeAll a tag hides while a part is between the
  camera and the head: hidden behind a part that only blocked the
  camera's view, and hidden though the tag's own spot cleared the part's
  top. Other characters don't hide it: a head behind another NPC's head
  kept its tag. NoOcclusion never hides; Inference: EnemyOcclusion hides
  only players not on the viewer's team.
- Tags draw over the world: a tag 300 studs off drew over a nearer head.
  Inference: under every screen GUI, as Rook draws them with it.

## Where and how big

- The bar's top is 1.5 studs above the head's center (the same with a
  head 1 or 2 tall), and 1.4 pixels higher than that point.
- Tags come in three sizes by the head's distance from the camera: full
  up to 18 studs, three quarters from 22 to 34, half from 52 (Inference:
  the steps are at 20 and 40). Only the bar's length and the text scale;
  the bar's height and end caps don't.

## How they look

At full size (the others scale as above):

- **The bar** is 102 × 8 pixels: a 96-pixel body between two 3-pixel end
  caps. The body is a 1-pixel outline in the health color around a
  100-gray track at half opacity; the caps are notched (outline down
  their outer column, and across their top and bottom rows) around
  opaque 100-gray. Across the body's middle two rows, a strip of the
  health color `round(96 × Health / MaxHealth)` pixels long (86, 72, 48
  and 24 at 90%, 75%, 50% and 25%).
- **The color** is the CoreScripts' `Healthbar3D` function: red (255, 28,
  0) to 10%, green (27, 252, 107) from 80%, and between them Shepard's
  interpolation of red at 10%, yellow (250, 235, 0) at 50% and green at
  80%. Studio's 75% bar drew (36, 251, 102) and its 25% (242, 91, 5),
  which it gives. The pixels are these colors exactly: no lighting, tone
  curve or fog.
- **The name** is white SourceSans (its two-story g), size 24 by its
  widths, with a soft black outline about a pixel wide that blends about
  60% at most, a single layer however the letters' outlines overlap. It
  sits just over the bar, and the bar draws over its descenders.

## Not yet

- Viewer and Subject distances between several players, and
  EnemyOcclusion, which need several clients (M10).
