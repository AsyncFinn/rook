//! How far each frame moves the game on. At the display's rate frames
//! show exactly one refresh (or a few) apart, but the time measured
//! between them wobbles by milliseconds; stepping by that wobble moves
//! things unevenly between evenly shown frames, a judder most visible as
//! the camera turns. So, as is standard for vsynced games, each frame
//! steps by a whole number of refreshes, and what that leaves over or
//! under the clock is carried until it adds up to a refresh, so game time
//! never drifts from real time.

/// Frames measured at most this far from a whole number of refreshes
/// (beyond the carry) still step by it.
const MAX_CARRY: f64 = 0.5;

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
        let refreshes = (due / refresh).round().max(1.0);
        // A frame far from any number of refreshes (a hitch, a stall)
        // steps as measured: carrying it would push later frames off.
        if (due - refreshes * refresh).abs() > MAX_CARRY * refresh * refreshes {
            self.carry = 0.0;
            return measured;
        }
        self.carry = due - refreshes * refresh;
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
}
