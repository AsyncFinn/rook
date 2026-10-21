//! `RichText` markup: the tags and escapes of creator-docs' rich text
//! page, turned into runs of text with one style each. Markup that
//! doesn't parse shows as it is (Inference).

use rook_types::{Font, FontStyle, FontWeight, font_from_enum};

/// How a run of text looks.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Style {
    pub(crate) font: Font,
    pub(crate) size: f32,
    pub(crate) color: [f32; 3],
    pub(crate) transparency: f32,
    pub(crate) underline: bool,
    pub(crate) strike: bool,
    pub(crate) stroke: Option<Stroke>,
    /// A highlight behind the text: colour and transparency.
    pub(crate) mark: Option<([f32; 3], f32)>,
    pub(crate) case: Case,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Stroke {
    pub(crate) color: [f32; 3],
    pub(crate) thickness: f32,
    pub(crate) transparency: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Case {
    AsWritten,
    Upper,
    SmallCaps,
}

/// Text in one style.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    pub(crate) text: String,
    pub(crate) style: Style,
}

/// The runs `text` makes with `RichText` on, or `None` when its markup
/// doesn't parse.
pub(crate) fn parse(text: &str, base: &Style) -> Option<Vec<Span>> {
    let mut spans: Vec<Span> = Vec::new();
    let mut stack: Vec<(String, Style)> = Vec::new();
    let mut style = base.clone();
    let mut rest = text;
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            push(&mut spans, &unescape(rest), &style);
            break;
        };
        push(&mut spans, &unescape(&rest[..open]), &style);
        rest = &rest[open..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = &after[after.find("-->")? + 3..];
            continue;
        }
        let close = rest.find('>')?;
        let tag = rest[1..close].trim();
        rest = &rest[close + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            let (open_name, outer) = stack.pop()?;
            if open_name != name.trim().to_ascii_lowercase() {
                return None;
            }
            style = outer;
            continue;
        }
        let (name, attrs) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        let name = name.trim_end_matches('/').to_ascii_lowercase();
        if name == "br" {
            push(&mut spans, "\n", &style);
            continue;
        }
        let mut inner = style.clone();
        apply(&name, &attributes(attrs)?, &mut inner)?;
        if tag.ends_with('/') {
            continue;
        }
        stack.push((name, std::mem::replace(&mut style, inner)));
    }
    stack.is_empty().then_some(spans)
}

/// `text` as it is, in one style.
pub(crate) fn plain(text: &str, base: &Style) -> Vec<Span> {
    vec![Span {
        text: text.to_owned(),
        style: base.clone(),
    }]
}

