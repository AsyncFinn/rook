# Changelog

## Versioning

Rook's version says how far along the roadmap it is:

- Every change goes under **[Unreleased]** as it lands, without a version of its own.
- **Minor** (`0.N.0`): finishing milestone N releases what's collected. 0.3.0 finished M3 Physics, 0.4.0 M4 Content and 0.5.0 M5 Audio. Version 1.0 comes with M22, the release; after it a minor comes with each finished set of new features.
- **Patch** (`0.N.y`): fixes collected under [Unreleased] are released together, at most once a week. Single fixes, roadmap steps and pushes don't get a version of their own.
- Commits go to GitHub as they land, so [Unreleased] on GitHub shows what the next version will hold.
- Before 1.0, any release may break things.

Versions were renumbered twice: on 2026-10-15 (every step had been a minor version, 0.4.0 to 0.13.1) and on 2026-10-16 (every fix had been a patch of its own, up to 0.5.17), when commit messages were rewritten to match.

Every crate and app shares the workspace version. Each release is tagged `vX.Y.Z` in git. The pinned Roblox version (currently 0.741.19) is separate and listed under each release.

## [Unreleased]

## [0.6.0] — 2026-10-16 — M6 steps 3 and 4: terrain collision and the Terrain API; M6 Terrain done

Pinned Roblox version: 0.741.19. Finishes M6 Terrain: archived places with terrain show it, parts rest on it and queries find it, and scripts build and dig it as Studio does; the conformance case `terrain/terrain_api` passes.

