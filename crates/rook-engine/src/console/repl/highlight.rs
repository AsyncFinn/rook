//! Colors for Luau at the command bar: keywords, constants, strings,
//! numbers and comments. Only the terminal's 16 named colors, so the
//! user's theme decides what they look like.

const KEYWORD: &str = "\x1b[35m";
const CONSTANT: &str = "\x1b[36m";
const STRING: &str = "\x1b[32m";
const NUMBER: &str = "\x1b[33m";
const COMMENT: &str = "\x1b[90m";
const RESET: &str = "\x1b[0m";

/// `source` with color codes around its tokens. The whole buffer is
/// colored at once, so a long string or comment spanning lines stays one
/// token.
pub(super) fn highlight(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len() * 2);
    let mut i = 0;
    while i < bytes.len() {
        let start = i;
        let color = match bytes[i] {
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                i = comment_end(bytes, i + 2);
                Some(COMMENT)
            }
            b'"' | b'\'' | b'`' => {
                i = quoted_end(bytes, i);
                Some(STRING)
            }
            b'[' if long_bracket(bytes, i).is_some() => {
                i = long_end(bytes, i);
                Some(STRING)
            }
            b'0'..=b'9' => {
                i = number_end(bytes, i);
                Some(NUMBER)
            }
            b'.' if bytes.get(i + 1).is_some_and(u8::is_ascii_digit) => {
                i = number_end(bytes, i);
                Some(NUMBER)
            }
            b if b.is_ascii_alphabetic() || b == b'_' => {
                i = word_end(bytes, i);
                word_color(&source[start..i])
            }
            // Token ends above are all ASCII, so this is the only place a
            // multi-byte character is stepped over.
            _ => {
                i += source[i..].chars().next().map_or(1, char::len_utf8);
                None
            }
        };
        match color {
            Some(color) => {
                out.push_str(color);
                out.push_str(&source[start..i]);
                out.push_str(RESET);
            }
            None => out.push_str(&source[start..i]),
        }
    }
    out
}

fn word_color(word: &str) -> Option<&'static str> {
    match word {
        "nil" | "true" | "false" => Some(CONSTANT),
        // `continue` is contextual in Luau, but a variable of that name is
        // rare enough that coloring it as a keyword helps more than not.
        "and" | "break" | "continue" | "do" | "else" | "elseif" | "end" | "for" | "function"
        | "if" | "in" | "local" | "not" | "or" | "repeat" | "return" | "then" | "until"
        | "while" => Some(KEYWORD),
        _ => None,
    }
}

fn word_end(bytes: &[u8], mut i: usize) -> usize {
    while bytes
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        i += 1;
    }
    i
}

/// Numbers keep their letters and dots (`0x1F`, `1e-3`, `1_000`, `.5`), so
/// a malformed one stays one token as Luau's lexer reads it.
fn number_end(bytes: &[u8], mut i: usize) -> usize {
    let hex = bytes.get(i) == Some(&b'0') && matches!(bytes.get(i + 1), Some(b'x' | b'X'));
    while let Some(&b) = bytes.get(i) {
        let exponent_sign = matches!(b, b'+' | b'-')
            && !hex
            && matches!(bytes.get(i.wrapping_sub(1)), Some(b'e' | b'E'));
        if !(b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || exponent_sign) {
            break;
        }
        i += 1;
    }
    i
}

/// A quoted string ends at its closing quote, or unterminated at the end of
/// the line (where Luau stops reading it too). `\` skips the next byte,
/// which also carries the string over an escaped newline.
fn quoted_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while let Some(&b) = bytes.get(i) {
        match b {
            b'\\' => i += 2,
            b'\n' => return i,
            _ if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// The level of a long bracket (`[[` is 0, `[==[` is 2) opening at `i`.
fn long_bracket(bytes: &[u8], i: usize) -> Option<usize> {
    let level = bytes[i + 1..].iter().take_while(|&&b| b == b'=').count();
    (bytes.get(i + 1 + level) == Some(&b'[')).then_some(level)
}

/// The end of a long string or comment opening at `i`: past its matching
/// `]=*]`, or the end of the input while it's still open.
fn long_end(bytes: &[u8], i: usize) -> usize {
    let Some(level) = long_bracket(bytes, i) else {
        return i + 1;
    };
    let mut close = vec![b'='; level + 2];
    close[0] = b']';
    close[level + 1] = b']';
    let body = i + level + 2;
    bytes[body..]
        .windows(close.len())
        .position(|w| w == close)
        .map_or(bytes.len(), |p| body + p + close.len())
}

/// A comment after its `--`: a long one to its closing bracket, a short one
/// to the end of the line.
fn comment_end(bytes: &[u8], i: usize) -> usize {
    if bytes.get(i) == Some(&b'[') && long_bracket(bytes, i).is_some() {
        return long_end(bytes, i);
    }
    bytes[i..]
        .iter()
        .position(|&b| b == b'\n')
        .map_or(bytes.len(), |p| i + p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colored spans of `source`, as (color, text).
    fn spans(source: &str) -> Vec<(&'static str, String)> {
        let out = highlight(source);
        let mut spans = Vec::new();
        for part in out.split(RESET) {
            for color in [KEYWORD, CONSTANT, STRING, NUMBER, COMMENT] {
                if let Some(at) = part.find(color) {
                    spans.push((color, part[at + color.len()..].to_owned()));
                }
            }
        }
        spans
    }

    #[test]
    fn tokens_take_their_colors() {
        assert_eq!(
            spans("local x = nil -- note\nreturn 0x1F + 1e-3, 'a\\'b', [==[ ]] ]==]"),
            [
                (KEYWORD, "local".into()),
                (CONSTANT, "nil".into()),
                (COMMENT, "-- note".into()),
                (KEYWORD, "return".into()),
                (NUMBER, "0x1F".into()),
                (NUMBER, "1e-3".into()),
                (STRING, "'a\\'b'".into()),
                (STRING, "[==[ ]] ]==]".into()),
            ]
        );
    }

    #[test]
    fn open_strings_and_comments_run_to_their_ends() {
        // A short string stops at the line's end; long ones carry on.
        assert_eq!(
            spans("x = \"open\nlocal --[[ a\nb ]] y"),
            [
                (STRING, "\"open".into()),
                (KEYWORD, "local".into()),
                (COMMENT, "--[[ a\nb ]]".into()),
            ]
        );
        assert_eq!(spans("t[[[x"), [(STRING, "[[[x".into())]);
    }

    #[test]
    fn names_and_other_text_pass_through() {
        let source = "print(ending, élan.x)";
        assert_eq!(highlight(source), source);
    }
}
