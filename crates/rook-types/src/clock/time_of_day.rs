//! Reading a `TimeOfDay` string. Studio's errors are Boost's
//! (`bad lexical cast`, and MSVC's `invalid string position` from
//! `std::string::at` on an empty string), and every measured string reads
//! as Boost.Date_Time's `duration_from_string` reads it: a leading `-`
//! negates the whole time; `-`, `:`, `,` and `.` all separate fields and
//! empty fields are skipped; the first field is an `int` of hours, the
//! next two `unsigned short` minutes and seconds (`12:61:00` is 13:01),
//! the fourth fractional seconds (parsed, then dropped: Roblox keeps whole
//! seconds), and any after that are ignored (Confirmed: conformance case
//! `lighting/clock`). Unlike `ClockTime`, the result wraps into the day.

use super::Clock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TimeOfDayError {
    #[error("invalid string position")]
    Empty,
    #[error("bad lexical cast: source type value could not be interpreted as target")]
    NotANumber,
}

/// Digits Boost reads of fractional seconds, at its microsecond
/// resolution; the rest aren't looked at.
const FRACTION_DIGITS: usize = 6;

impl std::str::FromStr for Clock {
    type Err = TimeOfDayError;

    fn from_str(s: &str) -> Result<Clock, TimeOfDayError> {
        let negative = s.chars().next().ok_or(TimeOfDayError::Empty)? == '-';
        let mut seconds = 0i64;
        let fields = s.split(['-', ':', ',', '.']).filter(|f| !f.is_empty());
        for (i, field) in fields.enumerate() {
            match i {
                0 => seconds += i64::from(cast::<i32>(field)?) * 3600,
                1 => seconds += i64::from(cast::<u16>(field)?) * 60,
                2 => seconds += i64::from(cast::<u16>(field)?),
                3 => {
                    // A non-ASCII character among the first digits fails
                    // either way.
                    cast::<u64>(field.get(..FRACTION_DIGITS).unwrap_or(field))?;
                }
                _ => break,
            }
        }
        Ok(Clock::from_seconds(if negative {
            -seconds
        } else {
            seconds
        }))
    }
}

/// `boost::lexical_cast` to an integer: digits only (no spaces), within
/// the type's range. Boost also takes a leading `+` (Inference: Boost's
/// rule, not measured in Studio).
fn cast<T: std::str::FromStr>(field: &str) -> Result<T, TimeOfDayError> {
    let digits = field.strip_prefix('+').unwrap_or(field);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TimeOfDayError::NotANumber);
    }
    digits.parse().map_err(|_| TimeOfDayError::NotANumber)
}
