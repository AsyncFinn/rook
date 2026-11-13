//! `--bench`: timing each stage of a frame, offscreen, run as live play
//! runs it: the server steps by itself on its thread, the client steps
//! once a frame by the time it took, and the renderer draws each frame on
//! the render thread while the next one steps
//! (docs/design/architecture.md#frame-order).

use std::time::{Duration, Instant};

use glam::{Mat3, Vec3};
use rook_engine::InputEvent;
use rook_render::{Camera, Renderer};
use rook_scene::{Installed, SceneCache};

use crate::game::Game;
use crate::render_thread::RenderThread;
use crate::start::Start;
use crate::stats::{self, thousands};

mod budgets;
mod counters;

/// How `--bench` moves the camera, as someone playing does.
#[derive(Debug, Clone, Copy, Default)]
pub struct Bench {
    /// Degrees the camera turns each frame.
    pub turn: f32,
    /// Studs the camera flies each frame the way it looks, level.
    pub fly: f32,
}

/// The CPU recording's stages, as `StageTimes` orders them.
const STAGES: [&str; 7] = [
    "uploads",
    "setup",
    "instances",
    "effects",
    "draws",
    "lights",
    "passes",
];

/// What the render thread holds for `--bench`: the renderer, and what
/// each frame it drew measured.
struct Offscreen {
    renderer: Renderer,
    width: u32,
    height: u32,
    drawn: Vec<Rendered>,
    /// Reading a frame back failed.
    failed: Option<rook_render::RenderError>,
}

/// A frame on the render thread.
#[derive(Default, Clone, Copy)]
struct Rendered {
    /// The renderer's CPU recording, and the whole job with readback.
    record: Duration,
    with_readback: Duration,
    stages: [Duration; 7],
    /// The GPU's shadow and main passes, in ms.
    gpu: Option<(f64, f64)>,
    /// Per pass (shadow, main): draw calls, instanced draws, triangles.
    drawn: [[u64; 3]; 2],
}

/// A frame on the main thread.
#[derive(Default, Clone, Copy)]
struct Stepped {
    mouse: Duration,
    /// The client's step, applying what the server sent included.
    step: Duration,
    /// The scene following the step (`SceneCache::update`).
    scene: Duration,
    /// Waiting for the render thread to finish the frame before.
    wait: Duration,
    /// Handing the scene's changes to the renderer (`ToRender::apply`).
    hand_over: Duration,
}

impl Offscreen {
    fn draw(&mut self, camera: &Camera, clock: f32) {
        let start = Instant::now();
        self.renderer.set_time(clock);
        if let Err(e) = self
            .renderer
            .render_to_image(self.width, self.height, camera)
        {
            self.failed.get_or_insert(e);
        }
        let s = self.renderer.stage_times();
        let counts = self.renderer.draw_counts();
        self.drawn.push(Rendered {
            record: self.renderer.record_time(),
            with_readback: start.elapsed(),
            stages: [
                s.uploads,
                s.setup,
                s.instances,
                s.effects,
                s.draws,
                s.lights,
                s.passes,
            ],
            gpu: self.renderer.gpu_times().map(|g| (g.shadow, g.main)),
            drawn: [counts.shadow, counts.main]
                .map(|p| [u64::from(p.calls), u64::from(p.draws), p.triangles]),
        });
    }
}

