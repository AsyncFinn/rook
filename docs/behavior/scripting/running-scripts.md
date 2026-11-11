# Running scripts

Checked against Studio 0.747 on 2026-11-11.

Where a script runs is decided in `crates/rook-engine/src/scripts.rs`
(`runs_here`). A running script stops once it no longer would run where
it is, and its threads, delayed calls and connections never run again:

- Destroying it, or an ancestor, stops it.
- Parenting it, or an ancestor, to nil stops it too, as does moving it
  somewhere scripts don't run, such as ServerStorage.
- Put back where it runs, it runs afresh, like a script disabled and
  enabled again.
- It stops as it leaves: in Studio a LocalScript's `RenderStepped`
  connection stopped with its ScreenGui's `Destroy`, without one more
  frame. Rook checks between each phase of the frame, so a script removed
  during one misses the frame's later events.

Loading screens rely on this: their spinners connect to `RenderStepped`
and index `script.Parent`, then the screen destroys itself.

Measured with Scripts on the server and a LocalScript in a ScreenGui in
`PlayerGui`, each counting `Heartbeat` or `RenderStepped` and a loop; every
count stopped at the removal.
