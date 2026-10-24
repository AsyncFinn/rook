# Policies

The rules every part of Rook follows: what it aims to be compatible with,
where its knowledge of Roblox may come from, and what content it may ship.
The background behind them is in [research](research.md).

## Scope

**Decision:** Rook targets **modern Roblox**, tracking the live version rather than a pinned API snapshot (Roblox moves too fast for a pin to stay useful). Old archived content must also work, *to the extent that modern Roblox itself still runs it*.

- Modern Roblox's behavior is the single source of truth, including when it loads old content. There is no separate "2016 mode".
- Deprecated members are part of the modern API (the current API dump still lists `FormFactor`, `Instance:remove()`, `:connect()`, BodyMovers, `wait()`/`spawn()` and so on, tagged `Deprecated`). They are implemented as modern Roblox implements them, and their priority comes from how often archived content uses them.
- Behavior that modern Roblox broke or removed stays broken by default. Bringing any of it back is a separate, opt-in Rook extension.
- The test oracle is live Roblox Studio. Every behavior can be checked against it, which a 2016 target could not offer.

Out of scope, unless added later as an opt-in extension: behavior modern
Roblox no longer has (old content works as far as modern Roblox still runs
it), and wire compatibility with real Roblox clients (replication is Rook to
Rook). Why 2016 isn't the target is in
[research](research.md#compatibility-target).

## Clean implementation

Rook is an independent implementation.

- Allowed sources: public documentation (creator-docs, API dumps, DevForum, Roblox blog posts), black-box observation of Roblox Studio and clients, open-source projects under compatible licenses, and our own experiments.
- Not allowed: leaked Roblox source or documents derived from it, decompiled or disassembled Roblox binaries, and projects built from either (e.g. libreblox-roblox).
- Each subsystem's design note records where its behavior came from (see [architecture](architecture.md#behavior-provenance)).
- **GUI is citation-only.** Every GUI behavior entry must cite creator-docs or a committed Studio conformance recording. No GUI behavior is accepted on recall or "known behavior" alone.
- Community knowledge (forums, word of mouth) is a lead, not a source. Before relying on it, verify it in Studio or find a public citation.
- **Files shipped in official Roblox builds** (shader packs such as `shaders_glsl3.pack`, the plain-text shaders in some 2016 player builds, terrain and material tables) carry no license, but they may be read for facts: formulas, constants, which inputs feed which terms. Rook uses those facts exactly, aiming for the same results as Studio, in its own functions shaped for its own renderer. Their code is never pasted or transliterated statement by statement into the repository, and the files themselves are never committed. Docs may name the file a fact came from. Behavior read this way is an Inference until it matches Studio (golden images or conformance recordings), then Confirmed. Content from a user's Roblox install may still be loaded at runtime ([Content and assets](#content-and-assets)).

## Content and assets

- Rook ships **its own default content** (skybox, part textures, cursors, open-licensed fonts) so it runs with no Roblox files at all.
- Optional: read the user's installed Roblox content folder, or download Roblox's `content-*.zip` packages the way Bloxstrap does. Content packages only, never `RobloxApp.zip` or `shaders.zip`. Downloads are hash-verified and cached, and the version is recorded.
- Public `rbxassetid://` assets are fetched anonymously, cached and rate-limited.
- Restricted assets: an Open Cloud API key is preferred (scoped, revocable). Whether it unlocks place files is **Unverified**. An opt-in `.ROBLOSECURITY` cookie is possible as a fallback: stored in the OS credential store, never logged, and only ever sent to roblox.com.
- Nothing Roblox-owned is committed to the Rook repository. Tests that need real files download them at test time or read a path from an environment variable.
- Files from Roblox's client (meshes, textures, shaders) are used two ways:
  as a reference that someone measures, with only the facts written down and
  Rook's own version built from them; or loaded at runtime from the user's own
  Roblox install or downloaded content packages.
