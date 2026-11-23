//! Lighting's clock: the time of day as Roblox keeps it, and where that
//! puts the sun and the moon. Roblox keeps whole seconds after midnight;
//! `ClockTime`, `TimeOfDay` and `SetMinutesAfterMidnight` are three ways
//! in, each with its own rule for odd input, and all read the same seconds
//! back (Confirmed: conformance case `lighting/clock`). Everything that
//! asks the time goes through [`Clock`], so the rules live here once.
//! docs/behavior/lighting/clock.md.

use std::fmt;

mod sky;
mod time_of_day;

pub use sky::{MOON_PHASE, moon_direction, sun_direction};
pub use time_of_day::TimeOfDayError;

const SECONDS_PER_DAY: i64 = 86_400;

/// A time on Lighting's clock: whole seconds after midnight, 0 to 86,399.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock {
    seconds: u32,
}

impl Clock {
    pub const MIDNIGHT: Clock = Clock { seconds: 0 };

    /// `seconds` after midnight, wrapped into the day: a negative count is
    /// that long before midnight.
    pub fn from_seconds(seconds: i64) -> Clock {
        // rem_euclid of a positive i64 is within 0..86,400.
        Clock {
            seconds: seconds.rem_euclid(SECONDS_PER_DAY) as u32,
        }
    }

    /// What setting `ClockTime` to `hours` stores. Out of 0 to 24 is
    /// midnight, not wrapped as the other two ways in are (25.5, -0.25,
    /// 1e9 and infinities all read 0); NaN passes that check and lands
    /// where [`Clock::from_minutes`] puts it (Confirmed).
    pub fn from_hours(hours: f32) -> Clock {
        if hours < 0.0 || hours >= 24.0 {
            return Clock::MIDNIGHT;
        }
        Clock::from_minutes(f64::from(hours) * 60.0)
    }

    /// What `SetMinutesAfterMidnight(minutes)` stores: the seconds,
    /// truncated into a 32-bit integer, wrapped into the day. NaN is
    /// 20:45:52 in Studio, which is exactly where the x86 conversion's
    /// "integer indefinite" (-2^31) wraps to; the same conversion gives it
    /// for counts out of range (Inference: only NaN measured).
    pub fn from_minutes(minutes: f64) -> Clock {
        let seconds = minutes * 60.0;
        let whole = if seconds.is_nan() || !(-2_147_483_649.0..2_147_483_648.0).contains(&seconds) {
            i32::MIN
        } else {
            // In range, so `as` truncates towards zero as C's conversion does.
            seconds as i32
        };
        Clock::from_seconds(i64::from(whole))
    }

    pub fn seconds(self) -> u32 {
        self.seconds
    }

    /// `ClockTime`: hours after midnight, 0 to just under 24.
    pub fn hours(self) -> f64 {
        f64::from(self.seconds) / 3600.0
    }

    /// `GetMinutesAfterMidnight()`.
    pub fn minutes(self) -> f64 {
        f64::from(self.seconds) / 60.0
    }
}

/// `TimeOfDay`: always two digits each, `06:30:00`.
impl fmt::Display for Clock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = self.seconds;
        write!(f, "{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    }
}

#[cfg(test)]
mod tests;
