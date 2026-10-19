# Conformance

Small Luau cases that record what Roblox actually does, so Rook can be
checked against it.

- `cases/<group>/<name>.luau`: a case. It calls `log(...)` to record a line
  (values are passed through `tostring` and joined with spaces) and may
  yield. Errors escaping the case are logged as `ERROR <message>`. Cases
  must not touch the open place: create instances unparented and destroy
  them at the end.
- `expected/<group>/<name>.txt`: the output recorded in Roblox Studio. The
  first lines give the Studio version and whether the place used Immediate
  or Deferred signals; Rook runs the case in the same mode.
- A case with a `--! client mouse x,y x,y ...` line runs on the playtest
  client (a client DataModel in Rook), with the mouse moved through those
  screen points, 0.2 s apart, while it runs. Studio eases each move over a
  few events and Rook sends one, so client cases log what holds either way
  rather than positions.

## Recording in Studio

Recording uses Roblox Studio's built-in MCP server, an official Studio
feature, so Studio is only observed through its public scripting API.

1. Open a scratch place in Studio (not a real game).
2. In Assistant, open the ⋯ menu, choose **Manage MCP Servers** and enable
   Studio as an MCP server.
3. Run `rook-tools conformance record`. It starts a playtest if Studio is in edit
   mode, runs every case on the playtest server, saves the results and
   stops the playtest.

`--filter <text>` limits either command to cases whose `group/name`
contains the text.

## Checking Rook

`rook-tools conformance run` runs every case that has an expectation in Rook and
prints a line-by-line diff for each mismatch.

## Coverage notes

Expectations so far were recorded in a place whose `SignalBehavior` is
Deferred (new template places save that). Archived places use `Default`,
which Roblox documents as Immediate today; recording the same cases in such
a place is still to do.
