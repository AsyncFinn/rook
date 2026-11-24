//! How Roblox turns a number into an integer, wherever it does: property
//! writes, `UDim` offsets, Lighting's clock, JSON keys. It cuts toward zero,
//! and NaN, the infinities and anything past the integer's range on either
//! side become its lowest value, as x86's truncating conversion gives
//! ("integer indefinite"). Measured on `UDim.Offset`, `SoundEffect.Priority`
//! (2^31, -2^31 - 1, 1e12 and NaN all read -2147483648; conformance case
//! `content/int_properties`), `SetMinutesAfterMidnight(0/0)` (conformance
//! case `lighting/clock`) and `JSONEncode`'s number keys (`math.huge` is
//! `"-9223372036854775808"`; conformance case `scripting/json_edges`).

/// `n` as a 32-bit integer, as Roblox casts it.
pub fn int32(n: f64) -> i32 {
    let cut = n.trunc();
    // Both bounds are exact in an f64; NaN fails both comparisons.
    if (-2_147_483_648.0..2_147_483_648.0).contains(&cut) {
        cut as i32
    } else {
        i32::MIN
    }
}

/// `n` as a 64-bit integer, as Roblox casts it.
pub fn int64(n: f64) -> i64 {
    let cut = n.trunc();
    // 2^63 is exact in an f64 but past `i64::MAX`, so the range is half
    // open: `i64::MAX as f64` rounds up to it.
    if (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&cut) {
        cut as i64
    } else {
        i64::MIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_cast_as_roblox_casts_them() {
        assert_eq!(int32(3.7), 3);
        assert_eq!(int32(-3.7), -3);
        assert_eq!(int32(2_147_483_647.9), i32::MAX);
        assert_eq!(int32(-2_147_483_648.9), i32::MIN);
        for past in [
            2f64.powi(31),
            -2f64.powi(31) - 1.0,
            1e12,
            f64::NAN,
            f64::INFINITY,
        ] {
            assert_eq!(int32(past), i32::MIN, "{past}");
        }
        assert_eq!(int64(-1.5), -1);
        assert_eq!(int64(-2f64.powi(63)), i64::MIN);
        assert_eq!(int64(2f64.powi(62)), 1 << 62);
        for past in [2f64.powi(63), f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(int64(past), i64::MIN, "{past}");
        }
    }
}
