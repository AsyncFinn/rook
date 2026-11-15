# Waiting

Checked against Studio 0.747 on 2026-11-11.

`wait`, `delay`, `task.wait` and `task.delay` keep their duration as a
32-bit float (`rook_lua::api::globals::scheduling`).

- A duration that doesn't fit one waits as 0, so it lasts one step:
  `math.huge`, `-math.huge`, NaN and anything past about 3.4e38 (3.4e38
  waits, 3.5e38 doesn't).
- A long one that fits really waits: `wait(1e20)` never comes back in a
  session.
- Games park loops on it: `repeat wait(math.huge) until on` checks `on`
  every step.
