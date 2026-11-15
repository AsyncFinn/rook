# Member lookup

Checked against Studio 0.747 on 2026-11-11, in conformance case
`instance/lowercase_members`, on a playtest server and client.

Code: `get_member` and `capitalized_member` in
`rook-lua/src/api/instance/lookup.rs`.

## Lowercase first letters

`inst.key` looks for, in order:

1. a member named `key`, including the deprecated lowercase members the
   API lists (`brickColor`, `findFirstChild`, `keyDown`);
2. a child named `key`;
3. a member named `key` with its first letter capitalized, whatever its
   kind: property (`mouse.x`, `label.textSize`, `part.cFrame`), method
   (`part:isA`) or event (`run.renderStepped`, `part.touched`).

Only the first letter changes: `aNCHORED` and `viewsizeX` are errors, and
`part.foo` doesn't reach a child named `Foo`. A child named `name` beats
`part.name`; a child named `Anchored` doesn't beat `part.Anchored`.

Writes never fall back: `part.anchored = true` errors
`anchored is not a valid member of Part "Part"`, while
`part.brickColor = ...` works because the API lists `brickColor`.

Old game scripts and Roblox's own CoreScripts both rely on the read
fallback (`RunService.renderStepped`, `.name`,
`RbxAnalyticsService:setRBXEventStream`).