pub fn bench(
    game: &mut Game,
    mut cache: SceneCache,
    frames: u32,
    mut camera: Camera,
    start: &Start,
    how: Bench,
    install: &Installed,
) -> anyhow::Result<()> {
    let (width, height) = (start.width, start.height);
    let turn = Mat3::from_rotation_y(how.turn.to_radians());
    // The scene was just built, asking for its files: loading is timed
    // from here, not from the place's start, which `--seconds` delays it by.
    let scene_built = Instant::now();
    let mut renderer = Renderer::headless()?;
    install.apply(&mut renderer);
    cache.show_all(&mut renderer);
    let timing_gpu = renderer.time_gpu();
    let mut thread = RenderThread::new(Offscreen {
        renderer,
        width,
        height,
        drawn: Vec::with_capacity(frames as usize),
        failed: None,
    });
    game.set_viewport(width, height);
    game.server_steps.clear();
    game.client_steps.clear();
    // As in live play: the server keeps its own 60 Hz pace.
    game.free_run();
    let mut stepped = Vec::with_capacity(frames as usize);
    let mut starts = Vec::with_capacity(frames as usize + 1);
    let mut idle_at: Option<(Instant, usize)> = None;
    let mut updates = 0;
    let mut clock = 0.0;
    let mut last = Instant::now();
    let mut counted = counters::Frames::start();
    for frame in 0..frames {
        let frame_start = Instant::now();
        starts.push(frame_start);
        let dt = (frame_start - last).as_secs_f64().min(0.1);
        last = frame_start;
        clock += dt;
        let mut s = Stepped::default();
        // A moving cursor, as when someone plays: each move casts the m.mouse ray.
        let t = Instant::now();
        let x = (frame % 100) as f32 / 100.0 * width as f32;
        game.input(InputEvent::MouseMove {
            x,
            y: height as f32 / 2.0,
        });
        s.mouse = t.elapsed();
        let t = Instant::now();
        game.advance(dt);
        s.step = t.elapsed();
        camera.rotation = turn * camera.rotation;
        let ahead = (camera.rotation * -Vec3::Z) * Vec3::new(1.0, 0.0, 1.0);
        camera.position += ahead.normalize_or_zero() * how.fly;
        let t = Instant::now();
        let update = cache.update(game.view_mut(), camera.position);
        if update.changed() {
            updates += 1;
        }
        s.scene = t.elapsed();
        let t = Instant::now();
        let offscreen = thread.hand_over(frame_start, None);
        s.wait = t.elapsed();
        let t = Instant::now();
        update.apply(&cache, &mut offscreen.renderer);
        s.hand_over = t.elapsed();
        let drawn_clock = (clock % 3600.0) as f32;
        thread.send(move |o| {
            o.draw(&camera, drawn_clock);
            Duration::ZERO
        });
        stepped.push(s);
        let idle = rook_content::idle_since();
        if idle != idle_at.map(|(t, _)| t) {
            idle_at = idle.map(|t| (t, stepped.len()));
        }
        counted.frame();
        profiling::finish_frame!();
    }
    let offscreen = thread.get();
    starts.push(Instant::now());
    if let Some(e) = offscreen.failed.take() {
        return Err(e.into());
    }
    game.print_logs();
    let m = Measured {
        stepped,
        rendered: std::mem::take(&mut offscreen.drawn),
        starts,
        idle_at,
        updates,
    };
    m.report(game, &offscreen.renderer, &cache, scene_built, timing_gpu);
    counted.report();
    Ok(())
}

/// What the frames measured.
struct Measured {
    stepped: Vec<Stepped>,
    rendered: Vec<Rendered>,
    /// When each frame started, and when the last one finished.
    starts: Vec<Instant>,
    /// When the content loaders last ran out of work, and the frames
    /// until then.
    idle_at: Option<(Instant, usize)>,
    /// Frames the scene changed on.
    updates: u32,
}

impl Measured {
    fn times<T>(items: &[T], f: impl Fn(&T) -> Duration) -> stats::Times {
        let mut t = stats::Times::default();
        for item in items {
            t.push(f(item));
        }
        t
    }

    /// Each frame's time, start to start: what a player waits per frame.
    fn whole(&self) -> stats::Times {
        let mut t = stats::Times::default();
        for pair in self.starts.windows(2) {
            t.push(pair[1] - pair[0]);
        }
        t
    }

