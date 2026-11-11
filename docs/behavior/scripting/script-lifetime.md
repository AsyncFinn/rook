# Script lifetime

Checked against Studio 0.747 on 2026-11-11.

A Script or LocalScript runs while it's enabled and somewhere it runs
(`rook_lua::api::scripts::runs_here`).

- It starts at the end of the frame it gets there enabled.
- It stops the moment it's disabled, destroyed, orphaned (`Parent = nil`)
  or moved somewhere it doesn't run, such as ServerStorage. Its threads,
  delayed calls and connections never run again, and neither do handlers
  already queued for it by deferred signals. Code running right now
  carries on until it yields.
- Moving between two places it runs, such as ServerScriptService to
  Workspace, keeps it running as it was.
- Leaving and coming back in the same frame stops it, and it starts afresh
  at the end of the frame, as when it's disabled and enabled again.

A script that destroys an ancestor of another, such as a loading screen
removing its spinner, needs this: the spinner's `RenderStepped` handler
would otherwise go on indexing a parent that's gone.
