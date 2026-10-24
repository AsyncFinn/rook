//! Text laid out against values measured in Studio 0.743 (TextLabels in a
//! ScreenGui), with the fonts of the user's Roblox install; skipped when
//! there's none.

use std::sync::Arc;

use rook_content::resolver::install::{Places, newest_from};
use rook_content::{Config, Resolver};
use rook_types::font_from_enum;

use super::*;

fn fonts() -> Option<Fonts> {
    let install = newest_from(None, &Places::from_env(|n| std::env::var_os(n)))?;
    let mut config = Config::empty();
    config.install = Some(install);
    config.network = false;
    let mut fonts = Fonts::default();
    fonts.set_resolver(Arc::new(Resolver::new(config)));
    Some(fonts)
}

fn source(text: &str, font: &str, size: f32) -> Source {
    Source {
        text: text.into(),
        rich: false,
        style: Style {
            font: font_from_enum(font).unwrap(),
            size,
            color: [0.0; 3],
            transparency: 0.0,
            underline: false,
            strike: false,
            stroke: None,
            mark: None,
            case: Case::AsWritten,
        },
        wrapped: false,
        scaled: false,
        size_limits: None,
        align: Vec2::splat(0.5),
        line_height: 1.0,
        visible: None,
        carets: Carets::None,
    }
}

fn bounds(fonts: &mut Fonts, s: &Source, area: (f32, f32)) -> (f32, f32, bool) {
    let b = lay_out(fonts, s, Vec2::new(area.0, area.1), None);
    (b.bounds.x, b.bounds.y, b.fits)
}

macro_rules! need_fonts {
    () => {
        match fonts() {
            Some(f) => f,
            None => {
                eprintln!("skipped: no Roblox install found");
                return;
            }
        }
    };
}

#[test]
fn widths_and_heights() {
    let mut f = need_fonts!();
    let area = (400.0, 200.0);
    let hello = "Hello, world!";
    // Exact matches; Rook's rounding is within a few pixels elsewhere.
    assert_eq!(
        bounds(&mut f, &source(hello, "SourceSans", 24.0), area),
        (107.0, 24.0, true)
    );
    assert_eq!(bounds(&mut f, &source(hello, "Arial", 24.0), area).1, 24.0);
    assert_eq!(
        bounds(&mut f, &source(hello, "Legacy", 48.0), (2000.0, 400.0)),
        (359.0, 72.0, true)
    );
    assert_eq!(bounds(&mut f, &source(hello, "Legacy", 24.0), area).1, 36.0);
    assert_eq!(
        bounds(&mut f, &source("Hello", "Legacy", 13.0), area),
        (41.0, 19.0, true)
    );
    assert_eq!(
        bounds(
            &mut f,
            &source(&"H".repeat(10), "SourceSans", 100.0),
            (3000.0, 400.0)
        )
        .0,
        520.0
    );
    // Arial is drawn with Arimo, Gotham with Montserrat.
    assert_eq!(
        bounds(&mut f, &source(hello, "Arial", 100.0), area).0,
        bounds(&mut f, &source(hello, "Arimo", 100.0), area).0
    );
    assert!(bounds(&mut f, &source(hello, "Gotham", 24.0), area).0 > 0.0);
}

#[test]
fn lines() {
    let mut f = need_fonts!();
    let area = (400.0, 200.0);
    assert_eq!(
        bounds(&mut f, &source("", "Legacy", 24.0), area),
        (0.0, 36.0, true)
    );
    assert_eq!(
        bounds(&mut f, &source("a   ", "Legacy", 24.0), area).0,
        45.0
    );
    assert_eq!(
        bounds(&mut f, &source("A\nB", "Legacy", 24.0), area),
        (22.0, 72.0, true)
    );
    assert_eq!(
        bounds(&mut f, &source("A\n", "Legacy", 24.0), area),
        (22.0, 72.0, true)
    );
    let mut tall = source("A\nB", "Legacy", 24.0);
    tall.line_height = 1.5;
    assert_eq!(bounds(&mut f, &tall, area), (22.0, 90.0, true));
}

