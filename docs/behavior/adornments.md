# Adornments

ROADMAP M7 step 6. Measured in Studio 0.742 on 2026-10-20 on Baseplate.
SelectionBox is saved in 676 corpus files and made by scripts in 749.

## SelectionBox

- **It draws only on its `Adornee`** (Observed): a SelectionBox inside a
  part with no Adornee draws nothing; one anywhere in Workspace with
  `Adornee` set draws on that part. Rook also draws those under a
  PlayerGui (Inference: adornments draw there in Roblox).
- **Edges** (Measured, a 4-stud cube, LineThickness 0.5, seen square on
  at a 3° field of view): each edge is a bar `2 × LineThickness` across,
  centred on the part's edge, so the outline reaches `LineThickness`
  past the part (2.5 studs from the centre) and `LineThickness` in (1.5).
- **Colour**: `Color3` (default (13, 105, 172)) without light; red
  (1, 0, 0) draws (222, 0, 0), as a flat colour does through the tone
  curve. `Transparency` fades the lines.
- **Fill**: `SurfaceColor3` over the part's box at `SurfaceTransparency`
  (default 1: none); blue at 0.5 over grey 128 draws (25, 35, 190).
- **Depth**: parts in front hide it (it isn't drawn on top).
- `Visible` off hides it.

Rook draws it as unlit boxes with the part (`apps/rook-player/src/scene/
adornments.rs`): twelve bars and a fill box 0.02 studs outside the
part's faces, so the two don't flicker. The X bars cover the corners and
the others stop at them, so a see-through box doesn't darken its corners
(Inference: not checked in Studio). The same scene in `rook-player
--screenshot` lands the outline and fill on the same pixels as Studio's
capture, in the same colours. Not yet: a Model as Adornee (its bounding
box).
