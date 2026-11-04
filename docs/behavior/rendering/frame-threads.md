# Frame threads

Checked against Studio 0.745 on 2026-11-04.

Which thread does what in a client frame, and what overlaps. Rook's frame
follows from this (docs/design/architecture.md#frame-order).

## What Roblox documents

The MicroProfiler guide (creator-docs, `performance-optimization/microprofiler`)
names three kinds of thread:

- **Main**: input, Humanoids, animation and tweens, sound, script resumes,
  and the CPU side of rendering's hand-off; it coordinates the others.
- **Workers**: networking, physics, pathfinding, in parallel.
- **Render**: "prepare, perform, present". Prepare takes what the main
  thread changed and updates the render data (`updateInvalidParts`,
  `updateInstancedClusters`, `updateDynamicParts`); Perform culls
  (`queryFrustumOrdered`), lights near the camera
  (`computeLightingPerform`), fills GUI vertices and issues the draws;
  Present waits on the GPU.

Its frame colors compare a **Jobs Wall Time** (scripts, physics,
animation on the main thread and workers) with a **Render Wall Time**:
the frame is as long as the slower of the two, not their sum.

## Measured

A client in Play mode with 48,000 anchored parts in view (render CPU,
`Stats.RenderCPUFrameTime`, about 8.5 ms a frame) and a Heartbeat
connection that busy-waits a set time on the client, so the main thread
carries a known load. Frame times are `RenderStepped` intervals; the
viewport is capped at 60 fps.

| Script load | Frame (median) | Render CPU |
|---|---|---|
| 0 ms | 16.6 ms | 8.5 ms |
| 6 ms | 16.6 ms | 8.8 ms |
| 10 ms | 16.6 ms | 8.9 ms |

At 10 ms the load and the render CPU sum to about 19 ms, more than a
frame, and frames still come every 16.6 ms.

With the window in the background Studio draws only every fourth frame
(15 fps) while Heartbeat keeps 60 Hz, which splits frames that render
from frames that don't. With 12 ms of load and about 10 ms of render CPU:

| | Frames that render | Frames that don't |
|---|---|---|
| Heartbeat to Heartbeat | 16.6 ms | 16.7 ms |
| Idle main thread after the load | 4.2 ms | 4.4 ms |

If the render CPU ran on the main thread, the frames that render would
lose about 10 ms of idle time or stretch to about 26 ms. They lose
nothing: the render CPU runs on another thread, at the same time as the
main thread's next step. In each rendered frame `RenderStepped` fires
just after the previous frame's Heartbeat ends, then the frame's
Stepped and Heartbeat follow.

Conclusion: Roblox renders a frame on its render thread while the main
thread steps the next one, so a client frame costs about its slower half.
The MicroProfiler's own timeline wasn't dumped (it is shown and dumped
only from Studio's UI); the overlap above is measured through the
scheduler's events instead.

Not measured:

- Whether Prepare runs on the render thread or at a hand-off on the main
  thread (the guide lists it under the render thread).
- How many frames the render thread may run behind the main thread.
- The live client (RobloxPlayerBeta), which may differ from Studio.

## Rook's frames, measured

Rook's player on a 6-core (12-thread) machine, in its window at
1280x720 on a 2018-era place with moving machinery, the build that ran
every frame one stage after another against the pipelined one, back to
back:

| | serial | pipelined, always overlapped |
|---|---|---|
| Render thread's job per frame | 5-8 ms | 13-17 ms |
| Input to present at a 60 fps cap | 16 ms | 24-25 ms |
| Uncapped frame rate | 64 fps | 46-59 fps |

Overlapped, the render ran twice as long: the step beside it took the
cores and memory bandwidth it needed (the effects stage alone went from
3.5 to 7 ms). So overlap is not free. Under a cap that serial frames keep
to, it buys nothing and adds latency, and on a loaded machine it can lose
outright. Choosing per frame from measured costs (`render_thread::Pacer`),
the same place ran:

| | serial | chosen per frame |
|---|---|---|
| Input to present at a 60 fps cap, serial keeping to it | 16 ms | 16 ms |
| Frame rate at a 60 fps cap, machine loaded | 49-56 fps | 58-60 fps |
| Uncapped frame rate | 59-64 fps | 92-124 fps |

Offscreen (`--bench`), the median frame: that place 26.6 to 13.7 ms, a
mountain place of about 300,000 parts 14.3 to 8.6 ms, the Baseplate 1.86
to 1.60 ms.
