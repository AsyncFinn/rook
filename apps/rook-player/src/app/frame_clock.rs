//! How far each frame moves the game on. At the display's rate frames
//! show exactly one refresh (or a few) apart, but the time measured
//! between them wobbles by milliseconds; stepping by that wobble moves
//! things unevenly between evenly shown frames, a judder most visible as
//! the camera turns. So, as is standard for vsynced games (Godot's delta
//! smoothing, for one), each frame steps by a whole number of refreshes,
//! at least one, and what that leaves over or under the clock is carried
//! until it adds up to a refresh, so game time never drifts from real
//! time.

/// How far stepped time may run ahead of the time measured, in
/// refreshes, before frames count as not waiting for the display. A
/// frame begins as the one before is handed over, and with one frame
/// queued (`Gfx::open`) the wait for the screen can fall in either, so a
/// frame shown a refresh after the last can measure anywhere from nothing
/// to two refreshes: after a late frame the next is early by as much.
const MAX_LEAD: f64 = 2.0;

#[derive(Debug, Default)]
pub(crate) struct FrameClock {
    /// The display's refresh period in seconds, while frames are shown at
    /// its rate.
    refresh: Option<f64>,
    /// Measured time not yet stepped (negative: stepped ahead of it).
    carry: f64,
}

impl FrameClock {
    /// Frames are shown at a display's rate of `hz`, or (`None`) whenever
    /// they're ready.
    pub(crate) fn set_refresh(&mut self, hz: Option<f64>) {
        let refresh = hz.filter(|&hz| hz > 0.0).map(|hz| 1.0 / hz);
        if refresh != self.refresh {
            self.refresh = refresh;
            self.carry = 0.0;
        }
    }

    /// The step for a frame `measured` seconds after the last.
    pub(crate) fn step(&mut self, measured: f64) -> f64 {
        let Some(refresh) = self.refresh else {
            return measured;
        };
        let due = measured + self.carry;
        // No frame is shown sooner than a refresh after the last, so one
        // measured early still steps a whole refresh.
        let refreshes = (due / refresh).round().max(1.0);
        let carry = due - refreshes * refresh;
        // Further ahead, frames aren't waiting for the display after all
        // (it isn't vsynced, or its rate was misread).
        if carry < -MAX_LEAD * refresh {
            self.carry = 0.0;
            return measured;
        }
        self.carry = carry;
        refreshes * refresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames wobbling around a 60 Hz refresh step exactly one refresh
    /// each, and over many frames the steps add up to the time measured.
    #[test]
    fn wobbling_frames_step_whole_refreshes() {
        let mut clock = FrameClock::default();
        clock.set_refresh(Some(60.0));
        let refresh = 1.0 / 60.0;
        let measured = [0.0160, 0.0173, 0.0196, 0.0137, 0.0167, 0.0166];
        let steps: Vec<f64> = measured.iter().map(|&m| clock.step(m)).collect();
        assert!(
            steps.iter().all(|&s| (s - refresh).abs() < 1e-12),
            "{steps:?}"
        );
        let off = measured.iter().sum::<f64>() - steps.iter().sum::<f64>();
        assert!(off.abs() < refresh * 0.5, "{off}");
        // A frame that missed a refresh steps two.
        assert!((clock.step(0.0334) - 2.0 * refresh).abs() < 1e-12);
        // Uncapped, frames step as measured.
        clock.set_refresh(None);
        assert_eq!(clock.step(0.0042), 0.0042);
    }

    /// A late frame (25 ms at 60 Hz, shown two refreshes on) and the
    /// early one after it (8 ms, shown one refresh on) step two refreshes
    /// and one. The early one stepped its 8 ms, a quarter of how far the
    /// world moves in a shown frame, which looked like a jerk backward.
    #[test]
    fn the_frame_after_a_late_one_steps_a_refresh() {
        let mut clock = FrameClock::default();
        clock.set_refresh(Some(60.0));
        let refresh = 1.0 / 60.0;
        let steps: Vec<f64> = [0.0167, 0.025, 0.008, 0.0167, 0.0167]
            .iter()
            .map(|&m| clock.step(m))
            .collect();
        let refreshes: Vec<f64> = steps.iter().map(|s| s / refresh).collect();
        let want = [1.0, 2.0, 1.0, 1.0, 1.0];
        assert!(
            refreshes.iter().zip(want).all(|(r, w)| (r - w).abs() < 1e-9),
            "{refreshes:?}"
        );
        // Frames that never wait for the display step as measured once
        // they run more than two refreshes ahead.
        let fast: Vec<f64> = (0..8).map(|_| clock.step(0.004)).collect();
        assert!(fast.contains(&0.004), "{fast:?}");
    }
}
