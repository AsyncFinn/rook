# Tools and the backpack

Checked against Studio 0.747 on 2026-11-12.

How a Tool is held, used, dropped and picked up: `rook_lua`'s
`methods::tools`, the input bridge in `rook_engine` and the session host.
Measured in a Baseplate playtest with an R15 player and R6 and R15 NPCs
made by `Players:CreateHumanoidModelFromDescription`, with server Scripts
and LocalScripts in the tools logging every event. The backpack hotbar
and its number keys are Roblox's own CoreScript
([core scripts](../gui/core-scripts.md#health-bar-and-backpack)).

## Equipped

A Tool is held while its parent is a model with a Humanoid (a character)
and it has a `Handle` part, or `RequiresHandle` is off.

- `Equipped` fires when a Tool becomes held and `Unequipped` when it stops.
  A Tool without a Handle whose `RequiresHandle` is on still moves into the
  character but fires neither, and can't be activated.
- Order on equipping: `ChildAdded` on the character, `AncestryChanged`,
  `Equipped`, then the Handle's `CanCollide` change, then `Parent`'s
  property signal. On unequipping: `ChildRemoved`, the grip leaving the
  hand, the Handle's `CanCollide` change, `Unequipped`, `AncestryChanged`,
  `Parent`'s signal. Rook does the unequip's grip and `CanCollide` before
  the move, so `ChildRemoved` comes after them (Approximation; the rest
  holds).
- On the client, `Equipped` passes a new `Mouse` (class `Mouse`, not the
  PlayerMouse `GetMouse` returns) each equip. It acts as the player's
  mouse while the tool is held: `Hit`, `Target` and the rest follow the
  cursor, and its `Button1Down` fires before the PlayerMouse's. Once
  unequipped it stops firing, and reading its properties errors with
  `This Mouse is no longer active`; writes still succeed. On the server
  `Equipped` passes a new, inactive Mouse. A tool Mouse's own
  `TargetFilter` isn't applied; `Hit` and `Target` follow the
  PlayerMouse's (Inference: not measured).
- `Humanoid:EquipTool` and `UnequipTools` on a character with no player:
  `UnequipTools` sets the Tools' Parent to nil (they aren't locked).

## The grip

Equipping welds the Handle to the right hand; nothing else on the arm
changes (the R6 `Right Shoulder` keeps its `C0` and `DesiredAngle`). The
raised arm comes from the Animate script's `toolnone` animation.

- A `Weld` named `RightGrip` in the hand: `RightHand` on an R15
  Humanoid, `Right Arm` otherwise, found by name (a renamed hand gets no
  grip). `Part0` is the hand, `Part1` the Handle.
- `C1` is the Tool's `Grip`, and follows it while held.
- `C0` on R15 is the hand's `RightGripAttachment.CFrame`; without one,
  (0, -0.2, 0) turned -90 degrees about X. On R6 it's always (0, -1, 0)
  turned -90 degrees about X, whatever the arm's size or its
  `RightGripAttachment`.
- The Handle's `CanCollide` turns off on equipping and on on unequipping,
  whatever it was before. Other parts of the tool keep theirs.
- An NPC's grip exists as soon as `EquipTool` returns. A player's own
  client makes its character's grip, which the server sees a few frames
  later (four Heartbeats); Rook's server makes the same weld at once and
  the client keeps the one it made, which the server's then stands for.

## Leaving the hand

Whenever a held Tool leaves the character, for any parent (the Backpack,
Workspace, a Folder, ReplicatedStorage), the grip goes and the Handle
moves to 4 studs up and 8 ahead of the character's root part, keeping its
rotation. Without a root part, the point is measured from another body
part (Inference: Rook uses the root part's fallback, as `Humanoid.RootPart`
does).

## Dropping

Backspace drops the held Tool into Workspace when its `CanBeDropped` is on
(the default); with it off nothing happens. The engine does this, not the
backpack CoreScript. A client's script setting its held Tool's Parent to
Workspace is a drop too: the server repeats it only when `CanBeDropped` is
on, and otherwise keeps the tool in the character while the client shows
it dropped. A LocalScript in the dropped tool stops before `Unequipped`;
the server's handlers hear it.

## Activating

- A left click while a Tool is held fires `Activated` on the press and
  `Deactivated` on the release, on the client and then the server. A
  right click does nothing. A click an Active GUI object takes (a
  TextButton) activates nothing; an inactive Frame lets it through.
- `ManualActivationOnly`: clicks fire only `Deactivated`;
  `Tool:Activate()` and `Deactivate()` still fire theirs.
- `Enabled` off: clicks and `Activate()` fire nothing, with no warning.
- `Tool:Deactivate()` on a Tool no one holds does nothing (Inference:
  Studio's warning was only seen from `Activate()`).
- `Tool:Activate()` and `Deactivate()` fire on the side that calls them
  and then on the other. On a Tool that isn't held, `Activate()` warns
  `Tool:Activate() called from script when tool is not equipped. Tool
  will not be activated.` and fires nothing.

## Picking up

A Tool anywhere in Workspace, outside a character, whose Handle a
player's character touches, is equipped by that character: a Tool it held
goes to its Backpack. An NPC's touch picks nothing up.

## HopperBin

Modern Roblox no longer runs HopperBins, and Rook follows it. A HopperBin
in StarterPack still reaches the Backpack and the hotbar shows its slot;
its number key calls `ToggleSelect` (RobloxScript capability), which only
flips `Active`. `Selected` and `Deselected` never fire, nothing equips,
and `BinType` does nothing. A LocalScript inside one runs in the Backpack.
