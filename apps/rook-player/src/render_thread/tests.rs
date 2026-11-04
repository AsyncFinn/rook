use super::*;

/// A 144 Hz player whose mouse turns the camera a step each frame: the
/// render thread draws each frame with the camera its own step left, never
/// the step before, while the next frame steps.
#[test]
fn each_frame_draws_the_camera_its_own_step_left_at_144_hz() {
    let mut thread = RenderThread::new(Vec::<(u32, f32)>::new());
    let mut yaw = 0.0f32;
    let mut stepped = Vec::new();
    for frame in 0..144u32 {
        // The step: this frame's mouse movement turns the camera.
        yaw += 0.5;
        stepped.push((frame, yaw));
        // The hand-over, then the frame draws on the render thread.
        thread.get();
        let drawn = (frame, yaw);
        thread.send(move |log| log.push(drawn));
    }
    assert_eq!(*thread.get(), stepped);
}

/// A frame of the window's loop, in simulated time (ms): the main thread
/// takes `main` to step and update the scene, then hands over once the
/// render thread is done with the frame before, which takes `render`.
struct Pipeline {
    main: f64,
    render: f64,
}

/// Each frame's start (when it takes input) and when it finished drawing.
fn run(p: &Pipeline, frames: usize, just_in_time: bool) -> Vec<(f64, f64)> {
    let ms = |t: f64| Duration::from_secs_f64(t / 1000.0);
    let origin = Instant::now();
    let at = |t: f64| origin + ms(t);
    let since = |i: Instant| i.duration_since(origin).as_secs_f64() * 1000.0;
    let mut main_time = MainTime::default();
    let mut now = 0.0f64;
    // The frame in flight: when it was handed over and when it's done.
    let mut in_flight: Option<(f64, f64)> = None;
    let mut took = 0.0f64;
    let mut out = Vec::new();
    for _ in 0..frames {
        let due = in_flight.map(|(sent, _)| at(sent) + ms(took));
        let start = match start_at(due, main_time.get()).filter(|_| just_in_time) {
            Some(start) => now.max(since(start)),
            None => now,
        };
        let stepped = start + p.main;
        main_time.note(ms(p.main));
        let hand_over = in_flight.map_or(stepped, |(_, done)| stepped.max(done));
        let done = hand_over + p.render;
        took = p.render;
        in_flight = Some((hand_over, done));
        out.push((start, done));
        now = hand_over;
    }
    out
}

fn steady(frames: &[(f64, f64)]) -> (f64, f64) {
    let last = frames.len() - 1;
    let period = frames[last].1 - frames[last - 1].1;
    let latency = frames[last].1 - frames[last].0;
    (period, latency)
}

/// Drawn one after the other a frame costs `main + render`; pipelined it
/// costs the slower of the two, and starting just in time keeps each
/// frame's input exactly as old when it's shown as one after the other
/// would. Starting at once (`just_in_time` false) shows input a whole
/// render's wait older: the frame lag this pacing exists to prevent.
#[test]
fn pipelined_frames_cost_the_slower_half_with_no_added_latency() {
    let eps = 1e-3;
    // A 144 Hz budget is 6.94 ms. Render-bound, sim-bound and the
    // measured heavy place (client step and scene update 9.8 ms, render
    // 9 ms).
    for p in [
        Pipeline {
            main: 3.0,
            render: 5.5,
        },
        Pipeline {
            main: 5.5,
            render: 3.0,
        },
        Pipeline {
            main: 9.8,
            render: 9.0,
        },
    ] {
        let one_after_the_other = p.main + p.render;
        let (period, latency) = steady(&run(&p, 50, true));
        assert!((period - p.main.max(p.render)).abs() < eps, "{period}");
        assert!(latency <= one_after_the_other + eps, "{latency}");
        if p.render > p.main {
            let (_, eager) = steady(&run(&p, 50, false));
            assert!(eager > one_after_the_other + 1.0, "{eager}");
        }
    }
}
