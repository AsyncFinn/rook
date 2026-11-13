# ProximityPrompts

Checked against Studio 0.744 on 2026-11-13.

[M8](../../roadmap/m8-gui.md) step 9. Measured in Studio 0.744 in play
mode on Baseplate, with the character anchored and the camera pinned by a
LocalScript, from the events' handlers and `AbsoluteSize` reads. The GUI
is Roblox's own `ProximityPrompt` CoreScript, from the user's install
([Roblox's core scripts](core-scripts.md)); it starts only when
`GetEngineFeature("ProximityPrompts")` is true. In a same-scene
comparison the prompts land on the same pixels in Rook and Studio, at
the same sizes.

Code: `rook-lua/src/api/methods/prompts.rs` (showing, keys, the server's
events), `prompts/choose.rs` (which show), `prompts/hold.rs` (holding);
the engine calls it each frame after the camera settles, before GUI is
laid out.

## Which show

A client decides; the server never shows prompts.

- A prompt can show when it's `Enabled`, ProximityPromptService is
  `Enabled`, and it's in the Workspace under a BasePart (its position, not
  its pivot), an Attachment (its `WorldPosition`) or a Model with a
  `PrimaryPart`. Under a Model without one, a Folder, or outside the
  Workspace it never shows.
- It must be within `MaxActivationDistance` of the local character's
  `HumanoidRootPart`, and on screen: its point in front of the camera and
  inside the viewport, the GUI inset taken off the top.
- With `RequiresLineOfSight`, a ray from the camera to it must not hit a
  part other than the local character and the part it's under. Transparent
  and `CanCollide` false parts block it; `CanQuery` false parts don't.
  Other parts of the same model block it.
- Prompts compete by how near their screen point is to the viewport's
  center, the screen point counted from below the GUI inset (so the point
  they're nearest is the inset below the center). The nearer of two at the
  same point wins.
  - `AlwaysShow` prompts show, the best first, up to `MaxPromptsVisible`
    (0 shows none). The cap counts only these.
  - Of the `OneGlobally` prompts, the best one shows.
  - Of the `OnePerButton` prompts, the best one per `KeyboardKeyCode`.
- An indicator (`IndicatorShown`) shows for a prompt within
  `MaxIndicatorDistance` and on screen that isn't shown, a prompt beaten
  for its key included, up to `MaxIndicatorsVisible`. Line of sight
  doesn't matter for indicators.
- `PromptShown` passes the input type, `Keyboard`. TODO: gamepad and touch
  input types, with the platforms Rook gains them on (M19).
- ProximityPromptService's `PromptShown` and `PromptHidden` handlers run
  before the prompt's own. A prompt a script removes hides at once, and
  its own `PromptHidden` still fires. Inference: a prompt being hidden lets
  go of its hold before `PromptHidden` fires.

## Holding

- A key press triggers or begins holding every shown prompt on that key,
  in the order they were shown, before UserInputService's `InputBegan`.
  The press is `gameProcessedEvent`, and ContextActionService doesn't get
  it. Its release fires `TriggerEnded`, and `InputEnded` is processed too.
- A key already down when a prompt shows does nothing. A focused TextBox
  takes keys first.
- With a `HoldDuration`, the press fires `PromptButtonHoldBegan`. The first
  frame the hold has lasted that long fires `PromptButtonHoldEnded` then
  `Triggered`. Letting go earlier fires `PromptButtonHoldEnded` alone.
- Disabling a held prompt, leaving its reach or hiding it otherwise lets
  go (`TriggerEnded` or `PromptButtonHoldEnded`).
- `InputHoldBegin` holds as the key does, and Roblox's GUI calls it for
  clicks. A second `InputHoldBegin` is ignored, the first `InputHoldEnd`
  lets go, and it does nothing on a prompt that isn't shown.
- Each event fires on the prompt, then on ProximityPromptService (with the
  prompt first), and on the server too, with the player. Inference: the
  server takes the client's word and checks nothing.

## The GUI

The CoreScript hangs a BillboardGui per prompt in a ScreenGui in the
local PlayerGui. A world GUI anywhere in the local PlayerGui, inside a
ScreenGui included, draws as its own tree ([World GUI](world.md)). The
prompt's width comes from AutomaticSize: a list whose `Padding` tweens in
from a negative scale, beside a `RelativeYY` square, measured as Studio
measures them ([layout](layout.md#automaticsize)). The CoreScript sizes
the BillboardGui from it two frames after showing.
