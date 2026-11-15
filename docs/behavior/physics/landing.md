# Landing

Checked against Studio 0.745 on 2026-11-06.

- A part dropped flat stays flat. Over 0.5, 2, 5, 12 and 30 studs, 2 x 2 x 1
  boxes leaned at most 1.62 degrees and 4 x 1 x 4 boxes at most 0.26, and
  rose as elasticity says (Plastic on Concrete, about 0.3).

## Grazing an edge

Measured in Studio 0.747 on 2026-11-12: a 2-stud Plastic ball dropped
past an anchored 2 × 1.2 × 4 brick, overlapping the brick's side by a set
amount, server-owned.

- Clear of the brick (0.01) or overlapping by 0.0036, it drops straight
  past and stays where it lands, whether the brick is up in the air or
  standing on the floor beside where the ball lands.
- Overlapping by 0.06 to 0.2 it catches the brick's top edge, picks up
  10 to 15 rad/s of spin and rolls 15 to 25 studs away.
- In between it depends on where the 240 Hz steps fall: at 0.03 it caught
  from drops of 0.05, 1 and 4 studs and missed from 0.3 and 16; at 0.05 it
  missed from 5.
- A ball rolling slowly off a ledge curls over the edge at once (its
  center drops as the sphere's would pivoting on the edge), so edges
  aren't ignored, only shallow brushes in passing.

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
- Rapier stops every overlap at once, and extends a contact in a straight
  line within the step, so a ball passing an edge it barely overlaps (or
  misses by less than Rapier's 0.02-stud lookahead) took a sideways
  shove and spin from it. The hook lets a falling body's contacts sink
  0.05 studs before they push back, which lets through the brushes Studio
  ignores and still catches overlaps of 0.06 or more (Approximation: the
  in-between cases always pass). A body that has just landed keeps the
  hook one more step without that allowance, because Rapier goes on
  reusing a pair's last contacts once the hook is off, and those must not
  carry it; the landing merge above had the same problem.
