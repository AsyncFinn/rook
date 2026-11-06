# Joints

Checked against Studio 0.746 on 2026-11-06.

## When joints take effect

`conformance/cases/physics/joint_snap.luau`; `rook_physics::sync::settle`.

A joint moves nothing when it's made or changed. What moves a part is a look
at it:

- Reading the `CFrame`, `Position`, `Orientation` or `Rotation` of a part that
  isn't its assembly's root brings that one part to where the root holds it.
  Reading the root changes nothing. An Attachment's `WorldCFrame` and
  `WorldPosition`, and a model's pivot and bounding box, read their parts.
- A spatial query (`Raycast`, `GetPartBoundsInBox`) or the next step brings
  every part.
- Other reads (`AssemblyMass`, `AssemblyRootPart`, `GetConnectedParts`,
  `IsGrounded`) move nothing. `AssemblyCenterOfMass` reports where the parts
  belong without moving them: 997.5 for parts stored at 1000 and belonging at
  995 and 1000.

So what a script does before that still counts. With a weld between two
unanchored parts, anchoring Part0 straight after it makes Part1 move, unless
Part0 has already been read.

- *Which side moves.* The root stays and the rest of the assembly comes to it.
  An anchored part is the root, then the larger, then with equal parts Part1.
- *Which joints.* Weld, Snap, Glue, ManualWeld, Motor and Motor6D. Rotate,
  RotateV, RotateP and WeldConstraint leave their parts where they are.
- *What counts as a change.* `Part0`, `Part1`, `Enabled`, `C0` and `C1`, the
  joint or a part entering Workspace, and `Anchored`. Two anchored parts don't
  move, until one is let go. A joint outside Workspace or disabled does
  nothing, and one destroyed before the read never did. `Transform` waits for
  the step.

## Surface joints (`MakeJoints`)

Observed in Studio 0.741, where `MakeJoints` still works;
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
