# Scene queries

Checked against Studio 0.746 on 2026-11-06.

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
  (`shape::world_box`); touching faces count. Rook finds the candidates
  in an index of every part's `Size` box (`query/bounds.rs`), so a
  turned cylinder or wedge is found near its box's corners, where its
  shape's own bounds don't reach. `GetPartsInPart` and
  `GetTouchingParts` test real shapes and need the parts to overlap.
- Terrain: rays and shape casts hit its surface, water's too unless they
  ignore water, and report Terrain with the material hit; a ray starting
  inside it hits nothing until it leaves. Region3, the `GetPartBounds*`
  queries and `GetPartsInPart` never return it; `GetTouchingParts` does
  where a part overlaps its solid surface, whatever the part's
  `CanCollide`.

## Include and exclude lists

`RaycastParams` and `OverlapParams` take `ExcludeInstances` and
`IncludeInstances` beside the older `FilterDescendantsInstances` and
`FilterType`. Measured in Studio 0.748 on 2026-11-22 with rays through
walls, one inside a Folder:

- Both read nil until set, and can be set back to nil.
- Every list narrows what the query can hit, so they intersect: with
  `FilterType` Include over a Folder and `IncludeInstances` naming a wall
  outside it, nothing is hit.
- Exclusion wins: a part both included (by either list) and excluded is
  skipped.
- Descendants count, as for `FilterDescendantsInstances`: naming a Folder
  covers the parts in it.
- An empty `IncludeInstances` hits nothing.
- An assigned table is copied: adding to it afterwards changes nothing,
  and reading the property gives a new table.

## WorldModels

Observed in Studio 0.747 on 2026-11-14:

- A WorldModel is a world of its own. Its queries find only the parts
  whose nearest WorldRoot it is, also inside Models in it, and not those
  of a WorldModel inside it, which its own queries find. A part directly
  in a ViewportFrame, outside the WorldModel, isn't found.
- Workspace's queries never find a WorldModel's parts, even a WorldModel
  in Workspace; nothing simulates them (an unanchored part in one stays
  put, a part on its floor touches nothing).
- A WorldModel out of the DataModel finds nothing; in it (a ViewportFrame
  in a ScreenGui, say), it finds its parts where they are now, moved or
  not, by the same rules as Workspace (`CanQuery` off with `CanCollide`
  on still counts).

Rook gathers a WorldModel's parts and shapes when it's first queried and
again only after something in it changed, which the journal Physics
reads anyway tells (`query/world_models.rs`); a WorldModel holds a model
or two, so its queries test its parts one by one. Workspace's world is
the parts under Workspace with no WorldModel between
(`shape::in_workspace_world`).

## Model:MoveTo

`conformance/cases/physics/move_to.luau`; `Physics::rise_out_of_parts`.

- The model moves so its PrimaryPart, or its bounding box's center without
  one, is at the target, then up until nothing is in its way. All of it moves
  together, by the highest lift any of its parts needs.
- Each overlap lifts it to where it would rest on that part dropped from
  above, by real shapes: a ball over a block's edge sinks past the corner, a
  turned cube rests on its edge.
- *What's in the way.* Parts with `CanCollide`, anchored or not, in a group
  the model's part collides with. `CanTouch` and `CanQuery` make no
  difference. Terrain and water never are. Of the model's own parts, those
  with `CanCollide` off aren't tested.
- *How much counts.* A face merely meeting another doesn't lift it, from
  above or the side; sinking 0.0001 does.
- *Rising.* The check repeats from where it stopped, so a gap too small is
  passed. A gap counts as too small unless it's about 0.015 taller than the
  model: 0.01 is passed, 0.02 stops it.
- Outside Workspace, or empty, the model stays where it is and nothing is
  raised.

## The mouse

`conformance/cases/input/mouse_rules.luau`, on a Studio client.

- `Mouse.Target` and `Mouse.Hit` come from a ray 2,048 studs long from the
  camera: a face at 2,048 is hit, one at 2,049 isn't.
- On a miss `Target` is nil and `Hit` sits 10,000 studs along the ray.
- It sees what legacy rays see. Parts with `CanCollide` off count, and so do
  see-through ones, whatever else is set; a part with `CanCollide` and
  `CanQuery` both off doesn't. `CanTouch` makes no difference.
- `TargetFilter` leaves out an instance and everything in it.
- So does the local player's `Character`: the mouse looks through the
  whole model, accessories and any part put in it included, from any
  camera, so a click reaches what's behind it (measured in Studio 0.747,
  2026-11-11). Another character, such as an NPC, blocks it.
- Acting as the `Default` group, as legacy rays do, is Inference: a client
  case can't register a group.