fn push(spans: &mut Vec<Span>, text: &str, style: &Style) {
    if text.is_empty() {
        return;
    }
    match spans.last_mut() {
        Some(last) if last.style == *style => last.text.push_str(text),
        _ => spans.push(Span {
            text: text.to_owned(),
            style: style.clone(),
        }),
    }
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let entity = [
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&apos;", '\''),
            ("&amp;", '&'),
        ]
        .into_iter()
        .find(|(e, _)| rest.starts_with(e));
        match entity {
            Some((e, c)) => {
                out.push(c);
                rest = &rest[e.len()..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `name="value"` pairs, in either quote.
fn attributes(text: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut rest = text.trim().trim_end_matches('/').trim();
    while !rest.is_empty() {
        let eq = rest.find('=')?;
        let name = rest[..eq].trim().to_ascii_lowercase();
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let end = after[1..].find(quote)? + 1;
        out.push((name, unescape(&after[1..end])));
        rest = after[end + 1..].trim_start();
    }
    Some(out)
}

/// What tag `name` with `attrs` does to `style`; `None` for a tag Roblox
/// doesn't know.
fn apply(name: &str, attrs: &[(String, String)], style: &mut Style) -> Option<()> {
    let get = |key: &str| {
        attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    match name {
        "b" => style.font.weight = FontWeight::Bold,
        "i" => style.font.style = FontStyle::Italic,
        "u" => style.underline = true,
        "s" => style.strike = true,
        "uppercase" | "uc" => style.case = Case::Upper,
        "smallcaps" | "sc" => style.case = Case::SmallCaps,
        "font" => {
            if let Some(c) = get("color") {
                style.color = color(c)?;
            }
            if let Some(s) = get("size") {
                style.size = s.trim().parse().ok()?;
            }
            if let Some(face) = get("face") {
                let f = font_from_enum(face)?;
                style.font.family = f.family;
            }
            if let Some(family) = get("family") {
                style.font.family = family.to_owned();
            }
            if let Some(w) = get("weight") {
                style.font.weight = weight(w)?;
            }
            if let Some(t) = get("transparency") {
                style.transparency = t.trim().parse().ok()?;
            }
        }
        "stroke" => {
            let mut stroke = Stroke {
                color: [0.0; 3],
                thickness: 1.0,
                transparency: 0.0,
            };
            if let Some(c) = get("color") {
                stroke.color = color(c)?;
            }
            if let Some(t) = get("thickness") {
                stroke.thickness = t.trim().parse().ok()?;
            }
            if let Some(t) = get("transparency") {
                stroke.transparency = t.trim().parse().ok()?;
            }
            style.stroke = Some(stroke);
        }
        "mark" => {
            let c = get("color").map_or(Some([1.0, 1.0, 0.0]), color)?;
            let t = get("transparency").map_or(Some(0.0), |t| t.trim().parse().ok())?;
            style.mark = Some((c, t));
        }
        _ => return None,
    }
    Some(())
}

/// `#RRGGBB` or `rgb(r, g, b)` with 0-255 channels.
fn color(text: &str) -> Option<[f32; 3]> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let n = u32::from_str_radix(hex, 16)
            .ok()
            .filter(|_| hex.len() == 6)?;
        return Some([(n >> 16) & 255, (n >> 8) & 255, n & 255].map(|c| c as f32 / 255.0));
    }
    let inner = text.strip_prefix("rgb(")?.strip_suffix(')')?;
    let parts: Vec<f32> = inner
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    let [r, g, b] = parts.as_slice() else {
        return None;
    };
    Some([*r, *g, *b].map(|c| c.clamp(0.0, 255.0) / 255.0))
}

fn weight(text: &str) -> Option<FontWeight> {
    if let Ok(n) = text.trim().parse::<u16>() {
        return FontWeight::from_u16(n.clamp(100, 900) / 100 * 100);
    }
    Some(match text.trim().to_ascii_lowercase().as_str() {
        "thin" => FontWeight::Thin,
        "extralight" => FontWeight::ExtraLight,
        "light" => FontWeight::Light,
        "regular" => FontWeight::Regular,
        "medium" => FontWeight::Medium,
        "semibold" => FontWeight::SemiBold,
        "bold" => FontWeight::Bold,
        "extrabold" => FontWeight::ExtraBold,
        "heavy" => FontWeight::Heavy,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Style {
        Style {
            font: Font::default(),
            size: 24.0,
            color: [0.0; 3],
            transparency: 0.0,
            underline: false,
            strike: false,
            stroke: None,
            mark: None,
            case: Case::AsWritten,
        }
    }

    #[test]
    fn tags_and_escapes() {
        let spans = parse(
            "<b>Hello</b>, <font color=\"#FF0000\" size='40'>w&amp;</font><br/>x",
            &base(),
        )
        .unwrap();
        let texts: Vec<&str> = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, ["Hello", ", ", "w&", "\nx"]);
        assert_eq!(spans[0].style.font.weight, FontWeight::Bold);
        assert_eq!(spans[2].style.color, [1.0, 0.0, 0.0]);
        assert_eq!(spans[2].style.size, 40.0);
    }

    #[test]
    fn bad_markup_is_none() {
        assert!(parse("<b>open", &base()).is_none());
        assert!(parse("<b></i>", &base()).is_none());
        assert!(parse("<blink>x</blink>", &base()).is_none());
        assert!(parse("a < b", &base()).is_none());
        assert_eq!(parse("a <!-- note --> b", &base()).unwrap()[0].text, "a  b");
    }
}
