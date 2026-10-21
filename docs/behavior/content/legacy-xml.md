# Legacy XML

Checked against Studio 0.741 on 2026-10-05.

Roblox XML from roughly 2006 to 2010 holds constructs that `rbx_xml`
rejects. `rook-formats` rewrites them into modern equivalents, the way
Studio upgrades old files when it opens them, and only after a normal load
has failed, so files that already load are never touched
(`rook_formats::legacy`). How often each appears in the archive is in
[research](../../design/research.md).

| Legacy construct | Upgrade |
|---|---|
| `<External>…</External>` | Dropped. They declare referents of objects that weren't saved; nothing in the file defines them. |
| `<Content><binary>…</binary></Content>` | The property loads **empty**, as in modern Studio (Confirmed). The decoded data is kept in the report as an `EmbeddedAsset`. |
| `<Content><hash>H</hash></Content>` | The property loads **empty** (Inference: same era and treatment as embedded data). The value is kept in the report as a `HashReference`. |
| `&#0;` and other characters XML 1.0 forbids | Mapped to private-use characters for parsing, then restored exactly afterwards. |
| `1.#QNAN`, `1.#IND`, `1.#INF` (MSVC float text) | NaN or ±infinity, inside numeric elements only. |
| `null` where a number belongs | `0`, inside numeric elements only. Unverified against Roblox. |

Embedded assets and hash references are dropped from the loaded tree
because modern Roblox drops them. The report keeps them, tagged with the
file referent and property, so an opt-in Rook extension could restore
them later without affecting compatibility mode.
