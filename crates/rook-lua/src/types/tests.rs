use super::fmt_g;

#[test]
fn formats_like_printf_g() {
    assert_eq!(fmt_g(1.0, 9), "1");
    assert_eq!(fmt_g(0.1f32 as f64, 9), "0.100000001");
    assert_eq!(fmt_g(-2.5, 9), "-2.5");
    assert_eq!(fmt_g(1e10, 9), "1e+10");
    assert_eq!(fmt_g(0.00001, 9), "1e-05");
    assert_eq!(fmt_g(123456789.0, 9), "123456789");
}
