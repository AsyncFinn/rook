//! Reading Rust source for the drift checks: comments and string contents
//! told apart from code, test code found, and functions cut out.

use std::ops::Range;

/// One `.rs` file, seen two ways at the same byte offsets.
pub(super) struct Source {
    pub(super) path: String,
    /// The text with comments blanked out.
    pub(super) code: String,
    /// [`code`](Self::code) with string and char contents blanked too, so
    /// braces and keywords inside them don't count.
    pub(super) mask: String,
    /// Where test code starts: the first `#[cfg(test)]`, or the start of
    /// a file that is all tests.
    pub(super) tests_from: usize,
    pub(super) text: String,
}

/// A function: its signature and body, both from [`Source::code`].
pub(super) struct Function {
    pub(super) line: usize,
    pub(super) signature: String,
    pub(super) body: String,
}

impl Source {
    pub(super) fn new(path: String, text: String) -> Self {
        let (code, mask) = blank(&text);
        let is_test_file = path.contains("/tests/") || path.ends_with("tests.rs");
        let tests_from = if is_test_file {
            0
        } else {
            mask.find("#[cfg(test)]").unwrap_or(mask.len())
        };
        Self {
            path,
            code,
            mask,
            tests_from,
            text,
        }
    }

    fn line_of(&self, at: usize) -> usize {
        self.text[..at].matches('\n').count() + 1
    }

    /// Every function outside test code that has a body.
    pub(super) fn functions(&self) -> Vec<Function> {
        let mask = self.mask.as_bytes();
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(found) = self.mask[from..self.tests_from].find("fn ") {
            let start = from + found;
            from = start + 3;
            let is_word = start == 0 || !is_ident(mask[start - 1]);
            if !is_word {
                continue;
            }
            let Some((open, body)) = body_after(mask, from) else {
                continue;
            };
            let signature_start = line_start(&self.mask, start);
            out.push(Function {
                line: self.line_of(start),
                signature: normalize(&self.code[signature_start..open])
                    .trim_start_matches("pub(crate) ")
                    .trim_start_matches("pub(super) ")
                    .trim_start_matches("pub ")
                    .to_owned(),
                body: normalize(&self.code[body]),
            });
        }
        out
    }

    /// Runs of more than `limit` comment lines outside test code, as (first
    /// line, length).
    pub(super) fn long_comments(&self, limit: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut run = 0;
        let lines: Vec<&str> = self.text[..self.tests_from].lines().collect();
        for (i, line) in lines.iter().chain([&""]).enumerate() {
            if line.trim_start().starts_with("//") {
                run += 1;
            } else {
                if run > limit {
                    out.push((i + 1 - run, run));
                }
                run = 0;
            }
        }
        out
    }

