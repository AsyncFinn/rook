# Scene queries

Checked against Studio 0.742 on 2026-10-12.

Rays, shape casts, regions, overlaps and touching parts
(`rook_physics::query`).

Queries see the current DOM, not last frame's physics: scripts often
create or move a part and query in the same frame (Observed: a ray hits a
part created or moved just before it). So every query first brings the
world up to date, then puts the colliders of bodies moved or rebuilt
since the last step where they are now, in the broad phase too (Rapier
only does that when it steps). Filters (`CanQuery`, ignore lists,
collision groups) are read from the DOM per candidate, so they're always
current as well.

Observed in Studio 0.742 unless noted:

- A part is skipped when both `CanCollide` and `CanQuery` are off
  (`CanQuery` alone does nothing while `CanCollide` is on).
  `Transparency` doesn't matter; anchored and unanchored parts count.
- Rays don't hit a part they start inside (or on the surface of), and
  reach at most 15,000 studs (`query::MAX_RAY_LENGTH`).
- Region3 and the `GetPartBounds*` queries test each part's `Size` box
  (`shape::world_box`); touching faces count. `GetPartsInPart` and
  `GetTouchingParts` test real shapes and need the parts to overlap.
- Terrain: rays and shape casts hit its surface, water's too unless they
  ignore water, and report Terrain with the material hit; a ray starting
  inside it hits nothing until it leaves. Region3, the `GetPartBounds*`
  queries and `GetPartsInPart` never return it; `GetTouchingParts` does
  where a part overlaps its solid surface, whatever the part's
  `CanCollide`.