    fn report(
        &self,
        game: &Game,
        renderer: &Renderer,
        cache: &SceneCache,
        scene_built: Instant,
        timing_gpu: bool,
    ) {
        let frames = self.stepped.len();
        let mean = |t: &stats::Times| stats::ms(t.summary().total) / frames.max(1) as f64;
        let median = |t: &stats::Times| stats::ms(t.summary().median);
        eprintln!(
            "images on the GPU: {:.1} MB",
            renderer.image_bytes() as f64 / f64::from(1 << 20)
        );
        eprintln!(
            "{frames} frames, {} entries, {} mesh files, {} images, scene updated on {} frames",
            cache.scene.parts.len(),
            cache.scene.meshes.len(),
            cache.images_loaded(),
            self.updates
        );
        let mouse = Self::times(&self.stepped, |s| s.mouse);
        let step = Self::times(&self.stepped, |s| s.step);
        let scene = Self::times(&self.stepped, |s| s.scene);
        let wait = Self::times(&self.stepped, |s| s.wait);
        let hand_over = Self::times(&self.stepped, |s| s.hand_over);
        let render = Self::times(&self.rendered, |r| r.with_readback);
        let record = Self::times(&self.rendered, |r| r.record);
        eprintln!(
            "main thread per frame: mouse {:.2} ms, client step {:.2} ms, scene update {:.2} ms, \
             waiting for the render thread {:.2} ms, hand-over {:.2} ms",
            mean(&mouse),
            mean(&step),
            mean(&scene),
            mean(&wait),
            mean(&hand_over)
        );
        eprintln!(
            "render thread per frame: render+readback {:.2} ms",
            mean(&render)
        );
        let per_frame = |n: u64| thousands((n / frames.max(1) as u64) as usize);
        let drawn = self.rendered.iter().fold([[0u64; 3]; 2], |mut sum, r| {
            for (s, d) in sum.iter_mut().zip(r.drawn) {
                for (a, b) in s.iter_mut().zip(d) {
                    *a += b;
                }
            }
            sum
        });
        eprintln!(
            "draw calls per frame: shadow pass {} ({} draws), main pass {} ({} draws)",
            per_frame(drawn[0][0]),
            per_frame(drawn[0][1]),
            per_frame(drawn[1][0]),
            per_frame(drawn[1][1])
        );
        eprintln!(
            "triangles per frame: shadow pass {}, main pass {}",
            per_frame(drawn[0][2]),
            per_frame(drawn[1][2])
        );
        eprintln!(
            "render medians: CPU recording {:.2} ms (mean {:.2} ms), render+readback {:.2} ms",
            median(&record),
            mean(&record),
            median(&render)
        );
        let stages: Vec<stats::Times> = (0..STAGES.len())
            .map(|i| Self::times(&self.rendered, |r| r.stages[i]))
            .collect();
        let line: Vec<String> = STAGES
            .iter()
            .zip(&stages)
            .map(|(name, t)| format!("{name} {:.2} ms", median(t)))
            .collect();
        eprintln!("CPU recording medians by stage: {}", line.join(", "));
        // The light voxels fill only when something moves on, so their
        // median hides what turning the camera costs.
        let fills = stages[5].summary();
        eprintln!(
            "light voxels: mean {:.2} ms, {fills}",
            stats::ms(fills.total) / frames.max(1) as f64
        );
        if timing_gpu {
            let shadow = Self::times(&self.rendered, |r| {
                Duration::from_secs_f64(r.gpu.map_or(0.0, |g| g.0) / 1000.0)
            });
            let main = Self::times(&self.rendered, |r| {
                Duration::from_secs_f64(r.gpu.map_or(0.0, |g| g.1) / 1000.0)
            });
            eprintln!(
                "render medians: GPU shadow pass {:.2} ms, main pass {:.2} ms",
                median(&shadow),
                median(&main)
            );
        }
        for (name, times) in STAGES.iter().zip(&stages) {
            eprintln!("CPU recording, {name}: {}", times.summary());
        }
        eprintln!("mouse: {}", mouse.summary());
        eprintln!("client step: {}", step.summary());
        eprintln!("scene update: {}", scene.summary());
        eprintln!("waiting for the render thread: {}", wait.summary());
        eprintln!("hand-over: {}", hand_over.summary());
        eprintln!("render+readback (render thread): {}", render.summary());
        eprintln!(
            "server step (its own thread): {}",
            game.server_steps.summary()
        );
        let whole = self.whole();
        eprintln!(
            "whole frame (pipelined, with readback): {}",
            whole.summary()
        );
        match self.idle_at {
            Some((at, idle)) => {
                eprintln!(
                    "content loaded {:.2} s after the scene was built; frames until then: {}",
                    at.saturating_duration_since(scene_built).as_secs_f64(),
                    whole.until(idle).summary()
                );
                self.report_slowest(idle);
                for (budget, times) in [
                    (budgets::SCENE_UPDATE, &scene),
                    (budgets::HAND_OVER, &hand_over),
                    (budgets::UPLOADS, &stages[0]),
                ] {
                    eprintln!("{}", budget.check(&times.since(idle)));
                }
            }
            None => eprintln!(
                "content still loading after {frames} frames, {:.2} s after the scene was built",
                scene_built.elapsed().as_secs_f64()
            ),
        }
    }

    /// The slowest frame from `from` on (once content has loaded), by
    /// stage: the main thread's part, and the render thread's frame it
    /// waited for, which drew while it stepped.
    fn report_slowest(&self, from: usize) {
        let whole = self.whole();
        let Some((i, took)) = (from..self.stepped.len())
            .map(|i| (i, self.starts[i + 1] - self.starts[i]))
            .max_by_key(|(_, took)| *took)
        else {
            eprintln!("slowest frame after load: none (content loaded on the last frame)");
            return;
        };
        let s = self.stepped[i];
        let ms = stats::ms;
        eprintln!(
            "slowest frame after load: frame {i} of {}, {:.2} ms (median {:.2} ms): \
             mouse {:.2}, client step {:.2}, scene update {:.2}, waiting for the render thread {:.2}, hand-over {:.2}",
            self.stepped.len(),
            ms(took),
            ms(whole.since(from).summary().median),
            ms(s.mouse),
            ms(s.step),
            ms(s.scene),
            ms(s.wait),
            ms(s.hand_over),
        );
        // The frame before drew on the render thread while this one stepped.
        if let Some(r) = i.checked_sub(1).and_then(|p| self.rendered.get(p)) {
            let line: Vec<String> = STAGES
                .iter()
                .zip(r.stages)
                .map(|(name, t)| format!("{name} {:.2}", ms(t)))
                .collect();
            eprintln!(
                "  the render thread meanwhile (frame {}): render+readback {:.2} ms, CPU recording {:.2} ms ({})",
                i - 1,
                ms(r.with_readback),
                ms(r.record),
                line.join(", ")
            );
        }
    }
}
