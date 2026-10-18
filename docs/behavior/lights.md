# Local lights

PointLight, SpotLight and SurfaceLight (ROADMAP M7 step 1). Measured in
Studio 0.742 on 2026-10-16, in a fresh Studio on a test place saved
without `Lighting.Technology`, as archive places are (old files don't save
it; newer ones mostly save Voxel). Scripts can't read `Technology`, so
the measurements stand for "what an archive place gets".

## How Studio draws them

**Voxels, not per-pixel lights** (Confirmed: Studio's shaders and
screenshots). Studio's D3D11 shaders have two paths for local lights: a
forward path (froxel light lists, `LightRenderGPU`, a smooth
`110 / (d² + 110)` falloff with a smoothstep to the range) for lights near
the camera, and a voxel light grid (`LightMapTexture`, 4-stud voxels filled
on the CPU) for the rest. In a place without `Technology`, every light
came from the grid, even with the camera 6 studs away: pools are blocky,
reach past `Range`, and don't depend on which way a surface faces.

Studio's surface shaders read the grid at **the point 6 studs out along
the surface's normal** (`(position + normal × 6) × LightConfig`, in every
part and terrain vertex shader), trilinearly between voxel centres, and
add it to Ambient before the tone curve. There is no N·L: a wall lit from
behind is lit if the voxels in front of it are.

**What one light writes into a voxel** (measured on a white SmoothPlastic
floor, top-down, reading floor points whose sample lands exactly on a
voxel centre; voxels are centred at 4k + 2):

| Range | Value at the light | Reaches 0 at | Loses per stud |
|---|---|---|---|
| 8 | 0.854 × Brightness | 12 studs | 0.071 |
| 16 | Brightness (flat out to about 6 studs) | 18.3 | 0.085 |
| 32 | Brightness (flat out to about 20) | 31 | 0.095 |
| 60 | Brightness (flat out to about 42) | 53.4 | 0.08 |

Rook's fit: `Brightness × Color² × clamp((0.8 × Range + 5.5 − d) × s, 0, 1)`
with `s = 1 / (12 + max(0, 16 − Range) / 4)`, d from the light to the voxel
centre (Inference that d is 3D: a light 4 studs above the sample plane
matched within the measurement noise).

- **Brightness is linear** (measured: Brightness 1 is 3.9 to 4.5 times
  Brightness 0.25 at every distance, in light units) and isn't capped at
  1: Brightness 4 draws 253 where 1 draws 224.
- **Color is squared**, as Ambient is: Color (0.5, 0.5, 1) at Brightness 1
  drew (136, 136, 224); 0.5² of the blue channel's light is 136.
- **The tone curve** (measured with Ambient alone, light units = Ambient
  squared): Ambient 32, 64, 96, 128, 160, 192, 255 grey drew 14, 47, 91,
  134, 166, 193, 224 on white; brighter lights reach 248 at 2.24 units and
  253 at 3.5. Rook applies this curve to the local lights' sum on top of
  Ambient (`studio_light` in `shaders/lighting.wgsl`), so a dark scene
  draws Studio's pixels; Rook's other light has no such curve yet (M13).

**SpotLight** (Range 32, Angle 60, Brightness 0.25, from an 8-stud part
facing +Z): it shines from the part's centre, not its face. Unlike a
point light it is dim near its origin and brightens along its axis (0.10
of Brightness 4 studs out, 0.77 at 20), then falls off as a point light
does; nothing behind it. Its edge is soft and follows distance off the
axis more than angle: 4 studs off the axis it keeps 0.73 to 0.85 at any
distance from 12 to 24 studs. Rook's fit (Approximation): the point
falloff × `clamp((d − 2) / (0.7 × Range))` × `clamp(1 − off / (along ×
tan(Angle / 2) + 4))`.

**SurfaceLight** (Range 16, Angle 90, Brightness 0.25, from the bottom of
an 8 × 1 × 8 part 5.5 studs above the sample plane): Rook's fit
(Approximation) treats it as a light at the face's centre whose cone fades
by the cosine out to 45° past half its Angle. Studio's forward path
measures from the nearest point of the face (`axisU`/`axisV` in
`LightRenderGPU`), but in the voxel grid that fitted worse than the
centre.

## Shadows

Measured in Studio 0.742 on 2026-10-18, by the Method below (a fresh
Studio, no `Technology`), with a Range 60, Brightness 0.25 PointLight at
(2, 6, 2), seen from 120 studs up.

- **`Shadows` off, light goes through parts** (Observed): behind a
  4 × 30 × 80 wall 12 studs from a Range 32 light, the floor drew 143
  with `Shadows` off and 2 with it on.
