# Roadmap

This is the one place Rook's plan lives: the order below, and one page
per milestone with its steps. Other docs name milestones and link here.
What each step found in Studio and the archive is in
[`docs/behavior/`](../behavior/), linked from the step. Ideas without a
place in the order yet, and what's undecided, are in
[Unscheduled](unscheduled.md); what Rook will never do is in
[policies](../design/policies.md#scope).

## How the order is decided

1. **Visible progress on archived places first.** Rook's first job is to
   open an archived place and show it as Roblox would.
2. **Then by what the archive uses.** Counts come from the corpus scan
   (`reports/corpus/`): 6,245 place and model files that load. Text
   written before 2026-10-22 counts the first scan's 3,056.
3. **Dependencies.** A milestone that needs another's work comes after it
   (characters need physics and the head mesh).
4. **Modern and legacy together.** Rook targets everything modern Roblox
   supports: today's features and the legacy ones it still runs. Each
   milestone covers both sides of its area (the 2022 materials and the
   legacy set, SurfaceAppearance and MaterialVariant, R15 and R6, the
   audio API and Sound, TextChatService and the old Chat), so modern
   features are never left for later. The archive is mostly old places
   (about 62 files were saved by today's Studio), so modern features are
   checked against Studio and the creator docs rather than ranked by
   archive counts.

Each milestone is a vertical slice with a measurable exit criterion, and
each release says which milestone it finishes (see [CHANGELOG.md](../../CHANGELOG.md)).

## Now, next, later

- **Now:** M8 GUI.
- **Next:** M9 Characters and, right after, M10 Networking.

The world comes before the people in it: sound, terrain, lights and GUI
are each in hundreds of archive files, and characters need terrain to walk
on and GUI for their tools and health. Networking follows characters
directly, since a character is what makes playing together worth it.

## Milestones

A milestone is ⬜ until its first step lands, 🟨 while its steps land,
and ✅ once its exit criterion is met; a step is ✅ when done, linking
the page its findings are on. `scripts/check` holds the marks here to the
milestones' steps.

| | Milestone | Exit criterion |
|---|---|---|
| ✅ | **M0** File formats, datatypes, reflection, CLI | All test fixtures round-trip; 99.3% of the archive loads (v0.1.0) |
| ✅ | **M1** Instance tree, signals, scheduler, Luau, server Scripts, conformance | First conformance batch passes; baseline of archive scripts running (v0.1.0) |
| ✅ | **M2** Renderer v0, client/server split, LocalScripts, basic input | Archived places render recognisably; client scripts run alongside server scripts (v0.1.0) |
| ✅ | [**M3** Physics](m3-physics.md) | Physics conformance batch passes; unanchored places settle plausibly |
| ✅ | [**M4** Content: meshes, materials, textures, decals](m4-content.md) | Places render with their meshes, materials (2022 and legacy, MaterialVariants, SurfaceAppearance), decals and textures (v0.2.0) |
| ✅ | [**M5** Audio](m5-audio.md) | Archived places play their sounds, positioned in 3D; the audio conformance batch passes (v0.2.0) |
| ✅ | [**M6** Terrain](m6-terrain.md) | Archived places with terrain show it, and parts and queries collide with it (v0.2.0) |
| ✅ | [**M7** Lights and effects](m7-lights-and-effects.md) | Local lights light the scene; Fire, Smoke, Sparkles and ParticleEmitters draw as in Studio (v0.3.0) |
| 🟨 | [**M8** GUI](m8-gui.md) | GUI conformance batch passes; archived places' GUIs draw and take input |
| 🟨 | [**M9** Characters](m9-characters.md) | Spawn, walk, jump, sit and use tools in an archived place |
| ⬜ | [**M10** Networking](m10-networking.md) | Two players on two machines play an archived place together |
| ⬜ | [**M11** Avatars and animation](m11-avatars-and-animation.md) | Players pick an avatar (no Roblox account) and see each other's, dressed and animated |
| ⬜ | [**M12** Server hosting](m12-server-hosting.md) | Host a place for friends over the internet; its saved data survives a restart |
| ⬜ | [**M13** Lighting and post-processing](m13-lighting-and-post-processing.md) | Screenshots of reference places match Studio's lighting, sky and effects |
| ⬜ | [**M14** Scripting completeness](m14-scripting-completeness.md) | 90% of archive places with server scripts run without an error Rook causes |
| ⬜ | [**M15** Physics completeness](m15-physics-completeness.md) | Second physics conformance batch passes; MeshParts and unions collide with their real shapes |
| ⬜ | [**M16** Every class the archive uses](m16-every-class.md) | No class in 5 or more archive files is still inert |
| ⬜ | [**M17** Modern places](m17-modern-places.md) | A conformance set of places built in today's Studio plays as it does in Studio |
| ⬜ | [**M18** Offline content](m18-offline-content.md) | An archived place plays on a machine with no network, from a content pack |
| ⬜ | [**M19** Input and platforms](m19-input-and-platforms.md) | Play with a gamepad; Rook builds and runs on Windows, Linux and macOS |
| ⬜ | [**M20** Hosting safety](m20-hosting-safety.md) | A public server survives runaway scripts and a hostile client test suite |
| ⬜ | [**M21** Performance and scale](m21-performance-and-scale.md) | The archive's largest places hold 60 FPS on a mid-range PC |
| ⬜ | [**M22** Release](m22-release.md) | Download Rook, pick a place from a library and play, without a terminal |

What's partly done or accepted but ignored today is listed member by member
in [stubs](../guide/stubs.md).
