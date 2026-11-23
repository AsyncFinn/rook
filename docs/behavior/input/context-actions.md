# Context actions

Checked against Studio 0.747 on 2026-11-10.

[M19](../../roadmap/m19-input-and-platforms.md) step 1. Measured in
Studio 0.747 on 2026-11-10 in a playtest client on Baseplate, with a
binding on a DualShock's ButtonX and the PlayerModule's own bindings,
every UserInputService and ContextActionService event logged by a client
script. Earlier rules (priority, the stack, `Pass`) follow creator-docs.

## Order

- A binding hears an input before UserInputService does: the binding's
  function ran, then `InputBegan`, at the same instant.
- An input a binding sinks reaches `InputBegan`, `InputChanged` and
  `InputEnded` with `gameProcessedEvent` true. ButtonX bound and sunk
  gave true; ButtonA's `InputEnded` was true once the PlayerModule's jump
  binding held it, while its `InputBegan`, before the binding existed,
  was false.
- Changes reach bindings too: the PlayerModule's thumbstick bindings sink
  `InputChanged`, which then arrives processed.
- `BindActivate` doesn't sink: ButtonR2, bound by the PlayerModule's
  `BindActivate`, arrived unprocessed.

## Cancel

- Opening Roblox's menu (Escape) called the ButtonX binding with
  `UserInputState.Cancel` seven times in a row, each with a fresh
  InputObject of type None, KeyCode None and Position zero. Why seven,
  for a binding of one input, isn't known yet.

## How Rook does it

Code: `rook-lua/src/api/luau/context_action.luau` keeps the bindings;
`rook-engine/src/bridges/input.rs` offers each InputObject to them
(`DataModel::deliver`, in `bridges/input.rs`) before firing UserInputService's event.

- The menu's Cancel isn't sent yet, by Roblox's CoreScripts' menu or
  [Rook's own](../gui/menu.md); it belongs to M19 step 4 with the rest of
  ContextActionService.
