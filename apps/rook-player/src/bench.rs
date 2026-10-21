//! `--bench`: timing each stage of a frame, offscreen.

use crate::*;

/// `29436` → `29,436`.
pub(crate) fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What the frames measured.
#[derive(Default)]
struct Measured {
    /// The renderer's CPU recording, render with readback, and the GPU's
    /// shadow and main passes: medians, as the GPU is shared with the
    /// desktop and swings from run to run.
    render_times: [stats::Times; 4],
    /// Seconds in all, for the per-frame means.
    sim: f64,
    build: f64,
    render: f64,
    mouse: f64,
    /// Frames the scene changed on.
    updates: u32,
    mouse_times: stats::Times,
    update_times: stats::Times,
    /// The renderer's CPU recording by stage (`StageTimes`, in order).
    stage_times: [stats::Times; 6],
    whole: stats::Times,
    /// When the content loaders last ran out of work, and the frames
    /// until then.
    idle_at: Option<(Instant, usize)>,
    /// Per pass (shadow, main): draw calls, instanced draws, triangles.
    drawn: [[u64; 3]; 2],
}

/// Time each stage of a frame, offscreen.
pub(crate) fn bench(
    game: &mut Game,
    mut cache: SceneCache,
    frames: u32,
    mut camera: Camera,
    args: &Args,
    install: &Installed,
) -> anyhow::Result<()> {
    let (width, height) = (args.width, args.height);
    let turn = Mat3::from_rotation_y(args.turn.to_radians());
    let load_start = game.load_start;
    let mut renderer = Renderer::headless()?;
    install.apply(&mut renderer);
    renderer.set_scene(&cache.scene);
    let timing_gpu = renderer.time_gpu();
    let mut m = Measured::default();
    let secs = |t: Instant| t.elapsed().as_secs_f64();
    game.set_viewport(width, height);
    game.server_steps.clear();
    game.client_steps.clear();
    for frame in 0..frames {
        let frame_start = Instant::now();
        // A moving cursor, as when someone plays: each move casts the m.mouse ray.
        let t = Instant::now();
        let x = (frame % 100) as f32 / 100.0 * width as f32;
        game.input(InputEvent::MouseMove {
            x,
            y: height as f32 / 2.0,
        });
        m.mouse += secs(t);
        m.mouse_times.push(t.elapsed());
        let t = Instant::now();
        game.step();
        m.sim += secs(t);
        camera.rotation = turn * camera.rotation;
        let t = Instant::now();
        renderer.set_time(frame as f32 / 60.0);
        if cache.refresh(game.view_mut(), &mut renderer, camera.position) {
            m.updates += 1;
        }
        m.build += secs(t);
        m.update_times.push(t.elapsed());
        let t = Instant::now();
        renderer.render_to_image(width, height, &camera)?;
        m.render += secs(t);
        m.render_times[0].push(renderer.record_time());
        m.render_times[1].push(t.elapsed());
        let s = renderer.stage_times();
        let stages = [
            s.uploads,
            s.setup,
            s.instances,
            s.effects,
            s.draws,
            s.passes,
        ];
        for (times, stage) in m.stage_times.iter_mut().zip(stages) {
            times.push(stage);
        }
        if let Some(gpu) = renderer.gpu_times() {
            m.render_times[2].push(Duration::from_secs_f64(gpu.shadow / 1000.0));
            m.render_times[3].push(Duration::from_secs_f64(gpu.main / 1000.0));
        }
        let counts = renderer.draw_counts();
        for (sum, pass) in m.drawn.iter_mut().zip([counts.shadow, counts.main]) {
            sum[0] += u64::from(pass.calls);
            sum[1] += u64::from(pass.draws);
            sum[2] += pass.triangles;
        }
        m.whole.push(frame_start.elapsed());
        let idle = rook_content::idle_since();
        if idle != m.idle_at.map(|(t, _)| t) {
            m.idle_at = idle.map(|t| (t, m.whole.len()));
        }
        profiling::finish_frame!();
    }
    game.print_logs();
    m.report(frames, game, &renderer, &cache, load_start, timing_gpu);
    Ok(())
}

impl Measured {
    fn report(
        &self,
        frames: u32,
        game: &Game,
        renderer: &Renderer,
        cache: &SceneCache,
        load_start: Instant,
        timing_gpu: bool,
    ) {
        let ms = |total: f64| total / f64::from(frames) * 1000.0;
        eprintln!(
            "images on the GPU: {:.1} MB",
            renderer.image_bytes() as f64 / f64::from(1 << 20)
        );
        eprintln!(
            "{frames} frames, {} entries, {} mesh files, {} images, scene updated on {} frames",
            cache.scene.parts.len(),
            cache.scene.meshes.len(),
            cache.scene.images.len(),
            self.updates
        );
        eprintln!(
            "per frame: mouse {:.2} ms, simulate {:.2} ms, scene update and upload {:.2} ms, render+readback {:.2} ms",
            ms(self.mouse),
            ms(self.sim),
            ms(self.build),
            ms(self.render)
        );
        let per_frame = |n: u64| thousands((n / u64::from(frames.max(1))) as usize);
        eprintln!(
            "draw calls per frame: shadow pass {} ({} draws), main pass {} ({} draws)",
            per_frame(self.drawn[0][0]),
            per_frame(self.drawn[0][1]),
            per_frame(self.drawn[1][0]),
            per_frame(self.drawn[1][1])
        );
        eprintln!(
            "triangles per frame: shadow pass {}, main pass {}",
            per_frame(self.drawn[0][2]),
            per_frame(self.drawn[1][2])
        );
        let [record, with_readback, gpu_shadow, gpu_main] = self
            .render_times
            .each_ref()
            .map(|t| stats::ms(t.summary().median));
        eprintln!(
            "render medians: CPU recording {record:.2} ms (mean {:.2} ms), render+readback {with_readback:.2} ms",
            stats::ms(self.render_times[0].summary().total) / f64::from(frames.max(1))
        );
        let [uploads, setup, instances, effects, draws, passes] = self
            .stage_times
            .each_ref()
            .map(|t| stats::ms(t.summary().median));
        eprintln!(
            "CPU recording medians by stage: uploads {uploads:.2} ms, setup {setup:.2} ms, instances {instances:.2} ms, effects {effects:.2} ms, draws {draws:.2} ms, passes {passes:.2} ms"
        );
        if timing_gpu {
            eprintln!(
                "render medians: GPU shadow pass {gpu_shadow:.2} ms, main pass {gpu_main:.2} ms"
            );
        }
        eprintln!("mouse: {}", self.mouse_times.summary());
        eprintln!("scene update and upload: {}", self.update_times.summary());
        eprintln!("server step: {}", game.server_steps.summary());
        if !game.client_steps.is_empty() {
            eprintln!("client step: {}", game.client_steps.summary());
        }
        eprintln!(
            "simulation step (server + client): {}",
            game.total_steps().summary()
        );
        eprintln!(
            "whole frame (offscreen, with readback): {}",
            self.whole.summary()
        );
        match self.idle_at {
            Some((at, frames)) => eprintln!(
                "content loaded {:.2} s after the place started loading; frames until then: {}",
                at.saturating_duration_since(load_start).as_secs_f64(),
                self.whole.until(frames).summary()
            ),
            None => eprintln!(
                "content still loading after {frames} frames, {:.2} s after the place started loading",
                load_start.elapsed().as_secs_f64()
            ),
        }
    }
}
