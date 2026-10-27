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
- A case with a `--! client input ...` line runs on the playtest client (a
  client DataModel in Rook), with the mouse and keyboard steps it lists
  played 0.2 s apart while it runs: `x,y` moves the mouse there (GUI
  coordinates, below the top bar), `down`, `up`, `down2` and `up2` press
  and release button 1 or 2, `wheelup` and `wheeldown` turn the wheel a
  notch, `key:Return` presses a key, `text:abc` types text and `wait`
  does nothing for a step. A bare `--! client` line plays nothing.
- A server case with a `--! player` line needs the player a Studio playtest
  has: Rook joins one before it runs. Studio
  eases each move over a few events and Rook sends one, and Studio's MCP
  is slower than 0.2 s a step, so client cases log what holds either way
  rather than positions mid-move or timings.

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

Each expectation is recorded in one signal mode, named in its header. New
template places save `SignalBehavior` Deferred; archived places use
`Default`, which is Immediate in Studio 0.743. To record in Immediate, open
a copy of a template place with `SignalBehavior2` set to 0 in its file and
pick it with `ROOK_STUDIO=<its name>`.
