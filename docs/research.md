# Raven — Research Findings

Status: initial survey, 2026-10-04. This document records what exists (prior Raven experiments, competing projects, Rust libraries, Roblox data sources) so that architectural decisions are made from knowledge rather than guesswork. The companion document is [architecture.md](architecture.md).

Confidence labels used throughout:

- **Confirmed** — verified directly (documentation, API dump, code, or a live request).
- **Inference** — strongly suggested by evidence but not directly verified.
- **Unverified** — believed true, needs checking before anything depends on it.

---

## 1. Compatibility target

**Decision:** Raven targets **modern Roblox**, pinned to a specific API snapshot. Old archived content must also work, *to the extent that modern Roblox itself still runs it*.

- Modern Roblox's behavior is the single source of truth, including when it loads old content. There is no separate "2016 mode".
- Deprecated members are part of the modern API (the current API dump still lists `FormFactor`, `Instance:remove()`, `:connect()`, BodyMovers, `wait()`/`spawn()` and so on, tagged `Deprecated`). They are implemented as modern Roblox implements them, and their priority comes from how often archived content uses them.
- Behavior that modern Roblox broke or removed stays broken by default. Bringing any of it back is a separate, opt-in Raven extension.
- The test oracle is live Roblox Studio. Every behavior can be checked against it, which a 2016 target could not offer.

Why 2016 was dropped (all **Confirmed** from the RobloxAPI build archive):

- "2016" is not one target. The January 2016 dump (0.225) has 329 classes and the December dump (0.271) has 370. MeshPart, Attachment, the constraints, R15 (`HumanoidRigType`), Accessory and `AnchorPoint` all arrived during that year.
- 2016 client content packages are no longer downloadable: the setup CDN returns 403 for the December 2016 manifest.
- 2016 behavior cannot be observed anymore. Its only detailed record is leaked source, which Raven must not use (see §6).

