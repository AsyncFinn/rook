# Antialiasing

Measured in Studio 0.741 on 2026-10-13, for when Rook adds antialiasing.
Rook draws with one sample per pixel today, so distant edges crawl a whole
pixel at a time as things move (a disaster place's rising water, seen from far away).

## Findings

| Quality level | Antialiasing |
|---|---|
| Automatic (Studio's edit viewport) | None |
| Level10, Level16 | None |
| Level17 | 2× MSAA (one in-between shade on edges) |
| Level18, Level19, Level21 | 4× MSAA (three in-between shades) |

- It is multisampling, not a post-process filter: edge pixels take only a
  few fixed coverage values (1 for 2×, 3 for 4×), and nothing is blurred
  away from edges.
- No place or script controls it. There is no antialiasing property on
  Workspace, Lighting or `settings().Rendering`; it follows the quality
  level only. (`Enum.AntiAliasing` exists, with Disabled and Enabled, but
  nothing found here uses it.)
- No level gave 8×.
- At 4× the three edge values measured about 0.19, 0.44 and 0.75 of the
  way from background to object, not an even ¼, ½, ¾. Which colour space
  the samples are averaged in isn't settled, because Studio's captures
  are JPEG.

Inference: levels 1 to 9 and 11 to 15 weren't measured; they're assumed
off, like 10 and 16. Level20 wasn't measured either; it's assumed 4×,
like the levels on both sides of it.

Not measured yet:

- How the in-game Graphics Quality slider (1 to 10, and Automatic)
  maps onto these levels.
- What Automatic picks on other hardware.
- Mobile.
- Whether transparent parts, decals or GUIs are multisampled.

## Method

1. In Edit, build a black SmoothPlastic 8×8×1 part, rolled 7°, in front
   of a large white SmoothPlastic backdrop, far from everything else.
   Don't use a Neon backdrop: Neon glows at high quality levels and blurs
   the edge.
2. Set `settings().Rendering.EditQualityLevel` to a level, wait a second,
   and take a screen capture through Studio's MCP server, pointed straight
   at the part. Captures come back 1:1 with `Camera.ViewportSize`, so the
   image isn't rescaled.
3. For each pixel near the part's edges, compute coverage as
   (background − pixel) / (background − part), and histogram the values
   between 0.04 and 0.96 in sixteenths. Count the clusters: none means no
   antialiasing; N − 1 clusters means N samples. JPEG noise only adds a few
   stray counts next to the real clusters.
4. Afterwards, put `EditQualityLevel` back to Automatic and delete the parts.

## For Rook

The simplest match is a quality setting that picks 1, 2 or 4 samples:
1 below Level17, 2 at Level17 and 4 from Level18 up. That means a
multisampled colour and depth target for the main pass, resolved into the
window or screenshot image, and every pipeline built with that sample
count.
