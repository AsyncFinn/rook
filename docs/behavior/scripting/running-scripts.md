# Running scripts

Checked against Studio 0.747 on 2026-11-11.

A Script or LocalScript runs while it's enabled and somewhere it runs
(`rook_lua::api::scripts::runs_here`).

- It starts at the end of the frame it gets there enabled.
- It stops the moment it, or an ancestor, is destroyed, parented to nil
  or moved somewhere scripts don't run, such as ServerStorage, and the
  moment it's disabled. Its threads, delayed calls and connections never
  run again, and neither do handlers already queued for it by deferred
  signals, or ones later in the same firing. Code running right now
  carries on until it yields.
- Moving between two places it runs, such as ServerScriptService to
  Workspace, keeps it running as it was.
- Put back where it runs, even in the same frame, it starts afresh at the
  end of the frame, like a script disabled and enabled again.

Loading screens rely on this: their spinners connect to `RenderStepped`
and index `script.Parent`, then the screen destroys itself, sometimes from
inside its own `RenderStepped` handler.

Measured with Scripts on the server, each counting a BindableEvent,
`Heartbeat` and a loop, and a LocalScript in a ScreenGui in `PlayerGui`
counting `RenderStepped`; every count stopped at the removal, and an event
fired just before it was never heard.
