//! Breaking shaped text into lines: at newlines, and when wrapping, before
//! a word that would pass the width; a word wider than a whole line is
//! split between glyphs (docs/behavior/gui/text.md#lines).

use super::shape::Glyph;

/// Shaped text, with its forced breaks.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Item {
    Glyph(Glyph),
    /// A newline, and the height of the line it ends were it empty.
    Break {
        ascent: f32,
        descent: f32,
    },
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Line {
    pub(crate) glyphs: Vec<Glyph>,
    /// Measured: spaces at the end of a line count ("a   " is wider than
    /// "a"), except where it wrapped (Inference).
    pub(crate) width: f32,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
}

impl Line {
    fn new((ascent, descent): (f32, f32)) -> Self {
        Self {
            ascent,
            descent,
            ..Self::default()
        }
    }

    fn push(&mut self, g: Glyph) {
        if self.glyphs.is_empty() {
            self.ascent = g.ascent;
            self.descent = g.descent;
        } else {
            self.ascent = self.ascent.max(g.ascent);
            self.descent = self.descent.max(g.descent);
        }
        self.width += g.advance;
        self.glyphs.push(g);
    }

    /// The width without the spaces it ends with.
    fn inked(&self) -> f32 {
        let trailing: f32 = self
            .glyphs
            .iter()
            .rev()
            .take_while(|g| g.space)
            .map(|g| g.advance)
            .sum();
        self.width - trailing
    }
}

/// The lines `items` make, wrapping at `wrap` pixels if given, and whether
/// a word had to be split. `empty` is the height of a line with no glyphs.
pub(crate) fn break_lines(
    items: &[Item],
    wrap: Option<f32>,
    empty: (f32, f32),
) -> (Vec<Line>, bool) {
    let mut lines = Vec::new();
    let mut line = Line::new(empty);
    let mut split = false;
    let mut i = 0;
    while i < items.len() {
        match items[i] {
            Item::Break { ascent, descent } => {
                if line.glyphs.is_empty() {
                    line = Line::new((ascent, descent));
                }
                lines.push(std::mem::replace(&mut line, Line::new((ascent, descent))));
                i += 1;
            }
            Item::Glyph(_) => {
                // A word: glyphs up to and including the spaces after it.
                let mut end = i;
                let mut seen_space = false;
                while let Some(Item::Glyph(g)) = items.get(end) {
                    if seen_space && !g.space {
                        break;
                    }
                    seen_space |= g.space;
                    end += 1;
                }
                let word: Vec<Glyph> = items[i..end]
                    .iter()
                    .filter_map(|it| match it {
                        Item::Glyph(g) => Some(*g),
                        Item::Break { .. } => None,
                    })
                    .collect();
                place_word(&mut lines, &mut line, &word, wrap, &mut split);
                i = end;
            }
        }
    }
    lines.push(line);
    (lines, split)
}

fn place_word(
    lines: &mut Vec<Line>,
    line: &mut Line,
    word: &[Glyph],
    wrap: Option<f32>,
    split: &mut bool,
) {
    let Some(width) = wrap else {
        word.iter().for_each(|&g| line.push(g));
        return;
    };
    let inked: f32 = word.iter().filter(|g| !g.space).map(|g| g.advance).sum();
    if !line.glyphs.is_empty() && line.width + inked > width {
        wrap_line(lines, line);
    }
    if line.glyphs.is_empty() && inked > width {
        // Split between glyphs, at least one on each line.
        *split = true;
        for &g in word {
            if !g.space && !line.glyphs.is_empty() && line.width + g.advance > width {
                wrap_line(lines, line);
            }
            line.push(g);
        }
        return;
    }
    word.iter().for_each(|&g| line.push(g));
}

/// End `line` where it wrapped: its trailing spaces don't count.
fn wrap_line(lines: &mut Vec<Line>, line: &mut Line) {
    line.width = line.inked();
    let metrics = (line.ascent, line.descent);
    lines.push(std::mem::replace(line, Line::new(metrics)));
}