The modern snapshot has roughly 682 classes (rbx-native's count from the current dump). The pinned version is the one bundled in `rbx_reflection_database` (currently `3.0.1+roblox-741`).

---

## 2. Prior Raven experiments

### Mortar

Rust; wgpu 0.16, winit 0.28, rbx_dom_weak 1.10 / rbx_xml 0.6, cdylib with a C ABI.

| Area | Assessment |
|---|---|
| File loading (`modules/rbx_dom_wrapper/rbx_file.rs`) | Flattens every instance into a `ParsedItem` (position, size, color...) and discards the hierarchy. A fallback scans the XML text with `find("<tag")`. **Do not reuse.** It is the opposite of the instance-first pipeline Raven needs. |
| CFrame parsing (`coordinate_frame.rs`) | String-scanning helpers. Superseded by rbx_types. |
| Renderer (`core/engine.rs`, ~100 KB) | Monolithic: sky cubemap, triplanar texturing, DXT1/DXT5 decoding, readback. Useful as a reminder of the features needed; **code not reused** (wgpu API is 14 major versions behind). |
| Runtime (`runtime/`) | C ABI, viewport readback, background render thread behind global statics. Physics and Luau are empty stubs. |
| Data (`BrickColors.yaml`, `PartTypes.yaml`) | Hand-entered palette and primitive geometry. rbx_types already has the full BrickColor table, and primitive meshes should be generated in code. |

Lessons: build the instance model first; never hand-parse formats a crate already handles; don't put a C ABI on top before the core exists; avoid global renderer state.

### BrickXML

- `BrickXML-old-Python`: PyQt + OpenGL viewer built on `pyrbxm`. Historical only.
- `BrickXML-Prototype-Rust`: roxmltree RBXLX parser with its own `Instance` list, an egui Studio-style UI, and Luau run as an **external `luau.exe` process** (`luau_pipe/luau_bridge.rs`). Lesson: embed the VM. A subprocess cannot share the DataModel.
- Its `content/` folder is a copy of a modern Roblox client content folder. Those are copyrighted Roblox assets: never committed to Raven, usable only as local test input.

### BrikNet

A RakNet-style UDP library (tokio): bitstream, 6 reliability modes, 4 priorities, 32 ordering channels, 5-step handshake, RTT, retransmission. ACK/NACK sending is unfinished ("Phase 4"). **A candidate for Raven's networking transport**, to be weighed against `quinn` (QUIC) and `renet` when networking begins. Its design follows RakNet's public documentation.

---

## 3. Competing and related projects

| Project | Lang / stack | Target | Strengths | Gaps / weaknesses |
|---|---|---|---|---|
| **rbx-native** (chteau) — created 2026-09-14, very active | Rust; wgpu, GPUI, mlua | Modern; **Studio replacement** (editor) | Binary + XML read/write with exact round-trip on 16k-instance places; reflection from the daily API dump; `.mesh` v1–v5; from-scratch CSG for legacy unions; strong renderer incl. WebGPU build; Open Cloud client; asset cache | Luau runtime is "synchronous... no scheduler, no events, no yielding" (command bar only). The roadmap lists **physics, Play/Test, client/server replication** as "explicitly impossible without Roblox's engine". No Humanoid, no terrain voxels, no animation playback. |
| **OpenRBLX** (TornadoCookie) | C; raylib, Luau, cJSON, lz4 | Mixed 2005-era + modern | Parses rbxm/rbxl/rbxmx/rbxlx including 2005 models; uses `rojo-rbx/rbx-test-files`; ~120 classes stubbed; Luau bytecode disassembler | Each script runs on its **own pthread** and `wait()` is `usleep`, which is structurally wrong (Roblox is single-threaded, cooperative). No physics, lighting, surfaces or replication. Manual memory management; small classes are mostly stubs. |
| **RNR's Not Roblox** (lrre-foss) — archived 2025-01-12 | C++; OGRE, Bullet, Qt, pugixml | Roblox ≤ 0.3.744 (Apr 2008) | Actual physics (8k-part demos), networking (`ArkNet`, NetworkServer/Client/Replicator), Humanoid, studio, player and server clients; stated clean-room policy | Dead project; 2008 target; its README says it references "decompilations of legacy client binaries". GPL-3 client. |
| **Luau_engine** (Alienduck) | Rust; Bevy, Rapier | Roblox-*inspired* API | Bevy ECS + Rapier integration pattern | Not a compatibility engine; tiny. |
| **OpenBLOX** (Tabletop0263) | C; SDL 1.2 | 2005–2011 XML maps; DS/Wii ports | Retro-platform portability | XML only, very early. |
| **openblox** (openblox.org mirrors) | C++ | Roblox-like engine (2016–2020) | Historical prior art | Defunct; not inspected in depth. |
| **libreblox-roblox** | C++ | — | — | Appears to be a build of the **leaked** client source tree (RCCService, CoreScriptConverter, ShaderCompiler). **Must not be referenced.** |
| **Lune** (lune-org) — MPL-2.0 | Rust; mlua/Luau | Standalone Luau runtime | `lune-roblox` crate: Roblox datatypes as mlua userdata, a DataModel facade over rbx_dom_weak, place/model read/write | A scripting tool, not an engine. Very useful reference for datatype semantics. |

**Where Raven can be clearly ahead:** nobody has a modern-API **runtime**: scheduler, events, physics, Humanoid, Play mode, replication. rbx-native has the best file layer and editor but declares the runtime out of scope. RNR had a runtime but for 2008 and is dead. OpenRBLX's threading model can't be fixed incrementally. Raven's differentiators:

1. A correct single-threaded cooperative scheduler with Immediate and Deferred signal semantics.
2. Physics and Humanoid behavior, checked against Studio.
3. Every class in the reflection database exists and round-trips, even before it has behavior.
4. Conformance tests against recorded Studio values, plus a corpus scanner that measures how much of the archive actually runs.
5. A headless server mode and, later, Raven-to-Raven replication.

---

## 4. Rust ecosystem

Versions are as of 2026-10-04 (crates.io).

| Need | Crate | Version | Use in Raven |
|---|---|---|---|
| Binary formats | `rbx_binary` | 3.0.1 | Load/save `.rbxl`/`.rbxm`. Use as-is. |
| XML formats | `rbx_xml` | 3.0.1 | Load/save `.rbxlx`/`.rbxmx`. Use as-is. |
| Interchange DOM | `rbx_dom_weak` | 4.2.0 | I/O-boundary tree only. Converted to and from Raven's runtime DOM (see architecture). |
| Value types | `rbx_types` | 3.1.0 | `Variant` interchange and the BrickColor table; Raven wraps these in its own math types. |
| Reflection | `rbx_reflection` + `rbx_reflection_database` | 7.0.0 / 3.0.1+roblox-741 | Classes, properties, enums, defaults. Pin the version. |
| Luau VM | `mlua` | 0.12.2 | Features `luau` (+ `luau-jit` later), `vendored`. Provides interrupts, threads (coroutines) and userdata. |
| Lua parsing | `full_moon` | 3.0.0 | Corpus scanner: parse scripts to find which APIs they use. |
| Mesh files | `rbx_mesh` | 0.8.0 | Evaluate against rbx-native's v1–v5 coverage; write our own if it falls short. |
| Compression | `lz4_flex`, `zstd` | 0.14 / 0.14 | Pulled in by rbx_binary; also for caches. |
| Math | `glam` | 0.34 | Backing for Vector3/CFrame; f32 on the engine side, f64 where Luau semantics need it. |
| GPU | `wgpu` | 30.0.1 | Renderer. |
| Windowing | `winit` | 0.30.13 | Player window and input. |
| Text | `cosmic-text` (+ `glyphon`) | 0.19 / 0.12 | GUI text shaping, layout and rasterisation. |
| Physics | `rapier3d` | 0.36.0 | Rigid bodies, colliders, joints and queries underneath Raven's own Roblox-semantics layer. |
| Audio | `kira` (preferred) or `rodio` | 0.12.5 / 0.22.2 | Kira's tweening, mixer tracks and spatial audio map well onto Sound and SoundGroup. |
| Datatype reference | `lune-roblox` | 0.3.5 (MPL-2.0) | Reference for datatype edge cases. Taking files from it means those files stay MPL. |

Format edge cases to be aware of (from rbx-native's findings, **Inference** until we test them): properties saved under several names (normalised on load), shared strings (`SSTR`), per-class defaults missing from the official dump (rbx-dom's database supplies them), legacy unions referencing CSG asset data.

---

## 5. Roblox data sources

| Source | What it gives | Notes |
|---|---|---|
| `rbx_reflection_database` | Modern class/property/enum database with defaults | Primary source. MIT. |
| Roblox setup CDN (`setup.rbxcdn.com`) | `{version}-API-Dump.json`, `Full-API-Dump.json`, per-version `rbxPkgManifest.txt` | **Confirmed** current manifests are public and list `content-*.zip` packages with MD5 sizes and hashes. Old versions disappear (2016 → 403). |
| `RobloxAPI/build-archive` | API dumps + ReflectionMetadata from 2011 onward (65 builds in 2016) | Deprecated by its maintainer but still readable. Useful for "when did X appear" questions. |
| `Roblox/creator-docs` | Official engine documentation (CC-BY-4.0) | Main behavioral reference. Citing and quoting with attribution is allowed. |
| Asset delivery (`assetdelivery.roblox.com`) | Asset bytes | **Confirmed**: a classic public mesh (id 1028713) downloads anonymously (302 to CDN). A place (id 1818) is refused anonymously ("User is not authorized"). Public meshes, images and sounds don't need a login; places and restricted assets do. |
| `rojo-rbx/rbx-test-files` | Format test fixtures | Already used by OpenRBLX. |
| Live Roblox Studio | Ground truth | Recorded via conformance scripts (see architecture §9). |

Frame ordering from the creator-docs task scheduler diagram (**Inference**: label order read from the SVG, needs confirming in Studio): input processing → PreRender → render (async) → replication receive → script resumption (legacy `wait`) → PreAnimation → Humanoid step → PreSimulation → simulation step(s) → PostSimulation → Heartbeat / resume delayed threads → replication send.

Signal semantics (**Confirmed**, creator-docs `scripting/events/deferred.md`): `SignalBehavior.Default` currently equals **Immediate** (template places are set to Deferred). The re-entrancy depth limit is 10. Deferred handlers run at the documented resumption points. `Disconnect()` drops pending invocations, but `Destroy()` still runs them. Old places use Default, so Immediate must be exact, and Deferred must be supported too.

Physics step rate: believed to be 240 Hz fixed, with an adaptive option (`PhysicsSteppingMethod`). **Unverified.**

---

## 5a. Roblox's engine internals: what is publicly known

Only public, citable sources. Primary source: Arseny Kapoulkine ("zeux", Roblox Technical Director 2012–2023), [*Eight years at Roblox*](https://zeux.io/2020/08/02/eight-years-at-roblox/) (2020), cited below as **[zeux]**.

### Third-party components

| Component | What Roblox used it for | Source | Status | Implication for Raven |
|---|---|---|---|---|
| **OGRE** 1.6→1.8 | Rendering, 2006-era to 2014; removed in 2014 for an in-house D3D9/GL renderer | [zeux] | Confirmed | History only; modern behavior comes from the in-house renderer. |
| **Bullet** (collision pipeline) | CSG and the collision pipeline; cylinder collision since Aug 2015; custom KD-tree for terrain triangle meshes | [zeux] | Confirmed | Rapier's collision library (`parry`) is also GJK/EPA-based, so primitive contact behavior should be close. |
| **PGS solver** (in-house) | Optional from mid-2015, plus a hybrid LDL-PGS in 2018; the legacy spring solver was **removed March 4, 2019** | Roblox newsroom (Aug 2020); DevForum announcements | Confirmed | Modern means PGS only. Old places no longer get spring-solver behavior, so Raven never implements it. Rapier's impulse-based solver is the same family. |
| **RakNet** (modified) | Transport under the Replicator | Developer Console memory categories `network/raknet` and `network/replicator`, discussed widely on DevForum | Confirmed (observable) | BrikNet's RakNet-style design fits. Raven doesn't need wire compatibility. |
| **FMOD** | Audio engine | Studio About dialog (0.741.19, checked 2026-10-04): "Uses 'FMOD Studio' from 'Firelight Technologies'"; also FMOD error codes on DevForum | Confirmed (observed directly) | `Sound` rolloff modes and attenuation likely follow FMOD's publicly documented 3D rolloff models. Implement those curves in kira. **Inference** until checked in Studio. |
| **HarfBuzz** | Unicode text shaping, plus dynamic TTF rasterisation (both after 2013) | [zeux] | Confirmed | cosmic-text shapes with a HarfBuzz port, so glyph advances and kerning should match closely. |
| **pugixml** | XML place/model parsing since 2019 | [zeux] | Confirmed | XML whitespace and entity behavior follow pugixml. Note it if rbx_xml ever disagrees. |
| **LZ4** | Per-chunk compression in the binary format (designed 2013) | [zeux] | Confirmed | Already handled by rbx_binary. |
| **Recast + Detour** | PathfindingService navmesh (≈2018–2019, replacing voxel pathfinding) | [zeux] | Confirmed | Use a Recast/Detour port or binding for `PathfindingService`. |
| **microprofile** | In-client profiler (MicroProfiler) since 2015 | [zeux] | Confirmed | A Raven-side equivalent (e.g. `puffin`/Tracy) is a natural extension. |
| hlsl2glsl → hlslparser → SPIR-V | Shader translation pipeline over time | [zeux] | Confirmed | Context only. Raven writes its own WGSL. |
| **Luau** | Lua VM rewrite (2019), now open source (MIT) | Public repo | Confirmed | Raven uses the real Luau via mlua, so language behavior matches exactly. |
| Lua 5.1 | Scripting base before Luau | [zeux] | Confirmed | Explains why old scripts are 5.1-style; Luau runs them. |

Not established from public sources: anything beyond the above. Studio's About dialog (0.741.19) names only FMOD. Its "Third Party Notices" heading is plain text, not a link, so it gives no full list. The table above is the extent of what's publicly known.

### Engine history that informs behavior

- **Lighting:** CPU voxel lighting with SSE2 (Jan 2013), skylight AO (Feb 2013), point-light shadows (2013), two-channel character shadow maps (Sep 2015), Future Is Bright (2018–2020): HDR voxel lighting, a tone mapper, EVSM shadows with tile-based cascade updates, forward+ lights. This tells us what `Lighting.Technology` modes actually are, so we can approximate them. [zeux]
- **Terrain:** sparse voxel storage (2013), smooth terrain as a voxel distance field with geometric LOD (2014), cascaded LOD and a new in-memory format (2016). [zeux]
- **Binary format:** chunk-based, per-chunk LZ4, binary filters before compression. [zeux]
- **Remote events:** fire-and-forget events and coroutine-based remote functions (Sep 2013). [zeux]
- **Accurate Play Solo** (2018): server and client in one process with full replication. This **confirms** Raven's planned Play-mode design (architecture §8). [zeux]
- **Physics threading** (2019–2020): simulation split into jobs per connected island; multi-core narrowphase. [zeux; Roblox newsroom]
- **PGS stabilisation:** warm starting, post-stabilisation, compliance regularisation, LDL preconditioning. [Roblox newsroom, Aug 2020]

### Roblox-licensed code Raven can use

| Repo | License | Contents | Use |
|---|---|---|---|
| [Roblox/Core-Scripts](https://github.com/Roblox/Core-Scripts) (archived; last commit Aug 2018) | **Apache-2.0** | CameraScript, ControlScript (incl. ClickToMove, DynamicThumbstick), character Sound scripts, CoreGui scripts (Topbar, HealthScript, chat, prompts) | Can ship as Raven's default player and character scripts, with Apache-2.0 attribution. Era-appropriate for archived content. **No Animate script** is included, so Raven writes its own. |
| Roblox/Studio-Tools (archived) | Apache-2.0 | Old built-in Studio Lua plugins | Reference for future Studio tooling. |
| Roblox/Wiki-Lua-Libraries | **No license** | `LoadLibrary` libraries (RbxUtility, ...) | **Not usable.** |
| Roblox/luau and the Luau library mirrors | MIT | Language and utility libraries | Use freely. |

### Not to be used

Decompiling or disassembling Roblox binaries or `shaders.zip`, even though some projects do. Third-party knowledge stays limited to the public sources above.

---

## 6. Clean-implementation policy

Raven is an independent implementation.

- Allowed sources: public documentation (creator-docs, API dumps, DevForum, Roblox blog posts), black-box observation of Roblox Studio and clients, open-source projects under compatible licenses, and our own experiments.
- Not allowed: leaked Roblox source or documents derived from it, decompiled or disassembled Roblox binaries, and projects built from either (e.g. libreblox-roblox).
- Each subsystem's design note records where its behavior came from (see architecture §10).
- **GUI is citation-only.** Every GUI behavior entry must cite creator-docs or a committed Studio conformance recording. No GUI behavior is accepted on recall or "known behavior" alone.
- Community knowledge (revival communities, forums, word of mouth) is a lead, not a source. Before relying on it, verify it in Studio or find a public citation.

---

## 7. Content and asset policy

- Raven ships **its own default content** (skybox, part textures, cursors, open-licensed fonts) so it runs with no Roblox files at all.
- Optional: read the user's installed Roblox content folder, or download Roblox's `content-*.zip` packages the way Bloxstrap does. Content packages only, never `RobloxApp.zip` or `shaders.zip`. Downloads are hash-verified and cached, and the version is recorded.
- Public `rbxassetid://` assets are fetched anonymously, cached and rate-limited.
- Restricted assets: an Open Cloud API key is preferred (scoped, revocable). Whether it unlocks place files is **Unverified**. An opt-in `.ROBLOSECURITY` cookie is possible as a fallback: stored in the OS credential store, never logged, and only ever sent to roblox.com.
- Nothing Roblox-owned is committed to the Raven repository. Tests that need real files download them at test time or read a path from an environment variable.

---

## 8. Open questions

1. **License** for Raven (MIT OR Apache-2.0 would match the Rust ecosystem and the archives).
2. **Corpus**: where the archived places live, and how many there are. This drives the corpus scanner and priorities.
3. **Default player scripts**: partly answered. Roblox/Core-Scripts (Apache-2.0) provides CameraScript, ControlScript, character sounds and CoreGui from up to 2018, and Raven can ship those. Still open: the Animate script (not included; write our own) and the modern PlayerModule (not open-licensed; write our own, or load from the user's install). Decide before M4.
4. **Physics fidelity bar**: how closely Rapier-backed behavior must match Roblox's solver (stacking, friction, joint stiffness), and how we measure it.
5. **Networking**: Raven-to-Raven replication only (assumed). Wire compatibility with real Roblox clients is not a goal.

---

## 9. Archive scan (2026-10-05)

First full `raven corpus-scan` of the maintainer's archive of places and models. Only aggregate figures are recorded here.

- **4,605 files, 3,077 unique** (12.7 GB). **3,056 load (99.3%).** Before the legacy XML upgrade the figure was 55.6%.
- **1,346 files needed the legacy XML upgrade**: 27,096 `<External>` entries dropped, 1,456 embedded assets and 5,906 `<hash>` references (both load empty, as modern Studio does; the data is kept in the upgrade report), 19.1 million control characters restored (almost all legacy `Terrain.ClusterGrid`/`ClusterGridV2` data), 6 MSVC float values, 3 `null` numbers.
- **Embedded assets in Studio (Confirmed, 2026-10-05):** a December 2008 place opens in modern Studio, and the SpecialMesh whose texture was embedded in the file has an empty `TextureId`/`TextureContent`. `<hash>` references are assumed to behave the same (Inference).
- The 21 remaining failures: 3 non-Roblox or empty files, 2 binary files with bad headers, 2 binary files where rbx_binary rejects a `Color3uint8` stored as `Part.Color3uint8Internal` (an upstream issue), 3 XML files that aren't valid UTF-8, and a handful of malformed XML files.
- **11.5 million instances and 287,000 scripts.** About 1.0% of scripts have syntax errors. Nearly all of the rest of the raw total comes from one place holding 44,000 broken copies of a 2008 "anti-lag" script, which real Lua rejects too.
- **Legacy API use is pervasive:** `:connect` (861 files), `:findFirstChild` (611), `:clone` (558), `:remove` (506), `:disconnect` (491), `game:service` (465); global `wait()` in 997 files.
- **Deprecated classes in wide use:** CylinderMesh, Hat, ManualWeld, HopperBin, Snap, and the BodyMovers (BodyPosition, BodyVelocity, BodyGyro, BodyForce).
- **Classes absent from the modern database** (kept as data): old services such as `ScriptInformationProvider`, `RenderHooksService` and `FWService`, plus `Timer`, `StockSound` and others.

