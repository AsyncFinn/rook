use glam::Vec3;

use super::*;

fn parsed(s: &str) -> Result<String, TimeOfDayError> {
    s.parse::<Clock>().map(|c| c.to_string())
}

/// Studio 0.748, conformance case `lighting/clock`.
#[test]
fn clock_time_keeps_whole_seconds_and_no_wrap() {
    for (hours, studio) in [
        (13.999, "13:59:56"),
        (14.5, "14:30:00"),
        (23.9999, "23:59:59"),
        (24.0, "00:00:00"),
        (25.5, "00:00:00"),
        (-1.0, "00:00:00"),
        (-0.25, "00:00:00"),
        (1e9, "00:00:00"),
        (f32::INFINITY, "00:00:00"),
        (f32::NAN, "20:45:52"),
    ] {
        assert_eq!(Clock::from_hours(hours).to_string(), studio, "{hours}");
    }
    assert_eq!(
        format!("{:.4}", Clock::from_hours(13.999).hours() as f32),
        "13.9989"
    );
}

#[test]
fn minutes_wrap_into_the_day() {
    for (minutes, studio) in [
        (0.5, "00:00:30"),
        (1439.99, "23:59:59"),
        (1440.0, "00:00:00"),
        (1500.0, "01:00:00"),
        (-30.0, "23:30:00"),
        (1e7, "10:40:00"),
        (f64::NAN, "20:45:52"),
    ] {
        assert_eq!(
            Clock::from_minutes(minutes).to_string(),
            studio,
            "{minutes}"
        );
    }
}

#[test]
fn time_of_day_reads_as_boost_does() {
    for (s, studio) in [
        ("6:30", "06:30:00"),
        ("7", "07:00:00"),
        ("12:30:45.6", "12:30:45"),
        ("25:00:00", "01:00:00"),
        ("24:00:00", "00:00:00"),
        ("-1:00:00", "23:00:00"),
        ("12:61:00", "13:01:00"),
        ("12:00:75", "12:01:15"),
        ("1:2:3:4", "01:02:03"),
        ("99999:00:00", "15:00:00"),
    ] {
        assert_eq!(parsed(s).as_deref(), Ok(studio), "{s:?}");
    }
    for s in [" 08:00:00 ", "08:00:00am", "8h", "abc", "1:70000"] {
        assert_eq!(parsed(s), Err(TimeOfDayError::NotANumber), "{s:?}");
    }
    assert_eq!(parsed(""), Err(TimeOfDayError::Empty));
    assert_eq!(
        TimeOfDayError::NotANumber.to_string(),
        "bad lexical cast: source type value could not be interpreted as target"
    );
}

/// `Lighting:GetSunDirection()` in Studio 0.748.
#[test]
fn sun_matches_studio() {
    for (hours, latitude, studio) in [
        (0.0, 0.0, Vec3::new(0.0, -0.917, -0.399)),
        (21.5, 0.0, Vec3::new(-0.558, -0.728, -0.399)),
        (14.0, 41.733, Vec3::new(-0.475, 0.823, 0.313)),
        (14.0, -30.0, Vec3::new(-0.297, 0.515, -0.804)),
        (14.0, 120.0, Vec3::new(0.057, -0.098, 0.994)),
        (14.0, -400.0, Vec3::new(-0.223, 0.386, -0.895)),
        // Only Studio's order of rounding lands here.
        (14.0, 1e9, Vec3::new(-0.498, 0.863, 0.084)),
    ] {
        let d = sun_direction(Clock::from_hours(hours), latitude);
        assert!(d.distance(studio) < 1e-3, "{hours} {latitude}: {d}");
    }
    assert!(sun_direction(Clock::MIDNIGHT, f32::NAN).is_nan());
    assert!(sun_direction(Clock::MIDNIGHT, f32::INFINITY).is_nan());
}

#[test]
fn moon_is_the_sun_half_a_day_on() {
    let noon = sun_direction(Clock::from_hours(12.0), 41.733);
    let midnight_moon = moon_direction(sun_direction(Clock::MIDNIGHT, 41.733));
    assert!(noon.distance(midnight_moon) < 1e-5);
}
