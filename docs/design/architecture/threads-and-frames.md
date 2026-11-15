# Threads and frames

Which thread runs what, what each frame does in what order, and how bursts of work are spread over frames. Part of the [architecture](../architecture.md).

## Threading and data flow

Each DataModel's Luau, DOM and physics stay on one thread ([Signals and scheduler](scripting.md#signals-and-scheduler-rook-luas-signal-and-scheduler)). In singleplayer Play the player runs two DataModels on two threads, so the window waits only on its client and the renderer:

```
   server thread                                   main thread (window)
  ┌──────────────────────────────────┐            ┌─────────────────────────────────────┐
  │ server DataModel                 │  updates,  │ client DataModel                    │
  │  formats → DOM ⇄ Luau (scripts)  │  remotes   │  DOM ⇄ Luau (LocalScripts), input   │
  │   │ change journal               │ ─────────→ │   │ change journal                  │
  │   ├─→ physics (Rapier) ──┐       │            │   ├─→ queries (mouse, raycasts)     │
  │   └─→ replication source │       │ ←───────── │   └─→ scene cache ──→ renderer      │
  │ steps at 60 Hz of real time      │  remotes,  │ steps at 60 Hz; draws as often as   │
  └──────────────────────────│───────┘  clicks    │ the display allows (or uncapped)    │
                     physics workers (half the     └─────────────────────────────────────┘
                     cores: collision detection
                     and the solver, in parallel)
```