- **Parts that don't cast shadows don't block** (Observed): a post with
  `CastShadow` off casts nothing. `Lighting.GlobalShadows` makes no
  difference.
- **A one-voxel post leaves about half** (measured, in light units over
  the unshadowed floor): behind a 4 × 10 × 4 post 12 studs from the
  light, the darkest strip keeps 0.54 at 20 studs out and 0.6 further;
  with the post on the light's other side, 0.6. The shadow is soft, about
  three voxels wide, and grows no wider with distance.
- **It leans one way** (Observed): with the post at +X of the light the
  shadow spreads far to −Z (0.68 to 0.76 at 20 to 30 studs off the
  axis) and hardly to +Z; with it at −X it is narrow on both sides. A
  light inside a 12 × 4 × 12 part lights three quarters of its pool fully
  and the fourth (−X, −Z) at about half.
- **A light inside its own part still shines** (Observed): in a 3-stud
  opaque part, all of its pool; in a 12 × 4 × 12 one, as above.

Roblox's patent on its voxel lighting (US9245376) carries light from
cell to cell away from the light, each cell
taking its upstream face neighbours weighted by the light's direction,
less the cell's occupancy. That explains the softness and the lean (an
off-centre start), but fitting it to the two posts left 0.13 of error
on one of them, so Rook doesn't use it.

