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
        let started = Instant::now();
        // The step: this frame's mouse movement turns the camera.
        yaw += 0.5;
        stepped.push((frame, yaw));
        // The hand-over, then the frame draws on the render thread.
        thread.hand_over(started, None);
        let drawn = (frame, yaw);
        thread.send(move |log| {
            log.push(drawn);
            Duration::ZERO
        });
    }
    assert_eq!(*thread.get(), stepped);
}

/// A simulated frame (ms): when it took its input, when it was drawn, and
/// whether it ran serially.
#[derive(Debug, Clone, Copy)]
struct Frame {
    start: f64,
    done: f64,
    serial: bool,
}

/// The window's loop in simulated time (ms). Frame `i` takes `load(i)`:
/// its step on the main thread and its render alone; a render that
/// overlaps a step takes `contended` times as long. `period` is a frame
/// rate cap's. `paced: false` starts overlapped frames at once rather
/// than just in time.
fn run(
    load: impl Fn(usize) -> (f64, f64),
    contended: f64,
    period: Option<f64>,
    frames: usize,
    paced: bool,
) -> Vec<Frame> {
    let origin = Instant::now();
    let at = |t: f64| origin + Duration::from_secs_f64(t / 1000.0);
    let since = |i: Instant| i.duration_since(origin).as_secs_f64() * 1000.0;
    let span = |t: f64| Duration::from_secs_f64(t.max(0.0) / 1000.0);
    let mut pacer = Pacer::default();
    let mut now = 0.0f64;
    let mut last_start: Option<f64> = None;
    // The last hand-over, and whether that frame overlapped.
    let mut last_sent: Option<f64> = None;
    let mut overlapped = false;
    // The overlapped frame in flight: when it's done.
    let mut in_flight: Option<f64> = None;
    let mut out = Vec::new();
    for i in 0..frames {
        let (main, render) = load(i);
        let due = pacer.start_at(last_sent.map(at)).map(since);
        let mut start = now;
        if let (Some(period), Some(last)) = (period, last_start) {
            start = start.max(last + period);
        }
        if paced && let Some(due) = due.filter(|_| in_flight.is_some()) {
            start = start.max(due);
        }
        let held = due.map_or(0.0, |due| (start - due).max(0.0));
        let stepped = start + main;
        let sent = match in_flight.take() {
            Some(done) => {
                pacer.hand_over(span(done - stepped), span(stepped - done));
                stepped.max(done)
            }
            None => stepped,
        };
        if overlapped && let Some(last) = last_sent {
            pacer.finished(span(sent - last - held), period.map(span));
        }
        let serial = pacer.serial();
        let done = if serial {
            let done = sent + render;
            pacer.finished(span(main + render), period.map(span));
            now = done;
            done
        } else {
            let done = sent + render * contended;
            in_flight = Some(done);
            now = sent;
            done
        };
        last_sent = Some(sent);
        overlapped = !serial;
        last_start = Some(start);
        out.push(Frame {
            start,
            done,
            serial,
        });
    }
    out
}

/// Over the frames: the median time between frames and the median time
/// from a frame's input to its being drawn (medians, so an odd probe
/// frame doesn't count).
fn steady(frames: &[Frame]) -> (f64, f64) {
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    let periods = frames.windows(2).map(|w| w[1].done - w[0].done).collect();
    let latencies = frames.iter().map(|f| f.done - f.start).collect();
    (median(periods), median(latencies))
}

fn last_half(frames: &[Frame]) -> &[Frame] {
    &frames[frames.len() / 2..]
}

/// Uncapped, with the two sides not slowing each other, frames overlap.
/// One after the other a frame costs `main + render`; overlapped it costs
/// the slower of the two, and the just-in-time start keeps each frame's
/// input as old when it's shown as one after the other would. Starting
/// each frame at once (unpaced) shows input a render's wait older: the
/// frame lag the start exists to prevent.
#[test]
fn pipelined_frames_cost_the_slower_half_with_no_added_latency() {
    let eps = 0.05;
    // A 144 Hz budget is 6.94 ms. Render-bound, sim-bound, and a heavy
    // place measured in the window (main 9.8 ms, render 9 ms).
    for (main, render) in [(3.0, 5.5), (5.5, 3.0), (9.8, 9.0)] {
        let one_after_the_other = main + render;
        let frames = run(|_| (main, render), 1.0, None, 400, true);
        let serial = last_half(&frames).iter().filter(|f| f.serial).count();
        assert!(serial <= PROBE as usize, "{serial}");
        let (period, latency) = steady(last_half(&frames));
        assert!((period - f64::max(main, render)).abs() < eps, "{period}");
        assert!(latency <= one_after_the_other + eps, "{latency}");
        if render > main {
            let eager = run(|_| (main, render), 1.0, None, 400, false);
            let (_, eager) = steady(last_half(&eager));
            assert!(eager > one_after_the_other + 1.0, "{eager}");
        }
    }
}

