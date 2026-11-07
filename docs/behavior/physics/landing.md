# Landing

Checked against Studio 0.745 on 2026-11-06.

- A part dropped flat stays flat. Over 0.5, 2, 5, 12 and 30 studs, 2 x 2 x 1
  boxes leaned at most 1.62 degrees and 4 x 1 x 4 boxes at most 0.26, and
  rose as elasticity says (Plastic on Concrete, about 0.3).

## In Rook

- Rapier solves bounce after the step, one pass over a contact's points in
  turn, which can't be configured. The first points turn the body and the
  last can't undo it, so a box dropped 2 studs ended on its side, and more
  solver iterations changed nothing.
- The contact hook merges the points of a contact that has only just begun
  into one at their middle for its first step, so the bounce goes through
  the middle. Tilted landings have one or two points and rarely change.
- Hooked pairs lose Rapier's contact reuse: on every moving body that
  doubled a large place's server step (11.5 to 23 ms, and 30 to 67 ms). So
  only bodies gravity alone moved over the last step have the hook. A body
  landing is one of those, one resting or driving along isn't, and the
  same places then showed no change over three rounds.
- Approximation: a body a force or mover pushes in the air isn't noticed,
  and lands as Rapier has it.
