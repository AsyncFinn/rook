# Highlight

ROADMAP M7 step 6. In 68 corpus files (42 create one with
`Instance.new`). Measured in Studio 0.742 on 2026-10-20 on Baseplate, a
grey 4-stud cube seen square on at a field of view of 8°, 49 to 300
studs away, in a 1253 × 773 viewport. Unlike the other adornments,
Studio's MCP capture shows Highlights, through walls included.

## What it covers

- **`Adornee`**, or with none its parent (Observed). A part's Highlight
  covers only that part, not parts inside it. A Model's covers all its
  parts, outlined as one shape: two overlapping parts drew one outline
  around both, none between them. A Highlight inside a Folder, or
  adorning one, draws nothing.
- **Where it is**: with an `Adornee` it draws wherever it lives
  (ReplicatedStorage and Lighting both worked).
- `Transparency` 1 parts aren't highlighted.
- `Enabled` off hides it (API reference). Roblox draws at most 31 at once
  (API reference); Rook draws the first 31 by the order it numbers them
  in.

## How it draws

- **Fill**: exactly `FillColor`, mixed into the finished screen's pixels
  by 1 - `FillTransparency` (default 0.5), past the tone curve: red at
  0.5 over green (62, 154, 91) drew (158, 77, 47), the plain average.
  Faces keep their shading under a see-through fill.
- **Outline**: `OutlineColor` (default white) at `OutlineTransparency`
  (default 0), two pixels wide, one each side of the covered shape's
  edge, the same width 49 and 148 studs away. Only the shape's outer
  edge: no lines between faces or touching parts.
- **`DepthMode`** AlwaysOnTop (default): fill and outline show through
  parts in front. Occluded: only where the parts are seen, and the
  outline follows the edge of what's seen (it ran along the top of the
  wall hiding the cube's lower half).

## In Rook

The player gives each covered part's entry its Highlight's look
(`apps/rook-player/src/scene/adornments/highlight.rs`); the nearest
Highlight wins if several cover a part (Inference). The renderer draws
the covered parts into a mask of Highlight numbers after the main pass,
tested against its depth for Occluded ones, and the tone pass mixes the
fill over each covered pixel and the outline over each pixel whose
neighbour across or up and down belongs elsewhere
(`crates/rook-render/src/renderer/highlights.rs`,
`shaders/tonemap.wgsl`). The same scene in `rook-player --screenshot`
puts the outlines on Studio's pixels, and the fill at (190, 72, 78) to
Studio's (193, 72, 81) over the cube (the cube's own light differs,
M13).

Not measured: how the outline mixes over a see-through fill, the
outline at other window sizes and screen scales, and a Highlight in
Workspace without an `Adornee` (Rook doesn't draw it).
