//! `Lighting.TimeOfDay` is "HH:MM:SS"; `ClockTime` is hours as a number.

/// Hours past midnight for a `TimeOfDay` string.
pub fn clock_hours(time_of_day: &str) -> f64 {
    let mut parts = time_of_day
        .split(':')
        .map(|p| p.trim().parse::<f64>().unwrap_or(0.0));
    let h = parts.next().unwrap_or(0.0);
    let m = parts.next().unwrap_or(0.0);
    let s = parts.next().unwrap_or(0.0);
    (h + m / 60.0 + s / 3600.0).rem_euclid(24.0)
}

/// The `TimeOfDay` string for hours past midnight.
pub fn time_of_day(hours: f64) -> String {
    let total = (hours.rem_euclid(24.0) * 3600.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600 % 24,
        total / 60 % 60,
        total % 60
    )
}
