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

/// Time each stage of a frame, offscreen.
pub(crate) fn bench(
    game: &mut Game,
    mut cache: SceneCache,
    frames: u32,
    camera: &Camera,
    width: u32,
    height: u32,
    install: &Installed,
    load_start: Instant,
) -> anyhow::Result<()> {
    let mut renderer = Renderer::headless()?;
    install.apply(&mut renderer);
    renderer.set_scene(&cache.scene);
    let timing_gpu = renderer.time_gpu();
    // The renderer's CPU recording, render with readback, and the GPU's
    // shadow and main passes: medians, as the GPU is shared with the
    // desktop and swings from run to run.
    let mut render_times: [stats::Times; 4] = Default::default();
    let (mut sim, mut build, mut render) = (0.0, 0.0, 0.0);
    let mut updates = 0;
    let secs = |t: Instant| t.elapsed().as_secs_f64();
    game.set_viewport(width, height);
    let mut mouse = 0.0;
    let mut whole = stats::Times::default();
    // When the content loaders last ran out of work, and the frames
    // until then.
    let mut idle_at = None;
    // Per pass (shadow, main): draw calls, instanced draws, triangles.
    let mut drawn = [[0u64; 3]; 2];
    game.server_steps.clear();
    game.client_steps.clear();
    for frame in 0..frames {
        let frame_start = Instant::now();
        // A moving cursor, as when someone plays: each move casts the mouse ray.
        let t = Instant::now();
        let x = (frame % 100) as f32 / 100.0 * width as f32;
        game.input(InputEvent::MouseMove {
            x,
            y: height as f32 / 2.0,
        });
        mouse += secs(t);
        let t = Instant::now();
        game.step();
        sim += secs(t);
        let t = Instant::now();
        renderer.set_time(frame as f32 / 60.0);
        if refresh(&mut cache, game.view_mut(), &mut renderer, camera.position) {
            updates += 1;
        }
        build += secs(t);
        let t = Instant::now();
        renderer.render_to_image(width, height, camera)?;
        render += secs(t);
        render_times[0].push(renderer.record_time());
        render_times[1].push(t.elapsed());
        if let Some(gpu) = renderer.gpu_times() {
            render_times[2].push(Duration::from_secs_f64(gpu.shadow / 1000.0));
            render_times[3].push(Duration::from_secs_f64(gpu.main / 1000.0));
        }
        let counts = renderer.draw_counts();
        for (sum, pass) in drawn.iter_mut().zip([counts.shadow, counts.main]) {
            sum[0] += u64::from(pass.calls);
            sum[1] += u64::from(pass.draws);
            sum[2] += pass.triangles;
        }
        whole.push(frame_start.elapsed());
        let idle = rook_content::idle_since();
        if idle != idle_at.map(|(t, _)| t) {
            idle_at = idle.map(|t| (t, whole.len()));
        }
        profiling::finish_frame!();
    }
    game.print_logs();
    let ms = |total: f64| total / f64::from(frames) * 1000.0;
    eprintln!(
        "{frames} frames, {} entries, {} mesh files, {} images, scene updated on {updates} frames",
        cache.scene.parts.len(),
        cache.scene.meshes.len(),
        cache.scene.images.len()
    );
    eprintln!(
        "per frame: mouse {:.2} ms, simulate {:.2} ms, scene update and upload {:.2} ms, render+readback {:.2} ms",
        ms(mouse),
        ms(sim),
        ms(build),
        ms(render)
    );
    let per_frame = |n: u64| thousands((n / u64::from(frames.max(1))) as usize);
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
    let [record, with_readback, gpu_shadow, gpu_main] = render_times
        .each_ref()
        .map(|t| stats::ms(t.summary().median));
    eprintln!(
        "render medians: CPU recording {record:.2} ms (mean {:.2} ms), render+readback {with_readback:.2} ms",
        stats::ms(render_times[0].summary().total) / f64::from(frames.max(1))
    );
    if timing_gpu {
        eprintln!("render medians: GPU shadow pass {gpu_shadow:.2} ms, main pass {gpu_main:.2} ms");
    }
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
        whole.summary()
    );
    match idle_at {
        Some((at, frames)) => eprintln!(
            "content loaded {:.2} s after the place started loading; frames until then: {}",
            at.saturating_duration_since(load_start).as_secs_f64(),
            whole.until(frames).summary()
        ),
        None => eprintln!(
            "content still loading after {frames} frames, {:.2} s after the place started loading",
            load_start.elapsed().as_secs_f64()
        ),
    }
    Ok(())
}
