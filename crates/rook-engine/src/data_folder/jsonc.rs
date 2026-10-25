//! Lenient JSON: `//` and `/* */` comments and trailing commas, which
//! people add by hand, are dropped before parsing as plain JSON.

/// `text` as plain JSON: comments become spaces (so error positions still
/// point at the right line), and a comma before `}` or `]` is dropped.
pub(super) fn to_json(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    // Where in `out` the last comma outside a string is, until something
    // other than whitespace follows it.
    let mut comma: Option<usize> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '"' {
            comma = None;
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                match chars[i] {
                    '\\' if i + 1 < chars.len() => {
                        out.push(chars[i + 1]);
                        i += 1;
                    }
                    '"' => break,
                    _ => {}
                }
                i += 1;
            }
            i += 1;
        } else if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            out.push_str("  ");
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                out.push(if chars[i] == '\n' { '\n' } else { ' ' });
                i += 1;
            }
            out.push_str("  ");
            i += 2;
        } else {
            if (c == '}' || c == ']')
                && let Some(at) = comma.take()
            {
                out.replace_range(at..at + 1, " ");
            }
            if c == ',' {
                comma = Some(out.len());
            } else if !c.is_whitespace() {
                comma = None;
            }
            out.push(c);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn drops_comments_and_trailing_commas() {
        let text = r#"{
            // the cache
            "a": "x // not a comment", /* nor */
            "b": [1, 2,],
        }"#;
        let value: serde_json::Value = serde_json::from_str(&super::to_json(text)).unwrap();
        assert_eq!(value["a"], "x // not a comment");
        assert_eq!(value["b"], serde_json::json!([1, 2]));
    }
}
