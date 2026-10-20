//! Frame and step times, summarised the way players feel them: the
//! typical time, the slow tail and the worst hitch, not the average.

use std::fmt;
use std::time::Duration;

/// A 60 Hz frame's budget, rounded up as the readout states it, so a
/// frame paced by a 60 Hz display isn't counted for timing jitter alone.
pub(crate) const BUDGET: Duration = Duration::from_micros(16_700);

/// Durations collected over a run or a window of time.
#[derive(Debug, Default, Clone)]
pub(crate) struct Times {
    samples: Vec<Duration>,
}

impl Times {
    pub(crate) fn push(&mut self, d: Duration) {
        self.samples.push(d);
    }

    pub(crate) fn clear(&mut self) {
        self.samples.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.samples.len()
    }

    /// The samples from index `start` on.
    pub(crate) fn since(&self, start: usize) -> Times {
        Times {
            samples: self.samples.get(start..).unwrap_or_default().to_vec(),
        }
    }

    /// The samples before index `end`.
    pub(crate) fn until(&self, end: usize) -> Times {
        Times {
            samples: self.samples[..end.min(self.samples.len())].to_vec(),
        }
    }

    /// Sample by sample, these times plus `other`'s (a lockstep step's
    /// server and client parts); samples only one side has count alone.
    pub(crate) fn sum_with(&self, other: &Times) -> Times {
        let (long, short) = if self.samples.len() >= other.samples.len() {
            (self, other)
        } else {
            (other, self)
        };
        let mut samples = long.samples.clone();
        for (s, o) in samples.iter_mut().zip(&short.samples) {
            *s += *o;
        }
        Times { samples }
    }

    pub(crate) fn summary(&self) -> Summary {
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        let count = sorted.len();
        if count == 0 {
            return Summary::default();
        }
        // The slowest 1% (at least one sample), averaged: the "1% low" of
        // game benchmarks, which a few hitches move and averages hide.
        let tail = count.div_ceil(100);
        let slowest = &sorted[count - tail..];
        Summary {
            count,
            total: sorted.iter().sum(),
            median: sorted[count / 2],
            slowest_1pct: slowest.iter().sum::<Duration>() / tail as u32,
            longest: sorted[count - 1],
            over_budget: sorted.iter().filter(|d| **d > BUDGET).count(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Summary {
    pub count: usize,
    pub total: Duration,
    pub median: Duration,
    /// The mean of the slowest 1%.
    pub slowest_1pct: Duration,
    pub longest: Duration,
    /// How many took longer than a 60 Hz frame.
    pub over_budget: usize,
}

impl Summary {
    /// Frames per second, if the samples are back-to-back frames.
    pub(crate) fn fps(&self) -> f64 {
        if self.total.is_zero() {
            return 0.0;
        }
        self.count as f64 / self.total.as_secs_f64()
    }
}

pub(crate) fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "median {:.2} ms, slowest 1% {:.2} ms, longest {:.2} ms, {} of {} over 16.7 ms",
            ms(self.median),
            ms(self.slowest_1pct),
            ms(self.longest),
            self.over_budget,
            self.count
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reports_the_tail() {
        let mut t = Times::default();
        for i in 1..=200u64 {
            t.push(Duration::from_millis(if i > 198 {
                20 + i - 198
            } else {
                2
            }));
        }
        let s = t.summary();
        assert_eq!(s.count, 200);
        assert_eq!(s.median, Duration::from_millis(2));
        // The slowest 1% of 200 is the slowest 2: 21 and 22 ms.
        assert_eq!(s.slowest_1pct, Duration::from_micros(21_500));
        assert_eq!(s.longest, Duration::from_millis(22));
        assert_eq!(s.over_budget, 2);
    }
}
