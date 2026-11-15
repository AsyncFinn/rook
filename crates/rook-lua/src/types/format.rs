//! Numbers as Roblox prints them in datatypes' `tostring`.

/// Format like C's `%.9g`, which is how Roblox prints datatype components
/// (32-bit floats shown with 9 significant digits).
pub(crate) fn fmt_g(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() {
            "-0".into()
        } else {
            "0".into()
        };
    }
    let exp = x.abs().log10().floor() as i32;
    let precision = precision.max(1);
    let p = precision as i32;
    if exp < -4 || exp >= p {
        let s = format!("{:.*e}", precision - 1, x);
        let (mantissa, e) = s.split_once('e').expect("exponent form");
        let mantissa = strip_zeros(mantissa);
        let e: i32 = e.parse().expect("exponent");
        let sign = if e < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", e.abs())
    } else {
        let decimals = (p - 1 - exp).max(0) as usize;
        strip_zeros(&format!("{x:.decimals$}")).to_owned()
    }
}

fn strip_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

pub(crate) fn fmt_f32(x: f32) -> String {
    fmt_g(f64::from(x), 9)
}
