# GUI selection

Checked against Studio 0.747 on 2026-11-11.

What moving `GuiService.SelectedObject` or `SelectedCoreObject` fires, and
what `GuiService:Select` picks. Measured in Studio with a LocalScript on a
client, the place's signals deferred. Pads moving the selection, the
selection image and `NextSelection*` are M19 step 3's.

Code: the events in `rook-lua/src/api/properties/selection.rs` (every
script write passes there), the pick in `rook-gui/src/selection.rs`, and
`Select` in `rook-lua/src/api/methods/gui.rs`.

## Moving the selection

From A to B (either may be nil):

1. `SelectionLost` on A, then `SelectionGained` on B.
2. The property's change (`GetPropertyChangedSignal`).
3. `SelectionChanged(amISelected, previous, new)` on A, then on each of
   A's ancestors that is GUI (GuiBase2d, the ScreenGui included), nearest
   first; then on B and those of its ancestors not reached yet. Only B
   gets true. An ancestor of both fires once, in A's pass.

Setting the selection to what it is fires nothing.

Roblox's top bar depends on it: when the pad focuses the top bar, it
selects its own button with `Select`, and it gives up its focus (and the
dark overlay behind it) when `SelectionChanged` says the selection left
it. The settings menu clears `SelectedCoreObject` as it closes, so
without these events the overlay stayed after Escape.

Not measured yet (Rook keeps the object): Studio warns "Setting
GuiService.SelectedObject to invalid GuiObject" and refuses some objects;
a hidden or destroyed selected object stayed selected 0.1 s later.

## Select

`GuiService:Select(parent)` selects one of `parent`'s descendants that is
`Selectable`, shown (it and every ancestor `Visible`), `Interactable`, and
on screen. Of those, the one with the highest top wins, then the leftmost,
then the one drawn on top on an exact tie (higher `ZIndex`, else the later
sibling). Measured cases:

| Buttons (x, y) | Picked |
| --- | --- |
| (300, 300), (10, 10) | (10, 10) |
| (400, 0), (0, 20) | (400, 0): top before left |
| (300, 100), (0, 100) | (0, 100) |
| two at (100, 100) | the later child |
| two at (100, 100), the first with ZIndex 5 | the first |
| hidden, or inside a hidden Frame | skipped |
| `Selectable` false, or `Interactable` false | skipped |
| `Active` false, or fully transparent | still picked |
| a Frame with `Selectable` true | picked |
| at x = -5000 | skipped |
| inside a nested Frame | picked: any depth |

With nothing to pick, the selection stayed nil. Roblox's own scripts get
`SelectedCoreObject` set (Inference: the top bar reads its selection back
there); a game's get `SelectedObject`.
