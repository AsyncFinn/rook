# GUI input

Checked against Studio 0.743 on 2026-10-22.

[M8](../../roadmap/m8-gui.md) step 5. Measured in Studio 0.743 on 2026-10-22 in a
playtest client on Baseplate (Deferred signals), with clicks, the wheel
and keys sent through the MCP's `user_mouse_input` and
`user_keyboard_input`, and every event logged by a client script. The
conformance cases `input/gui_buttons`, `input/gui_overlap`,
`input/gui_wheel` and `input/textbox` hold the decisive orders; Rook
passes all four.

Code: hit-testing in `rook-gui` (`hits.rs`, walking `order.rs`, the
draw order), the wheel and bar drags in `scroll_input.rs`, tints and
hover images in `draw/frame.rs` and `draw/image.rs`, the caret in
`draw/caret.rs`; the events in `rook-engine/src/bridges/input/`
(`gui.rs`, `textbox.rs`); TextBox focus in
`rook-lua/src/api/methods/textbox.rs`.

## Coordinates

- The MCP's mouse points, mouse InputObjects' `Position` and
  `GetGuiObjectsAtPosition` are in GUI coordinates, below the top bar.
- The x and y that `MouseEnter`, `MouseLeave`, `MouseMoved`,
  `MouseButton1Down`/`Up` (and button 2's) and the wheel events pass
  include the top bar: a press at GUI (150, 67) passed (150, 125).

## What the cursor is over

- **Hit-testing** follows the drawing order ([layout](layout.md#drawing)),
  top first, as `GetGuiObjectsAtPosition` lists it. That uses the last
  frame's layout: a Frame parented just before isn't listed. An object is hit
  where it's drawn: inside its turned rectangle (a button turned 45° took
  a click inside its turned shape and not one inside its unturned
  corner), and only in the part its ancestors' `ClipsDescendants` leave.
- **Not hit**: objects in a disabled ScreenGui, under an ancestor with
  `Visible` off, or with `Interactable` off (no events at all, and
  `gameProcessedEvent` false).
- **Hover** reaches every object under the cursor, not just the top one:
  two stacked buttons both got `MouseEnter`, and a button under an
  `Active` Frame still did.
- **Defaults** of `Active`: true for TextButton, ImageButton and TextBox;
  false for Frame, TextLabel, ImageLabel, ScrollingFrame, ViewportFrame,
  VideoFrame and CanvasGroup.

## Buttons

- **Which button**: the topmost GuiButton under the cursor that is
  `Interactable` takes the press, whatever is over it (a Frame, `Active`
  or not, opaque or fully transparent, or a TextLabel) and whatever its
  own `Active`. A non-interactable button over another lets the lower one
  take it.
- **Press** fires, in order: `MouseButton1Down`, then `InputBegan`
  (MouseButton1, Begin) on each object under the cursor, top first, then
  `GuiState` Press, then UserInputService's `InputBegan`.
- **Release** over the pressed button: `MouseButton1Click`,
  `MouseButton1Up`, `Activated` (the InputObject and a click count:
  0, 1, 2 for quick repeats), `InputEnded`, `GuiState` Hover, then
  UserInputService's `InputEnded`.
- **Dragging off** a pressed button: `MouseLeave` and `GuiState` Idle;
  back on it, `MouseEnter` and Press again, and releasing there still
  clicks and activates. `Activated`'s click count goes up while
  activations come within about half a second of each other (0.465 s
  counted, 0.534 s didn't). Released over another button: that one fires
  `MouseButton1Up` only, the pressed one `InputEnded`; no Click or
  `Activated` anywhere.
- **Button 2** does the same with `MouseButton2Down`, `Up` and `Click`,
  and its own Press state; it never fires `Activated`.
- **GuiState** is Idle, Hover or Press on every GuiObject, Frames too
  (a Frame went Hover, Press, Hover, Idle under a click), and
  NonInteractable with `Interactable` off.
- **AutoButtonColor** never shows in `BackgroundColor3`: it's a tint
  applied when drawing. Hovered, the background is multiplied by 0.7;
  pressed, it's mixed 30% towards white (white stays white), in sRGB.
  Measured from `screen_capture` shots of buttons on a SurfaceGui,
  hovered and pressed through the world, read against a strip of known
  greys: white hovered read 179, (200, 100, 50) hovered (140, 70, 35)
  and pressed (216, 147, 110), grey 30 hovered 21 and pressed 98.
- **Mouse movement**: entering an object fires `MouseEnter`, `InputBegan`
  (MouseMovement, state Change) and `GuiState` Hover; leaving fires
  `MouseLeave`, `InputEnded` (MouseMovement) and Idle; moving over it
  fires `MouseMoved` and `InputChanged`.

## The world behind

- **gameProcessedEvent** is true for a mouse event when a button (even
  with `Active` off) or an `Active` object is under the cursor (an Active
  TextLabel, a fully transparent Active Frame), false over only inactive
  ones (a plain TextLabel or Frame).
- **ClickDetectors** behind an `Active` object (Frame, transparent or
  not, or a button) don't get the click; behind inactive ones they do.
  Their hover kept going while the cursor was over an Active Frame (no
  `MouseHoverLeave`).

## The wheel

- Over an object the wheel fires `MouseWheelForward` or `Backward` on
  it, with `InputChanged` (MouseWheel, `Position.Z` ±1); the first wheel
  step on a button came as `InputBegan` instead (Observed once).
- **ScrollingFrames** move 100 pixels per notch, eased: e^(−25 t) of the
  way is left after t seconds (97.7 of 100 pixels after 0.15 s). The
  wheel goes to the innermost ScrollingFrame under the cursor that can
  still move that way, then to its ancestors: an inner frame at its end
  passed the notch to the outer one, but one still easing towards its end
  took the next notch itself. A frame whose canvas is only wider scrolls
  sideways (its vertical bar doesn't show, though the horizontal bar
  leaves the window a little shorter than the canvas). With
  `ScrollingEnabled` off a frame takes no wheel.
- `gameProcessedEvent` for the wheel is true when it scrolled something
  or an `Active` object is under the cursor (false over a frame with
  `ScrollingEnabled` off).
- **Dragging a bar's thumb** moves the canvas by canvas ÷ window along
  the bar per pixel the mouse moves (16.04 for a 3,000 canvas in a 187
  window; 50 pixels down a 200 window over 1,000 moved it 250), and keeps
  going when the cursor leaves the bar. Clicking the track beside the
  thumb does nothing.

## TextBox

- **Focus** by clicking: with `ClearTextOnFocus` (the default) `Text`
  empties first, then `CursorPosition` becomes 1, then `Focused`, then
  UserInputService's `TextBoxFocused`. Without it the caret goes to the
  end (`CursorPosition` = length + 1) and `SelectionStart` is −1.
- **CaptureFocus** takes effect at once (`Text` empty and `IsFocused`
  true when it returns), with the same events. A box losing focus to it
  keeps its `CursorPosition` and fires `FocusLost(false, nil)` but no
  `TextBoxFocusReleased`. A new TextBox's `CursorPosition` is 1 and its
  `SelectionStart` −1.
- **Typing** comes from the platform's text input only: a key press of C
  typed nothing. Each character moves `CursorPosition` first, then
  changes `Text`. Backspace and Left edit and move the caret.
  Keyboard events still reach UserInputService, with
  `gameProcessedEvent` true.
- **Losing focus**: `CursorPosition` becomes −1, then `FocusLost`, then
  `TextBoxFocusReleased`. Enter passes (true, the Return InputObject),
  Escape (false, the Escape InputObject), clicking elsewhere (false, the
  MouseButton1 InputObject), `ReleaseFocus(true)` (true, nil) and
  `ReleaseFocus()` (false, nil). The text stays as typed. Enter's own
  `InputBegan` comes after, with `gameProcessedEvent` true; its
  `InputEnded`, with no box focused, false. Escape's `InputEnded` is
  still processed.
- **Tab** does nothing: the box stays focused. In a `MultiLine` box,
  Enter types a newline and keeps focus.