    /// Whether code outside tests contains `needle`.
    pub(super) fn code_contains(&self, needle: &str) -> bool {
        self.mask[..self.tests_from].contains(needle)
    }
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn line_start(text: &str, at: usize) -> usize {
    text[..at].rfind('\n').map_or(0, |i| i + 1)
}

/// Whitespace runs as one space.
fn normalize(code: &str) -> String {
    code.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The body of the function whose signature starts at `from`: where its
/// `{` is and the range inside the braces. `None` for a declaration
/// without one (`;`).
fn body_after(mask: &[u8], from: usize) -> Option<(usize, Range<usize>)> {
    let mut depth = 0i32;
    let mut i = from;
    while i < mask.len() {
        match mask[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b';' if depth == 0 => return None,
            b'{' if depth == 0 => break,
            _ => {}
        }
        i += 1;
    }
    let open = i;
    let mut braces = 0;
    while i < mask.len() {
        match mask[i] {
            b'{' => braces += 1,
            b'}' => {
                braces -= 1;
                if braces == 0 {
                    return Some((open, open + 1..i));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The text with comments blanked, and that again with string and char
/// contents blanked. Blanks keep newlines, so offsets and lines still
/// match the text.
fn blank(text: &str) -> (String, String) {
    let b = text.as_bytes();
    let mut code = b.to_vec();
    let mut mask = b.to_vec();
    let space = |v: &mut Vec<u8>, range: Range<usize>| {
        for c in &mut v[range] {
            if *c != b'\n' {
                *c = b' ';
            }
        }
    };
    let mut i = 0;
    while i < b.len() {
        let rest = &b[i..];
        if rest.starts_with(b"//") {
            let end = rest
                .iter()
                .position(|&c| c == b'\n')
                .map_or(b.len(), |n| i + n);
            space(&mut code, i..end);
            space(&mut mask, i..end);
            i = end;
        } else if rest.starts_with(b"/*") {
            let mut depth = 0;
            let mut j = i;
            while j < b.len() {
                if b[j..].starts_with(b"/*") {
                    depth += 1;
                    j += 2;
                } else if b[j..].starts_with(b"*/") {
                    depth -= 1;
                    j += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    j += 1;
                }
            }
            space(&mut code, i..j);
            space(&mut mask, i..j);
            i = j;
        } else if let Some(end) = raw_string_end(b, i) {
            space(&mut mask, i..end);
            i = end;
        } else if b[i] == b'"' {
            let mut j = i + 1;
            while j < b.len() && b[j] != b'"' {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            space(&mut mask, i + 1..j.min(b.len()));
            i = j + 1;
        } else if b[i] == b'\'' {
            // A char literal ('x', '\n', '\''), not a lifetime ('a).
            let end = if b.get(i + 1) == Some(&b'\\') {
                b[i + 2..]
                    .iter()
                    .position(|&c| c == b'\'')
                    .map(|n| i + 2 + n + 1)
            } else {
                let len = text[i + 1..].chars().next().map_or(1, char::len_utf8);
                (b.get(i + 1 + len) == Some(&b'\'')).then_some(i + 2 + len)
            };
            match end {
                Some(end) => {
                    space(&mut mask, i..end);
                    i = end;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    let text = |v: Vec<u8>| String::from_utf8(v).unwrap_or_default();
    (text(code), text(mask))
}

/// Where a raw string (`r"..."`, `r#"..."#`, `br"..."`) starting at `i`
/// ends, if one does.
fn raw_string_end(b: &[u8], i: usize) -> Option<usize> {
    if i > 0 && is_ident(b[i - 1]) {
        return None;
    }
    let mut j = i;
    if b.get(j) == Some(&b'b') {
        j += 1;
    }
    if b.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let hashes = b[j..].iter().take_while(|&&c| c == b'#').count();
    j += hashes;
    if b.get(j) != Some(&b'"') {
        return None;
    }
    let close: Vec<u8> = std::iter::once(b'"')
        .chain(std::iter::repeat_n(b'#', hashes))
        .collect();
    let start = j + 1;
    b[start..]
        .windows(close.len())
        .position(|w| w == close.as_slice())
        .map(|n| start + n + close.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_skip_strings_comments_and_tests() {
        let text = r#####"
// fn not_this() {}
pub(crate) fn one(x: [u8; 2]) -> &'static str {
    let s = "}{ fn two() {}";
    let c = '{';
    s
}
fn declared();
#[cfg(test)]
fn in_tests() {}
"#####;
        let source = Source::new("a.rs".into(), text.into());
        let fns = source.functions();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].signature, "fn one(x: [u8; 2]) -> &'static str");
        assert!(fns[0].body.contains(r#""}{ fn two() {}""#));
        assert_eq!(fns[0].line, 3);
    }

    #[test]
    fn long_comments_are_counted() {
        let text = format!("{}fn a() {{}}\n", "// x\n".repeat(17));
        let source = Source::new("a.rs".into(), text);
        assert_eq!(source.long_comments(15), [(1, 17)]);
    }
}
