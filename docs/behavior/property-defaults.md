# Property defaults

Checked against Studio 0.744 on 2026-10-22.

What a property reads before anything sets it. Rook takes, in order: its
own short list of corrections (`DEFAULTS` in
`crates/rook-dom/src/properties/defaults.rs`), the class default from the
reflection database, the value measured in Studio
(`crates/rook-dom/src/properties/studio_defaults.rs`), and the zero value
of the property's type (false, 0, empty, the enum's first item).

## Why the database leaves some out

rbx-dom records defaults by creating each class in Studio and saving it,
so it only knows properties that save to files. Service settings and
runtime state that never save (`UserInputService.MouseIconEnabled`,
`Humanoid.Health`, `TextBox.CursorPosition`) would read the zero value.
That is by design upstream, so Rook measures them itself.

## The sweep

`rook-tools studio-defaults` lists every scriptable property of every
creatable class and service that the database has no default for
(`--list` prints them: 3,984 on 699 classes for the pinned database).
Through Studio's MCP server it creates each class with `Instance.new`, or
finds each service with `FindService`, reads every listed property with
`pcall` and destroys what it made. It reads once in Edit mode and twice in
a playtest's client, two seconds apart.

- **Services are only found.** `GetService("NetworkServer")` on a playtest
  client tears down its replicator and crashes Studio 0.744 (an unhandled
  "Internal Permission is required" error). Finding them also leaves the
  place unchanged.
- **Kept:** values that differ from Rook's zero value. Where Edit and the
  client differ, the client's wins, since that is what a running game
  sees; services only Edit has keep Edit's value.
- **Left out:** values that depend on the window, session, place or
  machine (`ViewportSize`, `Absolute*`, ids, locales, `RunState`, Camera,
  Workspace, Players, Stats and the Studio-only services); values that
  changed between the two client reads (Stats' frame times); and views
  that Studio computes from other properties, which need views rather
  than defaults (mass and extents, axes and orientations, unsaved
  BrickColor twins of Color3 properties, `ContentText`, Sound's `Pitch`
  and distances, and every legacy property the database migrates to a
  newer one, such as `Sky.SkyboxBk` to `SkyboxBackContent` and
  `UICorner.CornerRadius` to its four corner radii).

## Results

45 defaults, among them `Humanoid.Health` 100 and `FloorMaterial` Air,
`UserInputService`'s `MouseEnabled`, `KeyboardEnabled` and
`MouseIconEnabled` true, `InputSink` Activate on buttons and TextBoxes,
`TextBox.CursorPosition` 1 and `SelectionStart` -1, `WeldConstraint.Enabled` true and `Lighting.ShadowColor` (0.7, 0.7,
0.72).

Edit and the client differ only in `StarterGui.ProcessUserInput` (true in
Edit, false on the client); the client's false is the zero value, so the
table has no entry for it. `BubbleChatMessageProperties.FontFace` and
`GetTextBoundsParams.Font` are skipped: the sweep doesn't store Fonts yet.

The database is wrong, not silent, about `SoundService.RespectFilteringEnabled`
(it says false; creator-docs and Studio 0.742 say true), so that one stays
in Rook's own list.