- The ends are `rook-engine`'s `ServerHost` and `ClientHost`, which only exchange messages (`host::ToClient`, `host::ToServer`) with Rook-to-Rook replication: the server's replication `Source` describes each change as a self-contained update (its current values, a new subtree in full) and the client's `Sink` maps server ids to its own instances and applies it. Remote events, invokes and ClickDetector clicks travel the same way, with instance ids translated by the client.
- The server's DataModel is made on its thread (`rook-engine`'s `session::server_thread`, which the player and `rook-server` share) and never leaves it (a Luau VM isn't `Send`). It steps by itself at 60 Hz in live play, and only when told to for `--seconds`, `--bench` and screenshots, which keep the client in lockstep so results are the same every run. `Session` runs both ends on one thread, delivering every message at once (headless runs, tests, conformance).
- Physics parallelises inside a step on a shared pool of worker threads, half the logical cores. Rapier lays the work out the same whatever the pool's size, so results don't depend on it.
- Run mode (no player) keeps the server on the main thread, since the window draws it.
- **Render thread** (pipelined rendering, as Bevy's pipelined rendering and Unreal's game and render threads run it, and as Roblox's render thread does: [frame threads](../../behavior/rendering/frame-threads.md)). The renderer and the window's surface live in one value that is handed to a render thread with each frame and handed back when it's drawn (`RenderThread`), so nothing is shared behind a lock. Frame N records, submits and presents there while the main thread steps frame N+1 and brings the scene cache up to date (`SceneCache::update`, which needs no renderer). The one moment both hold the frame is the hand-over (pipelined rendering's extract): the main thread waits for frame N, applies frame N+1's changes (`ToRender::apply`) and the cursor, then sends frame N+1 with the camera its step left. A frame costs about the slower of the two sides instead of their sum. Overlap doesn't always pay: the two sides contend for cores (a heavy place's render ran twice as long beside the step on a 6-core machine), and under a frame rate cap or vsync that serial frames keep to it buys nothing but older input. So the pacer measures both ways (`render_thread::Pacer`), Unreal's `r.OneFrameThreadLag` chosen per frame: frames run one after the other while that keeps to the cap or refresh, else whichever way gives shorter frames, re-measuring the other way every so often.

Scripts read some derived values synchronously: `AbsolutePosition`, `AbsoluteSize`, `TextBounds`, `TextFits`, raycasts, `GetPartsInPart`. So **GUI layout, text measurement and physics queries live on their DataModel's thread**. The renderer only consumes the client's scene, so no tree is ever shared between threads behind a lock.

## Copies of state and their owners

A copy of state another part owns, updated by its own code path, drifts from the owner and goes stale. Each copy below is either a cache the journal (or a key of everything it follows) invalidates, or kept by one choke point that the types or a lint make the only way to change it. Checked 2026-10-30.

| Copy | Owner | Kept by |
|---|---|---|
| The physics world: bodies, colliders, joints, movers, Humanoids, gravity, fallen height, collision groups | The DOM | The journal, read at each sync; queries sync first |
| The render scene (`SceneCache`), each adornment's Adornee, lights, effects | The DOM | `RenderChanges`, filled from the journal |
| GUI layout and the `Absolute*`/`Text*` it writes; world GUIs, Hints, Modal buttons, tags over heads | The DOM | The journal (first look scans the tree); a script's read lays out first |
| Scripts to start or stop, sounds, unions, Explosions, the player's settings (`Kept::current`) | The DOM | The journal, read before every trim |
| The focused TextBox | `Gui::focus`, the one record; `GetFocusedTextBox` and the caret both read it | `UserInputService`'s focus and release |
| Mouse `Hit`, `Target`, `Origin` and the ClickDetector under it | Camera, cursor, viewport, `TargetFilter`, physics | Recomputed when any of those (`Physics::generation`) changed |
| The game's mouse position | The engine (`World.input.mouse`) | The window's moves; a mouse locked to the center is put there by the engine (`hold_mouse`), each held move and each frame |
| The system cursor's place | The window | Only `App::warp` moves it, and tells the game (clippy ban) |
| The system cursor's look and visibility | The game's `cursor_look`, and looking around | Only `App::show_system_cursor` changes it, once a frame (clippy ban) |
| The drawn camera | The client step | `App::update` returns `Drawn` after the step, which `App::draw` takes |
| `Camera.ViewportSize` | The window's size | The host's `set_viewport`, and each frame for a replaced camera |
| What the window applied: its grab (`held`), IME, settings followed (`player`), its own held keys | The window | Compared with the game's answer after each step (`mouse_lock`, `typing`, `player_settings`); keys from the same handler that tells the game |

## Frame order

What a frame draws is settled from that frame's input and camera. Anything shown that follows the camera, the mouse or a script's render-step write is done after the render step and before drawing, never left to the next frame's physics or step: one frame behind (7 ms at 144 Hz) is felt, and the camera and character controllers kept hitting it. A client's frame (`DataModel::step_frame`):

1. Input the window collected since the last frame (`App::apply_inbox`), then content, sounds and scripts' Stepped.
2. Physics, in whole 240 Hz steps, then Heartbeat. Physics reads what scripts set up to here: Humanoids' drives, including the camera heading a CameraRelative character holds.
3. The render step: bound functions, RenderStepped, PreRender, then the engine's own camera moves. The camera is final from here.
4. What depends on the final camera or input, before drawing: a CameraRelative character faces the camera (`rook_lua::face_camera`), the mouse is held, then the drawn poses (`place_drawn`), GUI layout and hover.
5. The window takes the drawn camera and the scene's changes from after the step (`Drawn`), the hovers, cursor and sounds, and hands them to the render thread at the hand-over; the camera and the scene drawn always come from the same step.
6. A frame that runs one after the other is waited for before the next takes its input. An overlapped frame's successor starts just in time (`render_thread::Pacer`): as late as it can and still be ready when the render thread finishes, steered by whether each hand-over waited or found the render thread idle, the start NVIDIA Reflex and AMD Anti-Lag make. So its input is no older when shown than it has to be, while frames come at the slower stage's rate. `pipelined_frames_cost_the_slower_half_with_no_added_latency` checks this at 144 Hz budgets and fails when frames start at once; `frames_that_keep_to_the_cap_run_one_after_the_other`, `frames_that_miss_the_cap_overlap_to_keep_to_it` and `frames_that_overlap_no_faster_run_one_after_the_other` check the choice.

Where Roblox's own order makes a result a frame late for scripts (Studio measured a CameraRelative character two frames behind its camera in script reads), Rook still draws it current. Each of these has a test that steps a 144 Hz client and checks what's drawn against the camera drawn with it, and fails on a frame's delay: `a_camera_relative_character_is_drawn_facing_this_frames_camera`, `a_144_hz_client_draws_its_character_moving_evenly`. A new consumer of the camera or input gets one too. Audited 2026-11-03: audio's listener, Mouse.Hit and ClickDetector hover, the cursor, render-step `CFrame` and joint writes, and the renderer's camera-relative work all read the current frame.

## Per-frame budgets

A frame has 16.7 ms at 60 Hz, and a stage that does all of a burst in the frame it arrives in spends a burst's time there: a texture shared by 41,000 parts arriving, 24 MB of images going up, a table outgrowing its layout. So **no stage's work in a frame grows with a burst.** Steady work costs what changed, never the place's size; work that arrives in bursts waits in a queue, ordered so what matters goes first, and each frame drains it until a fixed budget is spent, at least one item so it always moves (time-sliced work queues, as Unreal's `s.AsyncLoadingTimeLimit` and Unity's `asyncUploadTimeSlice` bound streaming). Structures that would stall the frame to grow are sized once at startup. What waits shows what it showed before, as a later download would; anything visible held back follows Roblox's own measured rule, never an invented cutoff.

| Burst | Queue and budget | Standard followed |
|---|---|---|
| Mesh files and images arriving, and the parts waiting for them | `SceneCache`'s arrivals (`rook-scene/src/content/arrivals.rs`), nearest the camera first, 2 ms a frame; that bounds the hand-over and instance uploads behind them too | Time-sliced streaming (Unreal `s.AsyncLoadingTimeLimit`) |
| Files going to the GPU | `GpuCache` (`rook-render/src/gpu/cache.rs`), 2 ms and 24 MB a frame | Unity's `asyncUploadTimeSlice` and `asyncUploadBufferSize` |
| Images outgrowing the image table | Sized to its most slots (65,536, its 16-bit slots) at startup where the device binds arrays part full, so no pipeline is made again mid-play | A bindless descriptor heap sized at startup (D3D12 and Vulkan descriptor indexing) |
| Light blockers to mark | `blockers.rs`, 1 ms a frame | Time-sliced work queue |
| Physics after a long frame | A frame simulates at most 0.1 s (24 steps of 240 Hz) and drops the rest | Roblox's own cap, measured: 1/15 s on the server, about 0.12 s on a client ([timestepping](../../behavior/physics/timestepping.md#long-frames)); Unity's Maximum Allowed Timestep |
| Particles | At most 1/15 s stepped and 16,000 drawn a frame | Roblox's own ([particle emitters](../../behavior/effects/particle-emitters.md)) |

**Guard.** `rook-player --bench` checks the stages bursts land in against their budgets once content has loaded (`crates/rook-client/src/bench/budgets.rs`: scene update, hand-over and GPU uploads, 6 ms each, a queue's 2 ms slice plus the stage's steady work on an 85k-part place) and prints a `FLAG budget:` line for a stage over its budget, with how many frames and the longest, beside the work counters' `FLAG` lines for a system that touched a quarter of a collection for a small change. A new burst source gets a queue and a row here; a stage that a bench flags is a bug in it, not noise to tune.

**Not bounded yet** (found 2026-11-04 on large places with many moving assemblies): a client's physics step, whose cost per 240 Hz step can reach 4 ms with many moving assemblies, so frames slow enough to need more steps stay slow (Roblox steps slow assemblies at 60 or 120 Hz instead, [adaptive timestepping](../../behavior/physics/timestepping.md)); filling all the light voxels at once when lighting first settles; and applying a large batch of the server's updates in one frame at join.