- The Terrain API (M6 step 4): `FillBlock`, `FillBall`, `FillCylinder`, `FillWedge`, `FillRegion`, `ReplaceMaterial`, `ReadVoxels`, `WriteVoxels`, `Clear`, `CountCells`, `CopyRegion` and `PasteRegion` (TerrainRegion), `GetMaterialColor` and `SetMaterialColor`, and the legacy cell methods modern Roblox still runs on smooth terrain (`SetCell`, `SetCells`, `GetCell`, `SetWaterCell`, `GetWaterCell`, `WorldToCell` and its `PreferSolid`/`PreferEmpty` forms, `CellCenterToWorld`, `CellCornerToWorld`, `AutowedgeCell(s)`). Each fill sets voxels as Studio 0.742 does, byte for byte in the new conformance case `terrain/terrain_api`, errors included. Reads see edits at once; collision, drawing and replication follow at the end of the frame (Studio's rays see a fill 2 frames later). Only the chunks an edit changes are meshed and collided again, on servers and clients. `Region3int16` exists for scripts.
- `workspace.Terrain` is never nil: a place saved without a Terrain gets one, as in Roblox.
- Terrain collides (M6 step 3): unanchored parts land on and rest on terrain's smooth surface, as in Studio, and touch it (`Touched` fires with Terrain). Rays (`Raycast`, the `FindPartOnRay` family, the mouse) and `Blockcast`/`Spherecast` hit it and report Terrain with the material hit; they hit water too unless `IgnoreWater` or `ignoreWater` is set. `GetTouchingParts` lists Terrain, `IsRegion3Empty` counts its voxels, and the bounds queries leave it out, all as measured in Studio. A chunk's colliders are built when something first comes near it, so an 86k-part place's 12 million terrain triangles add nothing to its load time. Parts don't float in water yet.
- One place decides how many background threads Rook uses, from the machine's logical cores (`rook_engine::threads`), and hands content decoding and physics their counts. It sets aside the engine and server threads and hands out the rest. `ROOK_THREADS=n` pretends the machine has `n` cores.
- Content loading splits fetching from decoding. Each loader keeps 4 fetch threads for disk and network. Meshes, images and sounds decode on one process-wide pool: one thread on 2-3 cores, 8 at most (8 on a 12-core PC, up from 4 per loader). Both run at below-normal priority on Windows, so loading yields to the frame and audio. Physics' worker pool takes half the cores from the same budget, as before.
- Fix: doors that script-rotate about `Model:GetModelCFrame()` (QSML3's) swing about their hinge instead of their middle. `GetModelCFrame` is the bounding box's CFrame, as in Studio, not the primary part's; `GetBoundingBox`, `GetExtentsSize` and `GetModelCFrame` turn the box to the primary part's orientation instead of the world axes.
- Fix: `Instance.new` names instances as Studio does: a BindableEvent is called "Event", the value objects "Value", meshes "Mesh", WedgePart "Wedge", and so on, for the 79 classes Studio 0.742 doesn't name after the class (until now only the CSG classes were covered). QSML3's door handler waits for an "Event" its doors create without naming, so it no longer prints 23 infinite-yield warnings.
- Fix: clicks reach a ClickDetector through parts that are fully transparent and don't collide, as in Studio (`Mouse.Target` still reports those parts). QSML3's reactor key has invisible copies at its exact position, which took the click, so the key couldn't be turned.
- `rook-player` shows the pointing-hand cursor over anything with a ClickDetector, as Roblox does.
- `rook-player --bench` prints when content finished loading, counted from the start of loading the place, and the frame times until then.

## [0.5.4] — 2026-10-16 — Fixes: draw calls, Windows start-up, deprecated properties

Pinned Roblox version: 0.741.19.

### Fix: deprecated lowercase properties read stale values

- A deprecated lowercase property reads and writes its capitalized twin, as lowercase methods and events already did: `Mouse.hit` and `Mouse.target`, `Sound.isPlaying`, `Humanoid.maxHealth`, `Players.localPlayer` and `numPlayers`, `Player.userId`, `Camera.focus`, `JointInstance.part1`, `Instance.archivable` and the rest. Roblox's API lists them as separate properties, so Rook kept each at its default (`Mouse.hit` never followed the mouse). `className` and `game.workspace` go through the same rule, and their own special cases are gone.
- `game.lighting` gives Lighting; it was nil.
- Found by reading every deprecated member of Roblox's API (492 that scripts can reach) in Rook. Every member that errored is one Roblox also keeps from game scripts (plugin or Roblox-only security).

### Fix: `className` is nil

- `className`, the deprecated lowercase name for `ClassName`, gives the class name; it was nil. Old scripts that filter children with `if c.className == "Part"` found nothing: a heist place's vault door and drill turned in place without opening.

### Fix: the player crashes on OpenGL

- The player runs on devices that can't view a texture in a second format, such as OpenGL (what wgpu picks over Remote Desktop or on old drivers). There it stopped at once with "Downlevel flags VIEW_FORMATS are required". Each image now keeps a second copy stored as sRGB there, using twice the image memory. Elsewhere nothing changes. An 86k-part place renders the same on OpenGL as on the default backend.
- `WGPU_BACKEND` (`vulkan`, `dx12`, `gl`) picks the graphics backend for the player and for headless renders.

### Fix: the player won't start without the Visual C++ runtime

- Windows builds include the C and C++ runtime (`+crt-static`, in `.cargo/config.toml`), so `rook-player.exe` starts on machines without the Visual C++ Redistributable. It failed with "VCRUNTIME140_1.dll was not found". It now needs only DLLs that ship with Windows.

### Fix: drawing off-screen parts that share a batch with visible ones

- The renderer culls within batches, not just whole batches. Each batch keeps a box per 16 instance slots, grown when an instance changes and worked out afresh with the batch's own box. Each pass draws only the runs of consecutive boxes its view reaches (the camera's for the main pass and images, the sun's for the shadow pass), one draw per run, in the same order as before.
- On an 86k-part place's saved camera, triangles per frame fell from 9.6M to 6.2M (main pass) and 7.5M to 4.4M (shadow pass); GPU time from 3.73 to 2.63 ms (main) and 1.08 to 0.66 ms (shadow). A 61k-part place's saved camera: 11.7M to 8.1M triangles, GPU 4.70 to 3.15 ms. Draws rose (an 86k-part place 309 + 597 to 547 + 809, a 61k-part place 389 to 715) but draw calls didn't: they're all still in the same multi-draws. CPU recording barely moved (medians 2.98 → 3.91 ms on an 86k-part place, 2.49 → 2.64 ms on a 61k-part place, one round of 200 frames of `--bench` each, A/B against 0.5.12).
- Frames look the same: a 61k-part place's saved view is byte-identical to the build with the draw-call fix alone; an 86k-part place's differs in 108 pixels by at most 2 levels, within its run-to-run noise.

### Fix: too many draw calls on big places

- The renderer draws a whole pass in a few draw calls instead of one per mesh and image. Every mesh's vertices and indices sit in shared buffers, each set of batches keeps its instances in one buffer, and every image sits in one table that each instance indexes (binding arrays), so consecutive draws bind nothing new. Each pass's visible batches become indirect draw arguments, drawn with one multi-draw per pipeline in the same order as before. On an 86k-part place, 321 + 620 draw calls (shadow + main) fell to 1 + 6; from a camera seeing most of the map, 7,831 to 135. A 61k-part place's main pass went from 393 to 68 calls, and 3,143 to 805 from above (its sorted transparent parts still switch pipelines). The shadow pass reads positions only (12 bytes a vertex instead of 48).
- CPU time recording a frame (300 frames of `--bench`, A/B against 0.5.3, means of two rounds): an 86k-part place 18.2 → 10.4 ms from the saved camera and 30.3 → 13.4 ms from above; a 61k-part place 6.4 → 3.1 ms and 8.7 → 4.2 ms. Render with readback: an 86k-part place 29.9 → 21.4 and 50.4 → 27.2 ms; a 61k-part place 21.7 → 13.4 and 21.8 → 17.6 ms. In steady frames on an 86k-part place (Tracy), `Renderer::render` went from 7.7 to 3.3 ms. GPU time is unchanged.
- Frames look the same: from above both places are byte-identical to 0.5.3. From the saved cameras the only differences beyond run-to-run noise are 2 pixels on an 86k-part place one level apart (float rounding).
- Adapters without binding arrays, or without first instances in indirect draws, bind each image set and draw each batch directly; the frames are identical.
- `--bench` prints draw calls and instanced draws, triangles per pass, and medians of CPU recording and of each pass's GPU time (timestamp queries).
- Found: from inside the map, most triangles drawn belong to instances off screen that share a batch with some in view (an 86k-part place's saved camera: 13% of the opaque triangles drawn are in view; a 61k-part place's 22%). Culling within batches is next.

## [0.5.3] — 2026-10-15 — Fixes: speed on big places

Pinned Roblox version: 0.741.19.

### Fix: slow rendering on big places

- The renderer skips draws for what the camera can't see. Each batch of instanced parts (one per mesh and image set, so every mesh file and terrain chunk) keeps a box around its instances in the world. The main pass only draws batches whose box reaches into the camera's view, and transparent parts likewise. The shadow pass only draws batches the sun's view of the shadowed area reaches, so a part behind the camera still shadows what's in view. The GPU clipped those triangles away anyway, so no pixel changes: frames rendered with and without culling are byte-identical on an 86k-part place and a 61k-part place, from three views each. On an 86k-part place (86k parts, 7,680 mesh files), draws per frame fell from 6,762 + 8,332 (shadow + main) to 321 + 620, and render with readback from 84 to 36 ms a frame. On a 61k-part place (no shadows), main-pass draws fell from 2,966 to 393 and render from 18.7 to 15.3 ms (300 frames of `--bench` each, A/B against the build with the physics fixes; the fixes in between don't touch rendering).
- `--bench` prints the draw calls per frame.

### Fix: `game.workspace` is nil

- `game.workspace`, the deprecated lowercase name for `game.Workspace`, gives Workspace; it was nil. Old scripts that use it now run: a 61k-part place and an 86k-part place's `Univerisal_Opener_Lockdown` no longer fails on its first line.

### Fix: the player rebuilds the whole scene on its first frame

- The player no longer rebuilds the whole scene on the first frame after loading. A new DataModel says "redraw everything", and the scene the player had just built from it took that as a reason to rebuild all of it again: every part's look, the terrain's entries, and a fresh upload to the GPU. The scene now drops the changes it was built with. `--bench` also reuses the loaded scene instead of building its own, which meshed the terrain a second time (7,045 meshes, several seconds on an 86k-part place). On an 86k-part place, 300 frames of `--bench`, longest frame 5.1–6.1 → 3.6–4.3 s, slowest 1% 2.2–2.5 → 1.5–1.7 s (A/B against the build with the physics fixes, two rounds). Screenshots are unchanged.
- Found: what's left of the first frame is building the physics worlds. The client's first mouse raycast builds its world (1.3 s on an 86k-part place), and so does the server's first step (another 1.3 s). Nothing remeshes terrain after load any more: no script API writes voxels yet.

### Fix: a frame stalls when many sounds finish loading

- A sound's file is freed on the content worker that decoded it, not on the engine thread. Freeing each file took about 0.3 ms, so when an 86k-part place's 168 sound files finished loading together, one server step stalled for 55–80 ms. Time spent on the engine thread handing loaded sounds over (`sound_loaded`, Tracy, 300 frames of `--bench` on an 86k-part place): 144 → 22 ms in total, and at most 55 → 11 ms in one frame. Sounds play as before.

### Fix: slow server steps on big places, part 2

- Physics only runs Rook's friction and elasticity mixing on parts that need it: parts whose `FrictionWeight` or `ElasticityWeight` isn't 1, and anchored parts carrying what touches them (conveyors). Everywhere else both weights are 1, Roblox's weighted average is the plain average, and Rapier's own Average rule gives the same numbers. Rapier never reuses a contact between steps when a hook is on, so with the hook on every part it worked out every resting contact from scratch four times a frame. Server step median on 300 frames of `--bench`: a 61k-part place 22.0 → 16.2 ms, an 86k-part place 16.9 → 14.2 ms (A/B against 0.5.6, same output).

### Fix: slow server steps on big places

- Physics no longer runs continuous collision detection. Rapier turned it on for every fast-moving part, and each one was checked against the whole world's colliders four times a frame. On places with tens of thousands of parts that cost most of the server's frame. Roblox doesn't sweep fast parts either: they can pass through thin parts. Server step median on 300 frames of `--bench`: a 61k-part place 36.8 → 23.5 ms, an 86k-part place 26.0 → 17.8 ms; client steps unchanged.

## [0.5.2] — 2026-10-15 — M6 step 2: terrain materials, water, grass and waves

Pinned Roblox version: 0.741.19. M6 Terrain, step 2, measured again.

### M6 step 2: terrain's look checked against Studio

- Water plays all 25 of the install's normal frames, fading each into the next, on a phase of `WaterWaveSpeed` × time as Studio's does: at speed 0 the water freezes and it goes on from there, and the frames come round every 20 units of phase (2 seconds at the default speed), measured by freezing Studio's water at set phases. The ripples no longer scroll (Studio's don't), and are laid at Studio's size (fitted: same contrast and pattern from above). Water also reflects the sky's colour from overhead to the horizon, so its ripples show, and the sun's glint is a narrow highlight on ridges, as in Studio.
- Grass blades are single straight triangles as in Studio, 2.3 per square stud, up to 3.6 studs tall (fitted to how much of Studio's view they cover at each height: within 0.04), one flat colour 0.8 of the ground's brightness even on slopes, swaying as a wave crosses the grass, and turning into `Workspace.GlobalWind` by up to 50 degrees, as measured.
- Scripts changing the Terrain's water properties or `Workspace.GlobalWind` take effect at once (they were read only when the terrain was built).
- Found: Studio tints terrain textures differently from the tint mask Rook uses for non-default `MaterialColors` (red Grass turns magenta); not fixed yet. See `docs/behavior/terrain.md`.

### Fix: moved models balloon and vanish

- Fixed: models moved over and over with `SetPrimaryPartCFrame` (doors swung by `GetPrimaryPartCFrame() * CFrame.Angles(...)`, or driven by a tweened CFrameValue) grew huge and then vanished. Rounding error in a part's rotation tripled with every move, since the move inverts a rotation it assumes is orthonormal. Parts' CFrames are now orthonormalized whenever a script sets them, as Roblox does.
- Fixed: a sound on a part flung that far got NaN volumes, which could reach the ambient reverb and silence all audio until restart. Gains that aren't finite now mix as silence.

### M6 step 2: grass blades and water waves

- Grass grows blades with `Terrain.Decoration` on, as in Studio: dark olive spikes 1.5 to 3.3 studs tall, about one per square stud, leaning every way, sized against a ruler in Studio screenshots. They're built in 32-stud tiles within 200 studs of the camera, thin out with distance, and sway, bending with `Workspace.GlobalWind`. With `Decoration` off there are none.
- Water waves: the surface rises and falls by `WaterWaveSize` studs in long, slow humps, moving with `WaterWaveSpeed`, as Studio's does (its waves move the surface itself, measured at grazing angle). It's rippled by the water normal map from the user's Roblox install, reflects more of the sky at grazing angles, glints in the sun, and turns more opaque there.
- Costs about 1 ms a frame on the largest terrain place (median 4.5 → 5.5 ms offscreen).
- Not yet: Studio's exact blade shapes and sway, its 25-frame water animation, and terrain properties scripts change after load. See `docs/behavior/terrain.md`.

### M6 step 2: terrain materials and water

- Terrain is textured. Rook reads Studio's own terrain material table (from Studio's settings, kept in `rook-terrain`): each material's top, side and bottom texture packs, fetched from Roblox like part materials, projected onto the plane each point faces most of 18, in world studs. Grass has grass on top and earth on its sides, as in Studio.
- `Terrain.MaterialColors` tints the textures through their tint masks, as Studio does: CrackedLava's cracks take its colour over dark rock, Grass's sides stay earth. Places that saved no colours get the 2022 defaults.
- A MaterialVariant that MaterialService's override names for a terrain material replaces its textures on terrain.
- Terrain's shapes now come from the same table: Rock, Slate, CrackedLava, Basalt and Ice are rough (with Rook's own pseudo-random offsets, of the table's size), Cobblestone is faceted, and Rock, Basalt, Cobblestone and Pavement shade flat.
- Water sits half a voxel lower than solid terrain's surface would, as Studio's does, and reflects some of the sky.
- Not yet: grass blades, water waves, the texture detiling and height blending between materials; some colours differ from Studio's (Brick paler, Mud brighter there). See `docs/behavior/terrain.md` and `docs/behavior/stubs.md`.
- Fixed: step 1's Cobblestone was a guess (cubify plus a shift); it quantizes to 0.6 studs, which matches Studio exactly.

## [0.5.1] — 2026-10-15 — M6 step 1: terrain voxels and their surface

Pinned Roblox version: 0.741.19. M6 Terrain, step 1.

- Smooth terrain is drawn. The new `rook-terrain` crate decodes `Terrain.SmoothGrid` (checked against Studio's `ReadVoxels` on an archive place: every voxel and its occupancy match) and meshes it as Roblox does: surface nets, with each edge crossing `4 × occupancy` studs from its solid voxel, measured in Studio with `Workspace:Raycast` on single voxels, slabs and random blocks.
- Materials shape the surface as in Studio: Brick and WoodPlanks are 4-stud blocks, Concrete and Pavement snap to halves and quarters of a voxel, and those four are drawn faceted; Grass, Sand, Snow and the other plain materials are smooth. Each material is drawn in its `Terrain.MaterialColors` colour (textures are step 2), and water in `WaterColor` at `WaterTransparency`.
- Not yet: the pseudo-random roughness of Rock, Slate, CrackedLava, Basalt, Ice and Cobblestone (drawn smooth), Roblox's choice of diagonal on bent quads, and its water surface. Nothing collides with terrain yet (step 3).
- Legacy terrain (`ClusterGrid`, `ClusterGridV2`) shows nothing, as in today's Studio, which refuses it as malformed or ignores it (three archive places checked).
- Clients get the terrain: `SmoothGrid` replicates, as Roblox's own terrain streaming does.
- `rook terrain <file>` shows what a file's Terrain holds and how long it takes to mesh; `--scan <folder>` counts smooth and legacy terrain across an archive. Of the archive's 6,378 unique files, 10 hold smooth voxels and 104 legacy blocks. See `docs/behavior/terrain.md`.

## [0.5.0] — 2026-10-15 — M5 step 5: the audio API; M5 Audio done

Pinned Roblox version: 0.741.19. Finishes M5 step 5 and with it M5 Audio: archived places play their sounds, positioned in 3D, and the audio conformance cases pass.

### M5 step 5: the audio API; M5 Audio done

- The audio API plays on the same engine as Sound. AudioPlayers load through the same cache as Sounds and play as Studio's do: `Play` from `TimePosition`, `Stop` keeping it, `Looping` and `Looped`, `Ended` at the end or past it, `PlaybackSpeed`, `Volume`, `AutoLoad`, and `Play(atTime)`, `Stop(atTime)` and `Cancel` on clients (with Studio's error on a server).
- Wires connect by Studio's rules: both ends and the wire in the DataModel, pins that exist (`GetInputPins`, `GetOutputPins`), no self-wire and no loop. `Wire.Connected` and `GetConnectedWires` read as in Studio.
- AudioEmitters are heard by AudioListeners in the same `AudioInteractionGroup`, by Studio's default distance falloff (InverseTapered, 4 to 10,000 studs), its presets, and curves set with `SetDistanceAttenuation` and `SetAngleAttenuation`, panned as a Sound is. `GetAudibilityFor` matches Studio's. A client gets Studio's default listener under the camera, wired to `SoundService.AudioDeviceOutput`.
- Heard effects: AudioFader, AudioEqualizer (with `MidRange`), AudioDistortion, AudioPitchShifter and AudioReverb, which Studio plays exactly as the step 4 sound effects, plus new AudioChorus, AudioEcho and AudioCompressor (with `Sidechain`), fitted to Studio's levels. Effect properties keep to Studio's ranges.
- AudioAnalyzer measures `RmsLevel`, `PeakLevel` and `GetSpectrum` on clients, as Studio's (`RmsLevel` is `PlaybackLoudness` / 1000).
- Not yet: AudioFilter, AudioFlanger, AudioTremolo, AudioLimiter, AudioGate and AudioChannelSplitter pass sound through; `PlaybackRegion` and `LoopRegion`; Studio's 7.1 layouts and near-field spread. See `docs/behavior/audio.md` and `docs/behavior/stubs.md`.
- The `RobloxSecurity` capability is now called `RobloxEngine`, as Studio's error messages name it.
- Conformance case `content/audio_api`, recorded in Studio; all 11 content cases pass.

### AmbientReverb as loud as Studio's

- Fixed: `SoundService.AmbientReverb` was far too loud. Rook used FMOD's published wet level for each preset, but a recording of Studio's own output (Cave, Hallway, ConcertHall) put its reverb 14.5 to 17.0 dB below that. Every preset's wet level is now 16 dB lower, within 1.6 dB of Studio for all three.
- Measured from Studio's output, which `PlaybackLoudness` doesn't show: nested SoundGroups' volumes multiply, a Sound parented under a group without `SoundGroup` set isn't in it, a group's reverb rings on after its sounds end, stop or are destroyed, and a Sound's own reverb is cut when it ends. Rook already did all of this; `docs/behavior/audio.md` now records it as Confirmed.

## [0.4.5] — 2026-10-15 — M5 step 4: SoundGroups and sound effects

Pinned Roblox version: 0.741.19. Finishes M5 step 4; M5 Audio continues with the modern audio API.

### M5 step 4: SoundGroups and sound effects

- SoundGroups: a Sound is heard through the group its `SoundGroup` names, and through the groups that one is nested in. Each group runs its own effects and scales by its `Volume` (kept within 0 to 10, as in Studio). A group's reverb rings on after its sounds stop.
- Sound effects, picked by archive use: EqualizerSoundEffect (335 files), ReverbSoundEffect (248), PitchShiftSoundEffect (240) and DistortionSoundEffect (229) are heard, under a Sound or a SoundGroup, with `Enabled`. They run in Studio's `Priority` order, including each effect's base priority for 0, which was measured by bisection. They were measured silently: Studio's `PlaybackLoudness` is taken after a sound's own effects, so a sound at Volume 0 showed what each setting does. On the same file, Rook's equalizer and distortion match Studio within 4%. Its reverb and pitch shift are Rook's own, with levels fitted to Studio's. Chorus, Compressor, Echo, Flange and Tremolo keep their properties but aren't heard yet.
- `PlaybackLoudness` on clients is read after the sound's own effects, as in Studio.
- `SoundService.AmbientReverb` adds a reverb to everything heard, using FMOD's preset of the same name. `SoundService:PlayLocalSound` plays a sound once for the client that calls it, without changing `Playing` or firing events. It errors on a server and for anything but a Sound, with Studio's messages.
- `rook-audio` builds without its output device (feature `output`, on by default), so the engine can use the effects.
- Fixed: a new SoundService's `RespectFilteringEnabled` read false; Studio and creator-docs give true, and the reflection database is wrong. Setting `Sound.SoundGroup` to something other than a SoundGroup now errors as in Studio.
- Conformance case `content/sound_groups`. Findings: `docs/behavior/audio.md`.

### Material textures from Roblox

- Material textures come from Roblox like any other asset, with no Studio needed. Each material's textures in each set are a TexturePack asset, whose colour, normal, roughness and metalness images load through asset delivery and the disk cache. Rook used to read them from Studio's local content cache, so materials Studio hadn't shown on that machine drew Rook's own stand-ins, and Studio's distant-view copies drew blurry. Which pack is which material's comes from Studio's own `PartMaterialInfosPre2022` and `PartMaterialInfos2022` tables, checked texture by texture against what Studio showed. Findings: `docs/behavior/materials.md`.
- Removed: reading Studio's content cache (`ROOK_ROBLOX_CACHE`, the `studio-cache` provider) and Rook's own material textures drawn in code. Offline with nothing cached, materials draw untextured, like any missing asset.

### Unions without saved hulls are boxes

- Fixed: a union with no collision hulls saved (`PhysicalConfigData` empty or missing; 247 of a large modern place's unions) collided as its drawn geometry, fetched from its asset. In Studio it is its box: measured in Studio 0.742, such unions read `CollisionFidelity` Box and rays hit them everywhere, even through a doorway. Rook no longer fetches union assets for collision. Also measured: Studio's rays follow `CollisionFidelity` (Default and PreciseConvexDecomposition leave a cut hole open, Hull and Box fill it), and files don't save `CollisionFidelity`. Findings in `docs/behavior/collision.md`.
- `Options::wait_for_content` is `wait_for_sounds` again: only sounds load within the frame.

## [0.4.4] — 2026-10-14 — M15 step 1: unions collide as their shapes

Pinned Roblox version: 0.741.19. Starts M15 step 1; MeshParts' collision shapes follow.

### M15 step 1: unions collide as their shapes

- Unions collide and are queried as their real shape, not their box: the hulls Studio saved in `PhysicalConfigData` or `PhysicsData` (`CSGPHS` versions 3 to 8), scaled with the union like its drawn geometry, or its drawn geometry when none was saved; `CollisionFidelity` Box stays a box, and so does a moving union with `CanCollide` off (for speed). Rays, shape casts, `GetPartsInPart`, `GetTouchingParts`, collisions, the mouse and ClickDetectors all use it, on the server and the client. In two large places, buttons inside wall unions can be clicked. Rules and counts in `docs/behavior/collision.md`.
- `rook content` describes a union asset's collision geometry.
- `Options::wait_for_sounds` is now `wait_for_content`: union geometry loads within the frame too.

### Properties Roblox replicates under hidden twins

- Fixed: in Play mode nothing was heard, and once it was, sounds played at the wrong pitch. Some properties are NotReplicated in the API dump because Roblox sends them to clients under a hidden twin Rook doesn't store: Sound's `Playing` and `TimePosition` (as `PlayingReplicator` and `TimePositionReplicator`) and `PlaybackSpeed` (as `Pitch`). Rook never sent them, so the client, whose sounds the player hears, never played anything, and when it did, played it at speed 1. Checked by recording `rook-player`'s output: a test tone played by a server script came out at full level for as long as the script played it.
- Fixed: the same was true of every property with such a twin. A list of every saved NotReplicated property in the database found them all, and they now reach clients:
  - BasePart's `CollisionGroup` (client raycasts ignored collision groups) and Workspace's `CollisionGroups`;
  - Player's `Team`, `AccountAge` and `MembershipType`;
  - Humanoid's `Jump`, MeshPart's `RenderFidelity` and BallSocketConstraint's `MaxFrictionTorqueXml`;
  - VideoFrame's `Playing` and `TimePosition`.

  A test (`rook-net`, `replicated_twins`) pins the list, so a new database version can't add one silently.

## [0.4.3] — 2026-10-14 — M5 step 3: 3D sound

Pinned Roblox version: 0.741.19. Finishes M5 step 3; M5 Audio continues with SoundService, SoundGroups and effects.

- Sounds under a BasePart or Attachment in Workspace are heard from there: full volume within `RollOffMinDistance`, fading by `RollOffMode` (Inverse, Linear, LinearSquare, InverseTapered) and silent past `RollOffMaxDistance`, with `SoundService.RolloffScale`; panned between left and right as Studio's stereo output pans them. Every other sound is global. Measured from Studio's own audio output; findings in `docs/behavior/audio.md`.
- `SoundService:SetListener` and `GetListener` (Camera, CFrame, ObjectPosition, ObjectCFrame); the camera is the default listener.
- The deprecated `MaxDistance` reads and sets `RollOffMaxDistance`, and a negative roll-off distance reads 0, as in Studio (conformance case `content/sound_3d`).
- The mixer ramps gain changes over one buffer, so moving sounds don't click.

## [0.4.2] — 2026-10-14 — M5 step 2: the Sound API

Pinned Roblox version: 0.741.19. Finishes M5 step 2; M5 Audio continues with 3D sound.

- Sound's methods follow Studio in every state: `Play` starts from the last `TimePosition` a script set (0 after `Stop`) and restarts a playing sound without `Played`; `Stop` and `Pause` do nothing to a sound that isn't playing; `Resume` plays from where it is and fires `Resumed` (it was `Play` before). Events pass the `SoundId`. `IsPlaying` and `IsPaused` read from `Playing`. Conformance case `content/sound_api`.
- Sounds fire `Ended` at their end (without `Changed` for `Playing`), `DidLoop` with the loop count when `Looped`, and `Loaded` once loaded, again when they come back into the DataModel; a seek past the end ends a sound, and a negative one plays from the start. `Volume` is kept within 0 to 10 and `PlaybackSpeed` within 0 to 20.
- `PlaybackLoudness` reads the sound's level on clients (before `Volume`, as in Studio), and a sound with `PlayOnRemove` plays out once when removed.
- The mixer starts a sound over on every `Play`, even near the start. Findings: `docs/behavior/audio.md`.

## [0.4.1] — 2026-10-14 — M5 step 1: audio decoding and playback

Pinned Roblox version: 0.741.19. Finishes M5 step 1; M5 Audio continues with the Sound API.

- Sounds are heard. A Sound's file loads through the content loader like meshes and images (`Want::Audio`), and WAV (PCM and ADPCM), Ogg Vorbis and MP3 decode on its workers, through Symphonia's pure-Rust decoders, to one 16-bit PCM layout; damaged files fail with an error. `rook-player` mixes every playing sound to the default output device through the new `rook-audio` crate, opening it only once something plays; headless runs, `--bench` and `--screenshot` never open one, and `ROOK_AUDIO=off` turns sound off.
- As in Studio: a Sound in the DataModel loads whether or not it plays, and reads `IsLoaded` and its `TimeLength` once loaded (an Ogg's length is its last granule position; conformance case `content/sound`); `TimePosition` stays 0 while the file loads and then advances; a sound whose file fails stays `Playing` and silent at 0; a sound that ends stops and rewinds. A `Looped` sound wraps. Headless runs finish loading a sound within the frame, so runs repeat exactly. Findings: `docs/behavior/audio.md`.
- `rook content` decodes sound files it shows (format, length, rate, channels), and `rook content --place` totals a place's sounds by format and rate.

## [0.4.0] — 2026-10-14 — M4 step 6: unions

Pinned Roblox version: 0.741.19. Finishes M4 step 6 and M4 Content.

### M4 step 6: unions

- Unions draw their own geometry instead of their bounding box. The geometry is read from the asset a union's `AssetId` names (`MeshData` versions 2, 4 and 5, through `rbx_mesh`) and loads, uploads and draws through the same path as mesh files. As in Studio: it's sized by Size / `InitialSize`; with `UsePartColor` off it keeps the colours of the parts it was made from; Decals go on the geometry cut from their face, Textures tile from each face's centre, and surfaces don't show; a union whose asset fails, or that has none, draws nothing; NegateOperations draw in their fixed orange-red. Union assets of 2015-2016 places load anonymously; newer places' need an account and draw nothing without one. Findings: `docs/behavior/meshes.md`.
- `Instance.new` names new unions `Union`, NegateOperations `NegativePart` and IntersectOperations `Intersection`, and unions read their `InitialSize` as `MeshSize`, as in Studio (conformance case `content/union`).
- `rook corpus-scan` counts unions by where their geometry lives and which `MeshData`, `MeshData2` and `PhysicsData` forms they save; `rook content` describes union assets and totals their `MeshData` versions; a file that panics while being counted is recorded as failed instead of ending the scan.

### MakeJoints: wedge slopes from either side

- Fixed: a model's `MakeJoints` tried each touching pair once, from whichever part came first. A wedge's slope joins differently from each side, so a part welded onto a slope came loose whenever the wedge came first. As in Studio, when both parts are being joined and the first makes no joint, the other tries too.

### MakeJoints: balls and cylinders

- Fixed: `MakeJoints` gave balls and cylinders no surfaces, so wheels on Hinge surfaces and studded round parts were never joined (a small place's tractor and tank wheels, a shop's lights). As in Studio, they now join as their boxes: a ball's Hinge makes a Rotate and its Studs a Snap, and a cylinder's caps and curved side weld and hinge, framed at the box face.

## [0.3.1] — 2026-10-14 — M4 step 5: PBR shading and materials

Pinned Roblox version: 0.741.19. Finishes M4 step 5 (materials and PBR); M4 Content continues with unions.

### M4 step 5: MaterialVariant, MaterialService and SurfaceAppearance

- MaterialVariants draw: a part's `MaterialVariant`, or the variant MaterialService's override names for its material (`WoodName`), replaces the material's textures with the variant's colour, normal, roughness and metalness maps, tiled at its `StudsPerTile` and tinted by the part's Color. Only a variant of the part's own material counts, as in Studio.
- MaterialService's `GetBaseMaterialOverride`, `SetBaseMaterialOverride` and `GetMaterialVariant`, with Studio's answers and errors (conformance case `content/materialservice`). Changing MaterialService or a MaterialVariant redraws the scene.
- SurfaceAppearance on MeshParts: its maps through the mesh's UVs, with `AlphaMode` Overlay (the part's Color through the map's see-through pixels), Transparency, TintMask and Opaque, and `Color` tinting the map.
- Fixed: Studio draws a different set of built-in materials when `MaterialService.Use2022Materials` is on; the first build of this step drew the legacy set for every place, having concluded Studio ignores the flag from places that turned out to be legacy ones. Places that save it on now draw the 2022 set (Studio's textures from your content cache, or Rook's own in its colours), at its own scale (8 studs) and shading; places that save it off, don't save it, or have no MaterialService keep the legacy set. Findings: `docs/behavior/materials.md`.
- Fixed: a mesh file whose normals are all zero drew black; its vertices now take their triangle's normal, and it's lit as in Studio.
- Fixed: Rook's index of Studio's content cache didn't notice when the textures it looks for changed, so it could miss ones it had read past before.

### M4 step 5: material fixes

- Fixed: a wedge's material textures stretched along a long, low slope (a large modern place's 98-stud Concrete wedge drew long streaks). Wedge and corner wedge faces now take textures in their own plane, measured along the slope.
- Fixed: Slate and Basalt textures were drawn at half Studio's size (now 16 studs a tile), and Slate used the wrong one of the two normal maps its window caught; it now has Studio's fine layered relief.

### M4 step 5: the built-in materials

- Every material draws with its colour, normal and roughness-metalness textures, tinted by the part's Color as Studio does (multiplied in linear light) and tiled over each face from its top-left corner at Studio's scale (4 to 24 studs; measured against the textures, some fitted). The textures are Studio's own, read from your Roblox content cache once Studio has shown the material (`%LOCALAPPDATA%\Roblox\rbx-storage.db`; `ROOK_ROBLOX_CACHE` points elsewhere or turns it off), or Rook's own, drawn in code with Studio's brightness and scale. Nothing is downloaded. Findings: `docs/behavior/materials.md`.
- Neon glows past its Color as in Studio (grey 128 draws near white). Glass with Transparency filters what's behind it by its Color, and ForceField glows over what's behind it, brighter at grazing angles.
- `rook-content` reads KTX2 images (BC1 and BC3, Zstandard-compressed levels), the form Roblox's material textures come in, and has a new provider, `studio-cache`, between the install and content folders (`ROOK_CONTENT_ORDER`). Where the material textures are in the cache is indexed in Rook's cache, so only the first run reads the whole cache (about 6 s, on a background thread).
- Studio 0.742 draws the 2022 materials whatever `MaterialService.Use2022Materials` says (a place saved with it off fetched and drew the same textures), so Rook draws that one set for every place.

### M4 step 5: PBR shading

- Every material shades with its own roughness and metalness, measured from Studio's own material maps: a GGX sun highlight (sharp on SmoothPlastic, Ice, Glass and Metal, soft on Plastic) and the sky reflected with Fresnel. `Lighting.EnvironmentSpecularScale` sets how much parts reflect the sky and how far metals trade their diffuse light for it; at 0, which places saved without it read, Foil is lit like Plastic, as in Studio 0.742. Plastic's surfaces, Neon and Reflectance look as before. Findings: `docs/behavior/materials.md`.
- Internal: `Enum.Material` is defined once, in `rook-types` (`Material`, with each material's value, name and shading); physics' density and friction table is keyed by it. Materials not in the enum still draw and simulate as Plastic.
- Docs: where Studio keeps the material textures it downloads (its `rbx-storage` cache, as KTX2 texture packs), and that Studio 0.742 draws the 2022 materials whatever `Use2022Materials` says (`docs/behavior/materials.md`).

## [0.3.0] — 2026-10-14 — M3: physics; M4 steps 1–4

Pinned Roblox version: 0.741.19. Finishes M3 Physics; M4 Content is in progress (built-in mesh shapes, asset fetching, file meshes, textures and decals, trusses, skyboxes).

- Internal: source and docs reorganized, no behaviour change. Large files split by topic (one topic per file, `foo.rs` plus `foo/`, no `mod.rs`); Luau sources in `rook-lua/src/api/luau/`, shaders in `rook-render/src/shaders/`; typed property readers on `Value`, shared by physics and the player; docs grouped into `docs/guide/`, `docs/design/` and `docs/behavior/`, with M4's findings moved out of the roadmap; `scripts/check` lists files over 600 lines.
- TrussParts draw as Studio draws them (M4): four corner rails with rungs every 2 studs and the braces of each Style (AlternatingSupports, BridgeStyleSupports, NoSupports), at any length and along any axis, from Studio observation and its OBJ export. Trusses of the same length and Style share one mesh and draw instanced. Setting a truss's Size snaps it as Studio does (conformance case `content/truss`).
- Fixed: a truss's `Style` loaded from a file read as AlternatingSupports (files save it as `style`).
- README: a new pitch (offline singleplayer, self-hosting, preservation, modding, opt-in extensions), credits moved up as "Standing on the shoulders of giants" with Rapier added, and the status brought up to M3. ROADMAP lists the planned Rook extensions (unscheduled).
- Docs split: the README is now a short overview (287 to 108 lines) that links to `docs/guide/getting-started.md` (building, playing, controls, command bar), `docs/guide/tools.md` (the `rook` CLI), `docs/guide/features.md` (the full list, now with physics) and `CONTRIBUTING.md` (reporting problems, crate map, development).
- `rook-physics`: Workspace parts are simulated with Rapier on the server at 240 Hz. Anchored parts are fixed; each unanchored part is its own body for now. Workspace.Gravity and FallenPartsDestroyHeight apply; CFrame and assembly velocities are written back and replicate.
- Joints and assemblies (M3 step 2): Weld, ManualWeld, Snap, Glue, ManualGlue, Motor, Motor6D and WeldConstraint join parts into assemblies, each simulated as one body. As in Roblox, every anchored part roots its own assembly, which takes along the unanchored parts joined to it; anchored parts joined to each other stay separate. Rotate, RotateP and RotateV are free hinges between assemblies. Motors turn toward `DesiredAngle`, and Motor6D applies `Transform`.
- Scripts: `MakeJoints` and `BreakJoints` (BasePart, Model, Workspace) follow Studio's surface rules. Also added: `GetJoints`, `GetConnectedParts`, `GetRootPart`, `IsGrounded`, `AssemblyRootPart`, `AssemblyMass`, `Motor:SetDesiredAngle` and `getMass`. Setting a part's `CFrame` moves its whole assembly; setting `Position`/`Orientation`/`Rotation` moves only that part and rewrites its side of its joints.
- Places saved before `Workspace.ExplicitAutoJoints` existed have their parts joined by surfaces when they load, as Roblox did then (Inference: such places save almost no joints).
- `rook-dom` indexes reference properties both ways (`Dom::referrers`).
- Contacts and materials (M3 step 3): every material's density, friction and elasticity (`PhysicalProperties.new(material)`), and `CustomPhysicalProperties`; touching parts mix friction and elasticity by their weights, as measured in Studio. `GetMass` uses the real shape volume (balls, cylinders, wedges).
- `Touched` and `TouchEnded` fire from contacts, on both parts; `CanTouch` turns them off.
- Collision groups: `BasePart.CollisionGroup`, `CanCollideWith` and PhysicsService's group methods (current and deprecated), stored in Workspace's `CollisionGroupData` as Roblox saves it; `TouchesUseCollisionGroups` is honoured.
- Physics conformance cases (`physics/joints`, `physics/contacts`).
- Faster scripting: member lookups are resolved once per class, `inst.Key` and `inst.Key = v` take a dedicated path into Rust, the engine-function shim no longer forwards results through a vararg helper, `GetChildren`/`GetDescendants` build their arrays in Luau, and the script-timeout check reads the clock every 1024th interrupt. A large modern place's 10 s run drops from about 46 s to 32 s, with identical output.
- Command bar: in an interactive terminal, `rook-player` shows a `>` prompt that runs Luau in the open place, like Luau's REPL (expressions print their value, globals persist, unfinished statements continue on `>>` lines, line editing and history, output above the prompt; `:server`/`:client` pick the side); `rook run-headless --repl` does the same without a window. It is the `repl` Cargo feature, on by default; `--no-repl` turns the prompt off.
- Fixed: engine functions that return nothing (`print`, `Destroy`, ...) returned one `nil` since the faster engine-function shim, which changed `select("#", ...)`.
- BodyMovers and constraints (M3 step 4), from Studio measurements: BodyForce, BodyThrust, BodyVelocity, BodyAngularVelocity, BodyPosition, BodyGyro and RocketPropulsion (`Fire`, `Abort`, `ReachedTarget`) push their part's whole assembly every 240 Hz step. BodyVelocity and BodyAngularVelocity are velocity constraints limited only by MaxForce/MaxTorque; BodyPosition and BodyGyro are mass-independent spring-dampers (BodyGyro at a quarter strength, on the sine of the angle, with MaxTorque limiting angular acceleration).
- RotateV and RotateP are driven by their surface's `SurfaceInput`, `ParamA` and `ParamB` (RotateV turns at 31 rad/s per unit; RotateP steps 0.01 rad per step toward its angle; NoInput holds); Rotate stays free.
- HingeConstraint (free, Motor, Servo, limits, `CurrentAngle`), BallSocketConstraint, RopeConstraint and RodConstraint become Rapier joints between attachments; SpringConstraint, VectorForce and Torque apply forces.
- Fixed: the deprecated `Velocity` and `RotVelocity` were stored apart from the velocities physics uses, so scripts launching parts with them (a small place's volcano, meteors, tornado) did nothing. They're now views of `AssemblyLinearVelocity`/`AssemblyAngularVelocity` (`Velocity` reads the assembly's velocity at the part), velocities saved in files are adopted at load, and setting any of them sets the whole assembly's. Anchored parts keep their velocity and carry what rests on them, like Roblox's conveyors.
- Fixed: `MakeJoints` treated wedges as boxes, so parts resting on a wedge's slope never joined, and a small place's barn lost its roof as soon as the map was unanchored. A wedge's slope is now a face and its box's top and front aren't, following Studio: a wedge calling MakeJoints joins what's on its slope only through its own Weld surfaces (TopSurface, then FrontSurface); a part calling it sees the wedge's FrontSurface there; rotating surfaces on a slope make nothing.
- Physics conformance cases `physics/movers`, `physics/constraints` and `physics/velocity`. `rook conformance record` picks the Studio window whose place name contains `ROOK_STUDIO` when several are open.
- `ROADMAP.md`: the one place for the plan. Content (meshes, textures, decals) moves ahead of characters, which need the head mesh; then GUI, audio and networking. Other docs name milestones instead of numbering them.
- Fixed: RotateV had unlimited torque and Rapier capped every body's spin at a quarter turn per step, so a helicopter rotor (a small place's fort) slammed to full speed in one step and flung the whole helicopter. RotateV's torque is now limited by the smaller part's size, as measured in Studio, so heavy rotors spin up over seconds, and parts can spin as fast as Roblox lets them (1,550 rad/s at ParamB 50).
- Tracy zones per resumed script (`script: <chunk>:<line>`), per `Instance.__index`/`__newindex` (key as zone text), per native Instance method, and around all engine functions (`api`).
- Scene queries (M3 step 5), on Rapier's query pipeline and measured in Studio: `FindPartOnRay` (and `findPartOnRay`), `FindPartOnRayWithIgnoreList`, `FindPartOnRayWithWhitelist`, `FindPartsInRegion3` (and `findPartsInRegion3`), `FindPartsInRegion3WithIgnoreList`, `FindPartsInRegion3WithWhiteList`, `IsRegion3Empty`, `IsRegion3EmptyWithIgnoreList`, `Raycast`, `Blockcast`, `Spherecast`, `GetPartBoundsInBox`, `GetPartBoundsInRadius`, `GetPartsInPart`, `BasePart:GetTouchingParts` and `GetNoCollisionConstraints`. Rays reach 15,000 studs and ignore parts they start in; `CanQuery` only counts with `CanCollide` off; legacy rays act as the `Default` collision group and regions ignore groups; regions and `GetPartBounds*` test each part's `Size` box, `GetPartsInPart` and `GetTouchingParts` real shapes. Queries see parts created or moved earlier in the same frame.
- New datatypes `RaycastParams`, `OverlapParams` and `RaycastResult` (`FilterType` with the old Whitelist/Blacklist names, nested filter tables, `AddToFilter`, `CollisionGroup`, `RespectCanCollide`, `MaxParts`), with Studio's messages, which reach `pcall` as plain strings.
- Camera: `ViewportPointToRay`, `ScreenPointToRay`, `WorldToViewportPoint` and `WorldToScreenPoint`; the client's camera gets the window size as `ViewportSize`.
- Clients have a physics world too, built on their first query and never stepped, so LocalScripts can raycast. The mouse (`Mouse.Hit`, `Target`, `TargetFilter`) and ClickDetectors use the same queries, and `rook-engine`'s separate ray geometry (`raycast.rs`) is gone: part shapes are defined once, in `rook-physics`.
- Physics conformance case `physics/queries`.
- Built-in mesh shapes (M4 step 1), measured in Studio: parts with a SpecialMesh, BlockMesh or CylinderMesh draw the mesh's shape instead of their own. Brick, Torso and BlockMesh draw a block, Wedge and CornerWedge their wedges, Sphere a ball stretched on every axis, SpecialMesh Cylinder a cylinder along X and CylinderMesh one standing along Y; the deprecated Prism, Pyramid, ParallelRamp and RightAngleRamp draw nothing. `Scale` multiplies the part's Size before each type's rule, and `Offset` moves the mesh in the part's axes. Of several meshes, the last child counts; a file mesh with no `MeshId` is a 1-stud box times `Scale`. Physics and queries keep the part's own shape. The size rules live once in `rook-types` (`MeshKind`, `MeshLook`).
- Faster physics rebuilds and scripting, measured on a 2017 place's sequence ( forced through the place's own round code): server+client step early in the sequence 28.2 ms → 13.3 ms per frame, late in it 22.0 ms → 18.2 ms; the windowed player runs the sequence at 0.88-0.98 game seconds per real second instead of about 0.68; the 70 s before it run in 2.1 s instead of 3.2 s. Every change except the in-place collider updates below leaves output identical (console logs, a per-10-second digest of every part's position, velocity and material, and a mid-sequence screenshot); every conformance case passes. A large modern place's 10 s run drops from 26.7 s to 25.5 s, with identical output.
  - A physics rebuild walks each cluster of unanchored parts once. It used to walk the whole cluster once per joint into it from every anchored part, so recolouring one part of a welded map cost a whole-map walk per joint (50-85 ms each, up to 580 ms in a frame).
  - Scripts cache Instance methods per class in Luau, so `part:FindFirstChild(...)` no longer crosses into Rust to fetch the method; reads of keys that are no class's method skip that cache. `inst.Key` reads its key without copying it, fetches the engine context once, and its shim runs `type()` on Luau's fast path (an environment marked `safeenv` that holds only the builtins it uses).
  - Changing a part's `Material`, `CustomPhysicalProperties` or `CanTouch` updates its collider in place instead of rebuilding its assembly, and wakes the part and what touches it (observed in Studio 0.742: a part at rest on a 30° ramp slides as soon as its friction drops). Unlike a rebuild, this keeps the simulation's contact state, so long runs differ from before in the details of how piles of parts settle.
  - Engine-internal maps keyed by ids and names (reflection lookups, instance properties, defaults, referrers, signal connections, member tables) use a fast deterministic hash (`rustc-hash`).
- The Head MeshType draws Rook's own head mesh, built in code to match the vertex layout measured from the user's Roblox install (36 segments, rounded rims in four steps, smooth shading), fitted to the part as Roblox fits it. No Roblox file or data is included.
- Changing a mesh (its type, `Scale` or any property, or adding, removing or moving one) redraws its part.
- Asset fetching (M4 step 2), new crate `rook-content`. Content references are parsed once (`ContentRef`) by the rules Studio follows, checked form by form: `rbxasset://` paths, `rbxassetid://` ids and legacy asset URLs on any host, with their case, slash, spacing and `version=` variants; anything else is no content. They resolve through a configurable chain: Rook's defaults, the user's Roblox install (found automatically, newest first, or `ROOK_ROBLOX_INSTALL`), content folders (`ROOK_CONTENT_DIRS`) and Roblox's asset delivery (anonymous; an Open Cloud key from `ROOK_OPEN_CLOUD_API_KEY` for assets refused anonymously; `ROOK_ASSET_DELIVERY=off` turns the network off). Each answer says where it came from.
- 262 old `rbxasset://` paths of the 2009 client (`fonts/sword.mesh`, `sounds/collide.wav`, `textures/SpawnLocation.png`), which archived places use and modern installs lack, resolve to the public assets Roblox uploaded for them, as they do in Studio.
- A content-addressed, hash-checked disk cache outside the repository (`ROOK_CACHE_DIR`): a second run needs no network, damaged files are fetched again, and "not found" or "forbidden" answers are remembered for a day.
- `ContentProvider:PreloadAsync` and `RequestQueueSize`, from Studio observations: on a client, entries resolve on worker threads and callbacks run on the engine thread as each finishes, then the call returns; on a server it never returns once there is something to fetch, as in Studio. Conformance case `content/preload`.
- `rook content <reference>` shows how a reference resolves (provider, file or URL, size, hash, file type; for meshes, version, triangles and size); `rook content --place <file>` does it for every content property in a place and totals the results by provider and failure. `rook corpus-scan` reports content properties, URL shapes and what `rook-content` makes of them.
- The player draws the Head MeshType with the `head.mesh` from the user's Roblox install when there is one (decoded by `rook-content` through the `rbx_mesh` crate, which reads mesh versions 1.00 to 5.00), and Rook's own head otherwise.
- Fixed: places store their content in the `Content` properties (`MeshContent`, `TextureContent`, `AudioContent`, ...), so `MeshId`, `Texture`, `SoundId` and the other string properties read empty in scripts, and every file mesh from a file drew as a 1-stud box. They're now views of the `Content` properties, and setting one turns backslashes in `rbxasset://` paths into slashes, as Studio does.
- Rook now needs Rust 1.94 or later (for `rbx_mesh`; the code already needed 1.88 for `if let` chains, though 1.85 was declared).
- File meshes (M4 step 3), measured in Studio: SpecialMesh and FileMesh `MeshId`s and MeshParts are drawn, in the part's colour. A file mesh is sized by `Scale` alone and a MeshPart by Size / `InitialSize`, each with the mesh's own origin at the part; mesh children of MeshParts are ignored, those of unions drawn at 1 stud. Files load and decode in the background and appear when they arrive: a new mesh draws nothing until then, and a mesh whose `MeshId` changes keeps its old look until the new file arrives, or for good if it fails. Parts sharing a mesh file share one GPU mesh, drawn instanced. Places without file meshes start no loader.
- `MeshPart.MeshSize` reads the size saved as `InitialSize` (it read 0); `ContentProvider:PreloadAsync` reports `Failure` when a `MeshId` isn't a mesh, as in Studio. Conformance case `content/meshpart`.
- Fixed: a negative mesh `Scale` mirrored the built-in shapes and drew them inside out, and broke the Sphere and Cylinder sizes; Studio ignores the sign for them (observed with a Wedge) and mirrors only mesh files, as Rook now does.
- Permission errors name the capability as Roblox does (`lacking capability NotAccessible`, not `NotAccessibleSecurity`).
- `rook content --place` decodes every mesh reference and totals them by version and failure; `rook corpus-scan` counts mesh instances by kind; `rook-player --fov` sets the field of view.
- Rendering uses reverse Z with an infinitely far plane, so distant surfaces no longer z-fight from depth-buffer imprecision (with the old 0.1–10,000 stud range, depth resolved only to about 0.15 studs at 500 studs away).
- Smoother play: the server runs on its own thread, and simulating and drawing cost much less. Measured on a large 2016 place in normal play (30 s from the start) and on a 2017 place forced through its own round code into the sequence and light sequence (the place's `math.randomseed` fixed so every run plays the same round; t = 60–160 s). A step is one 60 Hz frame of server and client, in lockstep (`rook-player --bench`); window frames are 30 s of live play, uncapped:

  | | step median | step slowest 1% | longest step | window before (vsync / uncapped) | window after, uncapped |
  |---|---|---|---|---|---|
  | A large place | 13.5 → 9.9 ms | 39.1 → 34.5 ms | 233 → 249 ms (loading) | 22 / 22 FPS, longest frame 107 ms | 1,590 FPS, longest 7.7 ms, 0 over 16.7 ms |
  | Heavy sequence | 23.2 → 12.0 ms | 59.9 → 36.8 ms | 82 → 93 ms | 23 / 117 FPS, longest 158 ms | 3,010 FPS, longest 4.0 ms, 0 over 16.7 ms |
  | Light sequence | 2.9 → 2.4 ms | 17.9 → 14.1 ms | 29.8 → 23.8 ms | 60 / 688 FPS, longest 24 ms | 3,100 FPS, longest 4.4 ms, 0 over 16.7 ms |

  Output is identical in all three: console logs, a per-10-second digest of every part's position, velocity, material and colour, and a screenshot; so is `rook run-headless` on a large modern place, a small place and a 2008 place. Every conformance case passes. A large modern place's 10 s headless run drops from 33.0 s to 25.9 s; small places run about 0.1 s slower per 10 s (a 2008 place 0.30 → 0.40 s), Rapier's per-step cost for running in parallel.
  - The server steps on its own thread at 60 Hz, so the window waits only on its client and the renderer and draws as often as the display allows. `rook-engine`'s `host::ServerHost` and `ClientHost` only exchange messages; `Session` runs them on one thread. Replication is split into a server `Source`, which describes each change as a self-contained update (current values, new subtrees in full), and a client `Sink`, which maps ids and applies it. `--seconds`, `--bench` and screenshots keep the client in lockstep, so they give the same results every run.
  - `--uncapped` presents without waiting for the display. The window title shows the last second's frame times (FPS, median, slowest 1%, longest, frames over 16.7 ms, server and client steps); `--frame-times` prints them too, with a summary at exit. `--bench` reports server, client and total steps by median, slowest 1% and longest.
  - Collision detection and the solver run on half the cores (Rapier's `parallel` feature, whose results don't depend on the number of threads).
  - The script timeout is checked by a plain C interrupt instead of an mlua closure, which had made every Luau loop iteration three times slower and every call 2.7 times slower. It is the only `unsafe` code in Rook, confined to `rook-lua/src/watchdog.rs`.
  - Reading a plain stored property (`.Parent`, `.Anchored`, `.Name`, ...) and calling `IsA`, `FindFirstChild` or `GetChildren` take a fast path once the class has been seen: the property by a small id, instances returned as keys that the Luau shim turns into userdata. Per call: `.Parent` 0.65 → 0.30 µs, `IsA` 0.82 → 0.48, `FindFirstChild` 0.77 → 0.40, `GetChildren` 2.6 → 1.3. The general path stays the reference for everything unusual.
  - A physics rebuild looks up each part's joints once instead of up to four times, and services are found by class id instead of by comparing names.
  - Drawing: a part that only moved takes its new place from its CFrame; the renderer rewrites just the changed instances and uploads each batch once a frame; transparent parts share one buffer instead of one per part per frame; a part added or removed takes or frees a slot instead of rebuilding the scene (a large place rebuilt all 29,436 parts about once a second, a 45 ms frame).
- Fixed: physics kept bodies, motors and constraints in hash maps with a random order, so a round of light sequence played out differently on every run. It now uses deterministic maps.
- Fixed: holding the right mouse button to look around left the cursor visible and free to wander out of the window. It now hides and stays where it was pressed (held in place, or put back after each move where the platform can only confine it to the window).
- Fixed: changes a server script made while handling a client's message at the end of a frame were trimmed from the journal before the next frame's script and render checks saw them, so a script it parented never started.
- Sun shadows: the renderer draws a 2048² shadow map from the sun (depth only, reverse Z like the main pass) over the opaque parts, fitted around the camera's view out to 300 studs and moved in whole texels so edges don't shimmer. Only the sun's light is shadowed; Ambient and OutdoorAmbient still light shadowed areas. Edges are softened by 3×3 filtered comparison samples. `Lighting.GlobalShadows` off skips the pass entirely, as does a sun below the horizon; `BasePart.CastShadow` off keeps a part from casting while it still receives. Transparent and invisible parts don't cast.
- Fixed: the sun was placed by a textbook solar formula, too low and too far towards +Z (at 14:00 and the default latitude, 33° up instead of 55°). It now follows `Lighting:GetSunDirection()` as measured in Studio: the sun circles the Z axis once a day, rising at +X, on a circle tilted by the latitude less 23.5°.
- Textures and decals (M4 step 4, from Studio measurements):
  - Images load and decode in the background like mesh files (`Want::Image`; PNG, JPEG, BMP and DDS, the formats found among the archive's images), and parts sharing an image share one GPU texture. Uploads are spread over frames, and a frame never waits for decoding or uploading. Places without images pay nothing extra beyond a larger per-part instance.
  - Decals: one image per face (`Face`), oriented as Studio orients it, with `Color3`, `Transparency` and `ZIndex`, on every part shape and mesh. On balls, cylinders and wedges a decal is projected onto the part's box; on file meshes and the Head it goes through the mesh's UVs; on a MeshPart it is projected onto the mesh's box. Decals show on invisible parts, and on half-transparent parts they're drawn whole. Textures tile by `StudsPerTileU`/`V` and `OffsetStudsU`/`V` from each face's top-left corner. Images never cast shadows.
  - Mesh `TextureId` and MeshPart `TextureID`, multiplied by `VertexColor`; version 1.00 meshes' flipped V is confirmed. While a mesh texture loads the mesh is black, and a failed one leaves the part's colour; a decal draws nothing while loading or after failing.
  - Rook's own head mesh has UVs now, measured from the user's `head.mesh`, so face decals sit where Roblox puts them.
  - Plastic draws its surfaces (studs, inlets, universal, and the weld/glue lattice) from the install's `studs.dds`, or Rook's own patterns without an install; SpecialMesh spheres and cylinders show them too. Neon is drawn unlit, and `Reflectance` tints a part towards the sky colour (an approximation). The other materials are plain: their textures aren't in Roblox's install.
- Scripts: setting a Decal's or Texture's `Texture` (or a Decal's `ColorMap`) to a web URL with an `id` stores `rbxassetid://<id>`, and setting `MeshId` or `TextureId` on a SpecialMesh makes it a FileMesh, as in Studio (conformance case `content/decal`). `ContentProvider:PreloadAsync` reports `Failure` for something that isn't an image in an image property.
- `rook corpus-scan` counts materials, surfaces, decal faces, texture tiling, `VertexColor`, `Reflectance` and mesh textures; `rook content --place` decodes every image reference and totals them by format.
- The download cache moved to `%TEMP%\Rook\cache` on Windows, so Storage Sense and cleanup tools can clear it; an existing cache in `%LOCALAPPDATA%\Rook\cache` moves there once by itself. Rook keeps the cache under `ROOK_CACHE_MAX` (2 GB by default), dropping what was used least recently, and the new `rook cache` command shows its size or prunes (`--older-than <days>`), trims or clears it.
- Skyboxes: the place's Sky is drawn behind everything, each image placed and turned as Studio does it (Left is at +X and Right at -X, Up's top towards +X), and a place without a Sky shows the default sky from your Roblox install. Reflective parts take on the skybox's average colour, as in Studio. A large 2016 place's barrels (New Yeller, Reflectance 0.3, under an all-black Sky) were washed out towards Rook's default blue; they're olive now, as in Studio.
- Balls and cylinders are built as Roblox builds them (measured from Studio's OBJ export): a box with each face split into a 6 × 6 grid pushed out onto the round surface, each patch mapping images from its box face. Decals on round parts no longer stretch: on a CylinderMesh barrel they were about 13% too wide.
- Fixed: decals on a CylinderMesh were turned 90 degrees (projected in the upright cylinder's axes instead of the part's), and decals fought with other parts' faces lying in the same plane; they now draw on top, as in Roblox.
- Fixed: parts with a BlockMesh drew their studs and inlets; Studio draws them plain (a Brick SpecialMesh keeps them).
- Fixed: with `ROOK_OPEN_CLOUD_API_KEY` set, assets refused anonymously in an earlier run (and cached as refused) weren't retried with the key. The key path is now tested: a large 2016 place resolves 544 of its 704 distinct references with one.

## [0.2.0] — 2026-10-11 — M2: client side, renderer, input

Pinned Roblox version: 0.741.19.

- `rook-render` and `rook-player`: wgpu renderer for parts and primitive meshes, camera, basic lighting and sky; fly camera; `--screenshot`, `--bench`.
- Client/server split: separate DataModels joined by the in-process replicator (`rook-net`); `Player`, `Backpack`, `PlayerGui`, `PlayerScripts`; LocalScripts.
- RemoteEvent, UnreliableRemoteEvent and RemoteFunction.
- TweenService, Tween and the legacy GUI tweens.
- Input: UserInputService, `Mouse`, ClickDetector, mouse ray queries.
- Build profiles (`dev`, `release`, `profiling`) and Tracy zones; incremental scene updates.
- Console output in Roblox Studio's format, coloured by message type.
- Every inert or partial API is listed in `docs/behavior/stubs.md`.

## [0.1.0] — 2026-10-11 — M0 and M1: formats, DOM and scripting

Pinned Roblox version: 0.741.19.

- M0: workspace, datatypes, class database, binary and XML place formats, the legacy XML upgrade (99.3% of the archive loads), `rook dump/roundtrip/corpus-scan`.
- M1: `rook-dom`, `rook-signal`, `rook-scheduler`, the Luau bridge (`rook-lua`), the core Instance API and the members list for 0.741.19; headless server Scripts (`run-headless`, `corpus-run`); conformance through Studio's MCP server.
- Renamed from Raven to Rook.

[Unreleased]: https://github.com/AsyncFinn/rook/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/AsyncFinn/rook/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/AsyncFinn/rook/releases/tag/v0.1.0
