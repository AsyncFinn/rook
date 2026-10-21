# Surface joints (`MakeJoints`)

Checked against Studio 0.741 on 2026-10-11.

Observed in Studio, where `MakeJoints` still works;
`rook_physics::joints::surfaces`.

- Two faces join when they face each other within 0.01 studs (gap or
  overlap), their edges line up (within about half a degree) and they
  overlap with some area (touching along an edge isn't enough).
- A Hinge, Motor or SteppingMotor surface on either face makes a Rotate,
  RotateV or RotateP; the calling part's wins. Otherwise Weld on either
  side makes a Weld, then Glue a Glue, then Studs, Inlet or Universal on
  both sides a Snap, except Studs to Studs and Inlet to Inlet.
- The joint is named after its class, parented to Part0 and framed at
  Part0's face center with Z along the face's outward normal. Part0 is
  the part whose rotating surface decided the type, otherwise the
  calling part. Parts already joined aren't joined again.
- Only parts in Workspace join; they may join parts anywhere in it.
- A wedge's slope is a face; its box's top and front aren't. A wedge
  calling MakeJoints joins what's on its slope only through its own
  Weld surfaces, TopSurface before FrontSurface, framed at that box
  face. A part calling it on a slope sees the wedge's FrontSurface
  there. Rotating surfaces on either side of a slope join nothing.
- When both parts of a pair call (a model's MakeJoints), the earlier
  tries first and the later tries again if that made no joint, so a
  slope joins if either side would (Part0 is then the later caller).

Every other part is treated as its box (a wedge's side triangles as
rectangles). Balls, cylinders and corner wedges are boxes too: in
Studio a ball's Hinge makes a Rotate and its Studs a Snap, and a
cylinder's caps and curved side join, all framed at the box face.
