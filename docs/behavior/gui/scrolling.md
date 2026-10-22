# Scrolling frames

Checked against Studio 0.743 on 2026-10-22.

[M8](../../roadmap/m8-gui.md) step 4. Measured in Studio 0.743 on 2026-10-22 on Baseplate:
`AbsoluteWindowSize`, `AbsoluteCanvasSize`, `CanvasPosition` and children's
places read by script (edit mode and a playtest client), and the drawing
from `screen_capture` shots of ScrollingFrames on a SurfaceGui, as for
[GUI images](images.md). The conformance case `gui/scrolling` holds the
decisive values; Rook passes it, and its shots of the same scene differ
only along edges.

Code: `rook-gui`'s `layout/scrolling.rs` (canvas, window, bars, position)
and `draw/scrolling.rs` (the bars); scripts' `CanvasPosition` writes go
through `Gui::set_canvas_position`.

Defaults from `Instance.new`: `CanvasSize` {0, 0}, {2, 0},
`ScrollBarThickness` 12, ScrollingDirection XY, both insets None,
VerticalScrollBarPosition Right, `ScrollBarImageColor3` white at
transparency 0, ElasticBehavior WhenScrollable, `ClipsDescendants` on, and
the install's `textures/ui/Scroll/scroll-top.png`, `scroll-middle.png`
and `scroll-bottom.png` (white at half opacity, 14 pixels wide, the ends
rounded in their inner halves) as `TopImage`, `MidImage` and
`BottomImage`.

## The canvas

- **CanvasSize** scales with the ScrollingFrame's parent, as `Size` does
  (the default in a 400 × 300 parent: 600 high).
- **AbsoluteCanvasSize** is `CanvasSize`, but never smaller than the frame,
  less a bar's thickness where that bar's inset is Always, or ScrollBar
  while the bar shows (a 50 × 50 canvas in 200 × 100 with both insets
  Always: 188 × 88). Negative sizes count as 0.
- Children are placed on the canvas: their scales are of
  `AbsoluteCanvasSize` (inside its UIPadding and border,
  [layout](layout.md#layout)), and the canvas sits at the
  frame's top left less `CanvasPosition`, on whole pixels (a
  `CanvasPosition` of 10.6 moved children 11). With the vertical bar on
  the Left and its inset applying, the canvas starts after it.
- **AutomaticCanvasSize** grows the canvas on its axes to the far edge of
  everything visible inside it, grandchildren too, with the padding past
  them, never below `CanvasSize` (a child reaching 350 with
  `CanvasSize` 500 left 500; a grandchild at 400 in a child at 0 gave
  450). Hidden ones don't count. With a UI layout, that's its content.
  Inference: what's inside a ScrollingFrame inside counts by that frame's
  own rectangle. Studio's results for children placed by scale in a
  growing canvas depend on earlier layouts; Rook places them once on the
  canvas before it grows, then again on the grown one (Approximation).

## The window and the bars

- A bar shows when the canvas (before it's widened to the frame) is bigger
  than the window that way, the other bar's room counting: a 195 × 400
  canvas in 200 × 100 shows both, since the vertical bar leaves 188.
  ScrollingDirection X shows only the horizontal bar and Y only the
  vertical; `ScrollingEnabled` off still shows them. Inference: a bar 0
  thick doesn't show.
- **AbsoluteWindowSize** is the frame less, for each bar shown, its
  thickness and the frame's `BorderSizePixel` (a 1-pixel border made
  187 of 200).
- **CanvasPosition** is kept between 0 and the canvas less the window
  when a script sets it, laid out first (set past the end of 300 × 400 in
  a 188 × 88 window: 112, 312). Set before the canvas is laid out, it's
  kept within what the canvas was then. A canvas that shrinks pulls it
  back at a later layout (Studio does so a frame or two later; Rook at the
  next layout). ScrollingDirection doesn't keep it from moving.
- Not yet: scrolling by mouse wheel, touch or dragging (with input, step
  5), and `ElasticBehavior`, which only shows while scrolling.

## Drawing

- Children are clipped to the window, not the frame, so they never cover
  the bars. With `ClipsDescendants` off they aren't clipped at all and
  draw over the bars, which are drawn with the frame, under its children.
- A border draws a line of `BorderColor3`, its width, between each bar and
  the window, across the whole frame.
- The bars have no track: the frame's background shows. A thumb is as long
  as the window's share of the canvas times the window, but at least twice
  the thickness, and moves along the window's length as `CanvasPosition`
  moves through its range. It's drawn in `ScrollBarImageColor3` at
  `ScrollBarImageTransparency` from three images: `TopImage` and
  `BottomImage` a thickness square at its ends, whatever their shape,
  `MidImage` stretched between. The horizontal bar's images are the same
  turned a quarter left, `TopImage` at the left end.