**Rook's fit** (Approximation): for each light with `Shadows`, each voxel
it reaches casts nine parallel rays toward it, from the voxel and from
the voxels around it across the ray, weighted 1-2-1 each way, and keeps
the share that no blocked voxel stops. Blocked voxels are those the sky's
light uses ([materials](materials.md#the-skys-diffuse-light)): opaque
parts casting shadows, as boxes, at least a voxel thick. The part the
light hangs from doesn't block it. A wall blocks all of it, a one-voxel
post half, as in Studio; the lean isn't reproduced. Against Studio on
the same scenes, behind the post the darkest strip draws 84 where Studio
draws 79 to 90; Rook's soft edge reaches a voxel further on the side
Studio leaves lit, and Studio's long tail to −Z is missing (up to 30
levels).

## Rook against Studio

The same scenes as `rook-player --screenshot` rbxlx copies, same camera
(top-down at 60 studs, 55° field of view, 1310 × 784), floor profiles at
2-stud steps, largest difference in 0–255:

| Scene | Largest difference |
|---|---|
| PointLight Range 8, Brightness 0.25 | 2 |
| Range 16, Brightness 0.25 | 6 (Studio's plateau is 128, Rook's 134) |
| Range 32, Brightness 0.25 | 9 |
| Range 16, Brightness 4 | 13 (at the pool's edge) |
| Color (0.5, 0.5, 1) | 2 |
| SpotLight along its axis | 5 |
| SpotLight across, 8 to 24 studs out | 8 to 13 |
| SurfaceLight | 17 (its dim edge reaches 2 studs too far) |

## How Rook does it

`rook-player`'s scene keeps every PointLight, SpotLight and SurfaceLight
under a part or an Attachment in Workspace (`scene/lights.rs`); disabled
lights stay in the list at Brightness 0 so that scripts switching lights
on and off rewrite only colours. Each light keeps the box of the part it
hangs from, which doesn't shadow it. Scripts changing a light, its
Attachment, or the part it hangs from redraw it (only those lights are
read again).

The renderer (`renderer/lights.rs`) keeps the lights in a buffer and a
spatial hash (16-stud cubes hashed into buckets, padded 8 studs so that
lights that move a little don't rebuild it). A compute pass
(`shaders/light_volume.wgsl`) fills a 256 × 64 × 256 volume of 4-stud
voxels (Studio's grid, below) centred 256 studs ahead of the camera
along its view direction with the height dropped, again only when lights
change or that centre drifts 64 studs, and parts, decals and terrain
(water included) sample it as Studio does (`local_light` in
`shaders/lighting.wgsl`). Surfaces outside it get no local light, as in
Studio. When a light has `Shadows`, the parts are marked into a bit per
voxel (`renderer/lights/blockers.rs`, shared with the sky's light) and
the fill pass casts that light's rays through them.


Cost, on an archive place with 1,900 lights, about 55 of them on parts
that move every frame (dev build, 300 frames, before → after): scene
update 1.45 → 1.8–2.0 ms, GPU main pass 1.9–2.6 → 3.1–3.4 ms,
render + readback 8.4–9.8 → 10.8–11.6 ms.

## How far the grid reaches

Measured in Studio 0.742 on 2026-10-16 (Method below, with two
2048-stud floors along +X and −Z, a Range 16 light every 32 studs along
each, and red markers every 128 studs). No flag in
`StudioAppSettings.json` gives the grid's size.

- **One grid of 4-stud voxels, cut hard at its faces** (Confirmed:
  shaders and screenshots). Studio's part and terrain shaders read one
  `LightMapTexture` at `(p + n × 6).yxz × LightConfig0 + LightConfig1`
  and force it to 0 where `|uv − LightConfig2| ≥ LightConfig3` on any
  axis. Seen from 80 studs up looking along +X, pools 700 studs out have
  the same shape as near ones (no coarser voxels further out), and the
  last one is sliced through: full brightness on one side of the edge,
  nothing on the other.
- **About 1024 × 256 × 1024 studs** (256 × 64 × 256 voxels; Inference
  from where the edges fall). Looking straight down, pools reach about
  500 studs out along both floors and stop; the floor is lit with the
  camera's focus 94 studs above it and dark at 144.
- **Placed ahead of the view** (Observed): looking along +X from
  (2, 80, 2), 9° down, the cut falls at x ≈ 764, so the grid's centre
  sits about 250 studs ahead; looking straight down it's centred under
  the camera. Rook's screenshot of the same row ends on the same screen
  row (y = 269 of 675 in both).
- **It follows `Camera.Focus`, not the camera** (Observed: moving Focus
  300 studs along +X, the camera staying put, lit 9 more pools that way).
  In Studio's edit camera Focus sits 20 studs ahead of the camera, and in
  play it is the camera's subject; Rook has no Focus yet and uses the
  camera's position.
- **It moves in chunks, asynchronously**: after the camera jumps hundreds
  of studs Studio keeps drawing the old grid for several seconds, and a
  freshly opened place can draw no local light at all for the first few.
  Rook fills its volume in the frame it moves.

## Ambient and OutdoorAmbient

Measured in Studio 0.742 on 2026-10-16, on white SmoothPlastic floors,
walls and ceilings (EnvironmentDiffuseScale and EnvironmentSpecularScale
0), in the Baseplate (`Technology` ShadowMap) and again in a fresh Studio
on a copy saved without `Technology`, as archive places are. The pixels
read are the faces the sun doesn't reach.

**The ambient is the brighter of the two, per channel, not their sum**
(Confirmed):

| Ambient | OutdoorAmbient | Brightness | White drew (ShadowMap) | (no Technology) |
|---|---|---|---|---|
| 171 grey | 127 grey | 0 and 0.5 | 176 | |
| 171 grey | black | 0 and 0.5 | 176 | |
| black | 127 grey | 0 and 2 | 131 | 127 |
| 100 grey | 100 grey | 0 and 2 | 96 | 100 |
| black | white | 2 | 224 (as Ambient white alone) | |
| (171, 60, 0) | (0, 127, 127) | 0 | (175, 131, 130) | |

A sum would have drawn 100 + 100 grey near 148, and 171 + 127 near 207,
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
  grey: 131 under ShadowMap, 127 without `Technology`); without
  `Technology`, a grey ambient on white draws its own value, as Rook's
  does. The Ambient ramp above (128 drew 134) was read in a place without
  `Technology` too, so one of the two is off by a few levels; not
  chased.

Rook (`ambient_color` in `shaders/lighting.wgsl`) takes the per-channel
max of the two colours on every face, and the local lights' tone curve
starts from it. In the measured scene (Brightness 0, Ambient 171,
OutdoorAmbient 127) `rook-player --screenshot` draws the wall and floor at
171, where it drew about 207.

## Method

1. A fresh Studio on an rbxlx with only Workspace and Lighting (no
   `Technology`), set dark through MCP: Brightness 0, Ambient and
   OutdoorAmbient black, both environment scales 0, ClockTime 0.
2. A 400 × 1 × 400 white SmoothPlastic floor, the light in an invisible
   1-stud part at (2, 6, 2) (so that the floor's sample plane, y = 6,
   holds voxel centres), and two red Neon markers at (26, 0.1, −20) and
   (−22, 0.1, −20) for the pixel scale.
3. `screen_capture` from (2.001, 60, 2) looking at (2, 0, 2), 55° field of
   view, after waiting 2–3 seconds: the grid fills asynchronously, and a
   capture right after a change shows half the old pool.
4. Read floor profiles with System.Drawing and a small C# helper; convert
   pixels to light units through the Ambient ramp.

After a big camera move, or in a freshly opened place, the grid can take
several seconds to fill: capture twice before trusting a dark result.
Studio's viewport only renders while its window is visible; captures of a
covered window come back blank or time out.