#[test]
fn wrapping_and_scaling() {
    let mut f = need_fonts!();
    let fox = "The quick brown fox jumps over the lazy dog";
    let mut wrap = source(fox, "Legacy", 24.0);
    wrap.wrapped = true;
    // Studio: 142 wide; five lines either way.
    let (w, h, fits) = bounds(&mut f, &wrap, (150.0, 200.0));
    assert!(
        (w - 142.0).abs() <= 2.0 && h == 180.0 && fits,
        "{w} {h} {fits}"
    );
    let mut long = source("Supercalifragilistic word", "Legacy", 24.0);
    long.wrapped = true;
    let (w, h, fits) = bounds(&mut f, &long, (60.0, 200.0));
    assert!(w <= 60.0 && h == 180.0 && !fits, "{w} {h} {fits}");
    let nowrap = bounds(&mut f, &source(fox, "Legacy", 24.0), (150.0, 200.0));
    assert!(!nowrap.2);

    let mut hi = source("Hi", "Legacy", 24.0);
    hi.scaled = true;
    assert_eq!(bounds(&mut f, &hi, (200.0, 50.0)), (43.0, 50.0, true));
    assert_eq!(bounds(&mut f, &hi, (600.0, 400.0)), (85.0, 100.0, true));
    // A UITextSizeConstraint holds the TextSize: 20, so 30-pixel lines.
    hi.size_limits = Some((1.0, 20.0));
    assert_eq!(bounds(&mut f, &hi, (200.0, 50.0)).1, 30.0);
    hi.size_limits = None;
    let mut hello = source("Hello, world!", "Legacy", 24.0);
    hello.scaled = true;
    // Studio: two 41-pixel lines; Rook's rounding makes "world!" 102 wide
    // there, so it takes 40.
    let two = bounds(&mut f, &hello, (100.0, 200.0)).1;
    assert!(two == 80.0 || two == 82.0, "{two}");
}

#[test]
fn rich_text() {
    let mut f = need_fonts!();
    let area = (400.0, 200.0);
    let mut rich = source("<b>Hello</b>, world!", "Legacy", 24.0);
    rich.rich = true;
    let plain = bounds(&mut f, &source("Hello, world!", "Legacy", 24.0), area).0;
    let bold = bounds(&mut f, &rich, area).0;
    assert!(bold > plain && bold < plain + 10.0, "{bold} vs {plain}");
    rich.rich = false;
    // Studio: 300, the markup shown as it is.
    let raw = bounds(&mut f, &rich, area).0;
    assert!((raw - 300.0).abs() <= 4.0, "{raw}");
    let mut sized = source("A<font size=\"40\">B</font>", "Legacy", 24.0);
    sized.rich = true;
    assert_eq!(bounds(&mut f, &sized, area), (58.0, 60.0, true));
    let mut amp = source("a &amp; b &lt;", "Legacy", 24.0);
    amp.rich = true;
    assert_eq!(bounds(&mut f, &amp, area).0, 104.0);
}

/// A TextBox's caret stops: before each character, at each line's end,
/// and at the start of a line after a newline.
#[test]
fn caret_stops() {
    let mut f = need_fonts!();
    let s = Source {
        align: Vec2::ZERO,
        carets: Carets::Text,
        ..source("ab\ncd", "SourceSans", 20.0)
    };
    let b = lay_out(&mut f, &s, Vec2::new(200.0, 100.0), None);
    let (start, height) = b.caret(0).unwrap();
    assert_eq!((start, height), (Vec2::ZERO, 20.0));
    let end_of_first = b.caret(2).unwrap().0;
    assert!(end_of_first.x > 0.0 && end_of_first.y == 0.0);
    assert_eq!(b.caret(3).unwrap().0, Vec2::new(0.0, 20.0));
    assert_eq!(b.index_at(Vec2::new(-5.0, 25.0)), Some(3));
    assert_eq!(b.index_at(Vec2::new(500.0, 25.0)), Some(5));
    // An empty box shows its placeholder; the caret can only be at 0.
    let empty = Source {
        carets: Carets::Placeholder,
        ..s
    };
    let b = lay_out(&mut f, &empty, Vec2::new(200.0, 100.0), None);
    assert_eq!(b.index_at(Vec2::new(500.0, 5.0)), Some(0));
}
