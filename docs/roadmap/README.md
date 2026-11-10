# Roadmap

This is the one place Rook's plan lives: the milestones below, with what
each needs, and one page per milestone with its steps. Other docs name
milestones and link here. What each step found in Studio and the archive
is in [`docs/behavior/`](../behavior/), linked from the step. Ideas
without a milestone yet, and what's undecided, are in
[Unscheduled](unscheduled.md); what Rook will never do is in
[policies](../design/policies.md#scope).

## Picking work

The roadmap isn't a queue. Milestone numbers are names, not an order:
any milestone or step can be worked on at any time, alongside others,
once what it **Needs** is done. Only real dependencies go in Needs (a
step that can't be built or checked without another's work, as
characters need physics to stand on), never "this matters more". Steps
inside a milestone are independent too unless Needs names them; a
milestone's Match Studio step comes last, since it closes what the
others built.

When choosing between free steps, these help:

1. **Visible progress on archived places.** Rook's first job is to open
   an archived place and show it as Roblox would.
2. **What the archive uses.** Counts come from the corpus scan
   (`reports/corpus/`): 6,245 place and model files that load. Text
   written before 2026-10-22 counts the first scan's 3,056.
3. **Modern and legacy together.** Rook targets everything modern Roblox
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

## Match Studio

Studio is the bar. A step is ✅ only when what it covers behaves and
looks as it does in Studio. Every difference found is either fixed or
taken over by a named step of another milestone that owns that area:
lighting in M13; collision shapes, the remaining constraints, buoyancy
and timesteps in M15; inert classes in M16. A format an upstream
crate reads for Rook is owned by that crate, named with its issue or
PR, and Rook takes it by upgrading rather than writing a parser of its
own. "Conformance", "later" and "Renderer" own nothing.

Every milestone that reproduces Roblox ends with a **Match Studio**
step, added as its last step when its first step starts. It owns what
the milestone's other steps leave: the [stubs](../guide/stubs.md) rows
taken over by it, and each Approximation or Inference on the
milestone's behavior pages that a Studio measurement could settle. The
milestone stays 🟨 until that step is done, and the step is done when
all four of these hold for everything the milestone built:

- **Built right**: [built right the first
  time](../../CONTRIBUTING.md#built-right-the-first-time). Each part has
  the design established engines use, and its frame and edit costs scale
  with what changed, benched on the archive's largest places. A part
  that rebuilds or rescans everything isn't done, however it looks.

- **Correct**: every rule a script can observe (values, events and
  their order, errors, timing in frames) matches Studio. Each rule has a
  conformance case recorded in Studio, so a regression fails a test.
- **Edge cases**: the odd inputs are measured too, not just the common
  path:
  - empty, zero, negative, huge and NaN values;
  - several instances at once (overlap, which draws on top, creation
    order);
  - properties changed at runtime, and reparenting or destroying
    mid-use;
  - the limits Studio clamps to, and what an error leaves behind.
  Where Studio's tools can't show one, the user looks live.
- **Indistinguishable**: put the same views of the same place side by
  side, in Studio and in Rook, stills and in motion. Someone who knows
  Roblox can't tell which is which. This is how Xash3D runs Half-Life:
  in play the two are the same game, though a still of each differs
  pixel by pixel. Pixel differences are fine where they come from
  rounding, filtering, antialiasing or font hinting. They aren't fine
  where they come from a different rule: a shifted color or
  brightness, a different shape or edge, a missing detail, a different
  timing or motion. Sound is held to the same bar, heard side by side.

The exit criteria below are met only in this sense, never
"recognisably" or "plausibly".

## Steps

Each step on a milestone page says:

- **Scope**: what's in, what's out, and where the out goes.
- **Evidence**: corpus counts, Studio questions, sources.
- **Design**: written before the step's first code. How established
  engines solve it, named concretely; how changes flow (which change
  list or dirty set drives each update); and what a frame and an edit
  may cost on the archive's largest places. Only a step someone is
  about to start needs this, since it goes stale.
- **Done when**: the check, at the Match Studio bar for what the step
  covers, including the cost from its Design measured against the build
  before it.

A step without a written Design doesn't start. Most shortcuts Rook has
had to rip out were fixed in the first version's design, not in its
code, so this is where they're caught.

## Milestones

A milestone is ⬜ until its first step lands, 🟨 while its steps land,
and ✅ once its exit criterion is met; a step is ✅ when done, linking
the page its findings are on. `scripts/check` holds the marks here to the
milestones' steps.

**Needs** is the one place dependencies are written, for a whole
milestone or for the steps it names; everything not named there is free
to start. M0 to M2 are done, so they aren't listed.

| | Milestone | Exit criterion | Needs |
|---|---|---|---|
| ✅ | **M0** File formats, datatypes, reflection, CLI | All test fixtures round-trip; 99.3% of the archive loads (v0.1.0) | Nothing |
| ✅ | **M1** Instance tree, signals, scheduler, Luau, server Scripts, conformance | First conformance batch passes; baseline of archive scripts running (v0.1.0) | Nothing |
| ✅ | **M2** Renderer v0, client/server split, LocalScripts, basic input | Archived places render recognisably; client scripts run alongside server scripts (v0.1.0) | Nothing |
| 🟨 | [**M3** Physics](m3-physics.md) | Physics conformance batch passes; unanchored places settle as in Studio | Nothing |
| 🟨 | [**M4** Content: meshes, materials, textures, decals](m4-content.md) | Places draw their meshes, materials (2022 and legacy, MaterialVariants, SurfaceAppearance), decals and textures as Studio does | Nothing |
| 🟨 | [**M5** Audio](m5-audio.md) | Archived places sound as in Studio, positioned in 3D; the audio conformance batch passes | Nothing |
| 🟨 | [**M6** Terrain](m6-terrain.md) | Archived places' terrain looks as in Studio, and parts and queries collide with it as in Studio | M3 steps 1 and 5, M4 step 5 |
| 🟨 | [**M7** Lights and effects](m7-lights-and-effects.md) | Local lights light the scene and Fire, Smoke, Sparkles and ParticleEmitters draw as in Studio | Nothing |
| 🟨 | [**M8** GUI](m8-gui.md) | GUI conformance batch passes; archived places' GUIs draw and take input as in Studio | Step 10's chat between machines: M10 step 4 |
| 🟨 | [**M9** Characters](m9-characters.md) | Spawn, walk, jump, sit and use tools in an archived place as in Studio | M3, M4 step 1, M6 step 3; steps 5 and 7 (health bar, backpack): M8 step 9 |
| ⬜ | [**M10** Networking](m10-networking.md) | Two players on two machines play an archived place together | Steps 2 and 3: M9 step 2 |
| ⬜ | [**M11** Avatars and animation](m11-avatars-and-animation.md) | Players pick an avatar (no Roblox account) and see each other's, dressed and animated | M9 step 1; steps 1 and 4 (avatars sent, tracks replicated): M10 step 1 |
| ⬜ | [**M12** Server hosting](m12-server-hosting.md) | Host a place for friends over the internet; its saved data survives a restart | M10 step 1 |
| ⬜ | [**M13** Lighting and post-processing](m13-lighting-and-post-processing.md) | Screenshots of reference places match Studio's lighting, sky and effects | M7 step 3, M4 step 5 |
| ⬜ | [**M14** Scripting completeness](m14-scripting-completeness.md) | 90% of archive places with server scripts run without an error Rook causes | Step 7 (client scripts): M8 and M9 |
| ⬜ | [**M15** Physics completeness](m15-physics-completeness.md) | Second physics conformance batch passes; MeshParts and unions collide with their real shapes | Step 3 (buoyancy): M6 step 3 |
| ⬜ | [**M16** Every class the archive uses](m16-every-class.md) | No class in 5 or more archive files is still inert | Nothing |
| ⬜ | [**M17** Modern places](m17-modern-places.md) | A conformance set of places built in today's Studio plays as it does in Studio | Nothing; what it turns up goes to the milestone owning that area |
| ⬜ | [**M18** Offline content](m18-offline-content.md) | An archived place plays on a machine with no network, from a content pack | Step 4 (own PlayerModule and CoreGui): M8 step 9, M9 step 4 |
| ⬜ | [**M19** Input and platforms](m19-input-and-platforms.md) | Play with a gamepad and on a touchscreen; Rook builds and runs on Windows, Linux and macOS | Step 3: steps 1 and 2 |
| ⬜ | [**M20** Hosting safety](m20-hosting-safety.md) | A public server survives runaway scripts and a hostile client test suite | Step 2 (hostile clients): M10 step 1 |
| ⬜ | [**M21** Performance and scale](m21-performance-and-scale.md) | The archive's largest places hold 60 FPS on a mid-range PC | Step 3 (streaming): M10 step 1 |
| ⬜ | [**M22** Release](m22-release.md) | Download Rook, pick a place from a library and play, without a terminal | Joining servers: M10, M12 step 2; packages for Linux and macOS: M19's builds |

What's partly done or accepted but ignored today is listed member by member
in [stubs](../guide/stubs.md).
