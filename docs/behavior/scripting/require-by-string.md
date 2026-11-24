# Require by string

Checked against Studio 0.748 on 2026-11-22.

`require` given a string finds a ModuleScript by a path from the calling
script, by Luau's require-by-string rules applied to instances (Rook's
`rook_lua::api::scripts`). Measured with ModuleScripts requiring each other:

- `./` starts at the script's parent (its siblings), `../` at its
  grandparent, and `@self` at the script itself.
- After the prefix, each component names a child (the first by that name),
  `..` goes one parent up, and empty and `.` components are skipped
  (`.//B` and `././B` are `./B`). `./A/../B` and `@self/../B` both reach a sibling `B`.
- A path that reaches something other than a ModuleScript raises
  `Unable to require module from given path '<path>'` (measured with
  `./`, which reaches the script's parent Folder).
- Every other failure is `error requiring "<path>": <reason>`:

| Path | Reason |
|---|---|
| no prefix (`B`, the empty string) | `require path must start with a valid prefix: ./, ../, or @` |
| an alias other than `@self` (`@foo/B`) | `@foo is not a valid alias` |
| a child that isn't there (`./Nope`, `./B/Deep`) | `could not resolve child component "Nope"` |
| `../` from a script whose parent has no parent | `could not get parent of requiring context` |
| a `..` component above the root (`../../X`) | `could not get parent of component ".."` |

- Inference: `./` from a script with no parent fails like `../` does.
- `@self` alone requires the script itself, so a ModuleScript doing it gets
  `Requested module was required recursively`, as any module requiring
  itself does.