/// With the render thread's time jittering from frame to frame (here 5.5
/// ms ± 2), latency stays within a small part of the jitter of one after
/// the other's, while frames still come much faster.
#[test]
fn render_jitter_costs_frame_rate_not_latency() {
    let main = 3.0;
    let load = |i: usize| (main, 5.5 + [2.0, -2.0, 1.0, -1.0, 0.0][i % 5]);
    let frames = run(load, 1.0, None, 400, true);
    let half = last_half(&frames);
    let (_, latency) = steady(half);
    // The mean: jittered frames take turns being long and short.
    let period = (half[half.len() - 1].done - half[0].done) / (half.len() - 1) as f64;
    let one_after_the_other = main + 5.5;
    assert!(latency <= one_after_the_other + 0.5, "{latency}");
    assert!(period < one_after_the_other - 1.5, "{period}");
}

/// Uncapped, when the render beside the step slows down so much that
/// overlapped frames would come no faster (the heavy place on a 6-core
/// machine), frames run one after the other: same rate, fresher input.
#[test]
fn frames_that_overlap_no_faster_run_one_after_the_other() {
    let (main, render) = (6.0, 5.0);
    let frames = run(|_| (main, render), 2.0, None, 2000, true);
    let half = last_half(&frames);
    let serial = half.iter().filter(|f| f.serial).count();
    // All but a probe now and then.
    assert!(serial >= half.len() - 2 * PROBE as usize, "{serial}");
    let (period, latency) = steady(half);
    assert!((period - (main + render)).abs() < 0.05, "{period}");
    assert!((latency - (main + render)).abs() < 0.05, "{latency}");
}

const CAP_60: f64 = 1000.0 / 60.0;

/// Under a 60 fps cap that frames one after the other keep to, they run
/// that way: overlapping would buy no frame rate, and its renders, slowed
/// by the step beside them (here twice as slow), would show older input.
#[test]
fn frames_that_keep_to_the_cap_run_one_after_the_other() {
    let (main, render) = (4.0, 7.0);
    let frames = run(|_| (main, render), 2.0, Some(CAP_60), 300, true);
    assert!(frames.iter().all(|f| f.serial));
    let (period, latency) = steady(last_half(&frames));
    assert!((period - CAP_60).abs() < 0.05, "{period}");
    assert!((latency - (main + render)).abs() < 0.05, "{latency}");
}

/// Frames that miss the cap one after the other overlap and keep to it,
/// probing serially now and then, less often as probes keep failing.
#[test]
fn frames_that_miss_the_cap_overlap_to_keep_to_it() {
    // The heavy place: 18.8 ms one after the other, 11.7 overlapped.
    let frames = run(|_| (9.8, 9.0), 1.3, Some(CAP_60), 1200, true);
    let half = last_half(&frames);
    let (period, _) = steady(half);
    assert!(period < CAP_60 + 0.3, "{period}");
    // Ten seconds at the end probe at most twice (8 s apart), two frames
    // each.
    let serial = half.iter().filter(|f| f.serial).count();
    assert!(serial <= 4, "{serial}");
}

/// When a heavy stretch passes, frames go back to running one after the
/// other within the longest wait between probes.
#[test]
fn frames_go_back_to_one_after_the_other_when_the_load_drops() {
    let heavy = 600;
    // Heavy: 19 ms serially, 15 overlapped (the render 2.5 times slower).
    // Light: 11 ms serially, 17.5 overlapped.
    let load = |i: usize| if i < heavy { (13.0, 6.0) } else { (4.0, 7.0) };
    let frames = run(load, 2.5, Some(CAP_60), 2400, true);
    assert!(frames[..heavy].iter().filter(|f| !f.serial).count() > heavy / 2);
    let back = (MAX_PROBE_WAIT.as_secs_f64() * 1000.0 / CAP_60) as usize + 2 * PROBE as usize;
    assert!(frames[heavy + back..].iter().all(|f| f.serial));
    let (_, latency) = steady(&frames[heavy + back..]);
    assert!((latency - 11.0).abs() < 0.05, "{latency}");
}
