//! HttpService's JSON: what `JSONEncode` writes and what `JSONDecode`
//! reads, as Studio 0.748 does (`docs/behavior/scripting/json.md`). Both
//! work on bytes, so a string's bytes survive a decode as they are, and
//! numbers are written in Studio's form.

use mlua::{Lua, Table, Value as LuaValue};
use rook_dom::InstanceId;

use crate::runtime::ctx::args::{Args, display_value, number_of, type_name};
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError};

const CANT_CONVERT: &str = "Can't convert to JSON";
const CANT_PARSE: &str = "Can't parse JSON";

/// How deep tables may nest when encoding, a cyclic table's endless
/// nesting included, before it gives up with [`CANT_CONVERT`].
const ENCODE_DEPTH: usize = 100;

/// How deep arrays and objects may nest when decoding before it gives up
/// with [`CANT_PARSE`], so deep input can't exhaust the stack.
const DECODE_DEPTH: usize = 128;

/// `JSONEncode(value)`. Measured: called with nothing it raises
/// `Argument 1 missing or nil`, where an explicit nil is `null`.
pub(super) fn encode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    if a.len() == 0 {
        return Err(a.missing(0));
    }
    let mut out = String::new();
    write_value(lua, a.get(0), 0, &mut out)?;
    Outcome::ret(lua, out)
}

/// `JSONDecode(text)`. Measured: a number is read as its text, and a
/// table (tried with an empty one) gives an empty table back.
pub(super) fn decode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = match a.get(0) {
        LuaValue::Table(_) => return Outcome::ret(lua, lua.create_table()?),
        LuaValue::String(s) => s.as_bytes().to_vec(),
        _ => a.string(0)?.into_bytes(),
    };
    let mut parser = Parser {
        lua,
        text: &text,
        at: 0,
    };
    parser.space();
    let value = parser.value(0)?;
    parser.space();
    if parser.at != text.len() {
        return Err(ScriptError(CANT_PARSE.into()));
    }
    Outcome::ret(lua, value)
}

fn write_value(lua: &Lua, v: &LuaValue, depth: usize, out: &mut String) -> ApiResult<()> {
    match v {
        LuaValue::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            write_number(number_of(v).unwrap_or(0.0), out);
        }
        LuaValue::String(s) => write_string(&s.as_bytes(), out)?,
        LuaValue::Table(t) => write_table(lua, t, depth, out)?,
        // Measured: `{"m":null,"t":"buffer","base64":"AA=="}`.
        LuaValue::Buffer(b) => {
            out.push_str(r#"{"m":null,"t":"buffer","base64":""#);
            out.push_str(&base64(&b.to_vec()));
            out.push_str(r#""}"#);
        }
        // Measured: nil, and every value JSON has no form for (Vector3,
        // Color3, functions, threads, instances, EnumItems), alone or in a
        // table.
        _ => out.push_str("null"),
    }
    Ok(())
}

/// A number as Studio writes it (measured): the shortest digits that read
/// back as it, as a plain number up to 21 digits before the point and 6
/// zeros after it, otherwise with an exponent (`1e21`, `1e-7`, no `+`);
/// `-0` keeps its sign. Infinities and NaN, which JSON can't hold, are
/// objects: `{"m":null,"t":"numeric","v":"inf"}` (`-inf`, `nan`).
fn write_number(n: f64, out: &mut String) {
    if !n.is_finite() {
        let v = if n.is_nan() {
            "nan"
        } else if n > 0.0 {
            "inf"
        } else {
            "-inf"
        };
        out.push_str(&format!(r#"{{"m":null,"t":"numeric","v":"{v}"}}"#));
        return;
    }
    if n.is_sign_negative() {
        out.push('-');
    }
    if n == 0.0 {
        out.push('0');
        return;
    }
    // Rust's `{:e}` gives the shortest digits that read back: `d.ddde<x>`.
    let sci = format!("{:e}", n.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits: String = mantissa.chars().filter(|&c| c != '.').collect();
    let len = digits.len() as i32;
    // Where the point goes, counted in digits from the first.
    let point = exp.parse::<i32>().unwrap_or(0) + 1;
    let zeros = |n: i32| "0".repeat(n.max(0) as usize);
    if len <= point && point <= 21 {
        out.push_str(&digits);
        out.push_str(&zeros(point - len));
    } else if 0 < point && point <= 21 {
        let (whole, fraction) = digits.split_at(point as usize);
        out.push_str(&format!("{whole}.{fraction}"));
    } else if -6 < point && point <= 0 {
        out.push_str(&format!("0.{}{digits}", zeros(-point)));
    } else {
        let (first, rest) = digits.split_at(1);
        out.push_str(first);
        if !rest.is_empty() {
            out.push('.');
            out.push_str(rest);
        }
        out.push_str(&format!("e{}", point - 1));
    }
}

/// A string, which must be UTF-8 (measured: `"\255"` raises `Can't
/// convert to JSON`). Measured: `"` and `\` escaped, `\n`, `\r`, `\t`,
/// `\b` and `\f` by name, other control characters as `\u00XX`, `/`, DEL
/// and everything past ASCII as they are. Inference: the hex digits are
/// capitals.
fn write_string(bytes: &[u8], out: &mut String) -> ApiResult<()> {
    let text = std::str::from_utf8(bytes).map_err(|_| ScriptError(CANT_CONVERT.into()))?;
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    Ok(())
}

/// A table: an array when it has a length (`#t`), or is empty, of entries
/// 1 to that length, nil as `null` and other keys dropped; otherwise an
/// object (measured). Objects are written with their keys sorted, as
/// Roblox's order isn't defined.
fn write_table(lua: &Lua, t: &Table, depth: usize, out: &mut String) -> ApiResult<()> {
    if depth > ENCODE_DEPTH {
        return Err(ScriptError(CANT_CONVERT.into()));
    }
    let len = t.raw_len();
    if len > 0 || t.is_empty() {
        out.push('[');
        for i in 1..=len {
            if i > 1 {
                out.push(',');
            }
            write_value(lua, &t.raw_get::<LuaValue>(i)?, depth + 1, out)?;
        }
        out.push(']');
        return Ok(());
    }
    let mut entries = Vec::new();
    for pair in t.clone().pairs::<LuaValue, LuaValue>() {
        let (k, v) = pair?;
        entries.push((key_text(lua, &k)?, v));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    out.push('{');
    for (i, (key, v)) in entries.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_string(key, out)?;
        out.push(':');
        write_value(lua, v, depth + 1, out)?;
    }
    out.push('}');
    Ok(())
}

/// An object key's text, measured: a string as itself; a number cut to a
/// whole 64-bit integer (`1.5` is `"1"`), one out of that range or NaN as
/// its least (`math.huge` is `"-9223372036854775808"`); a boolean raises
/// `Invalid table key type used`; anything else is its type and
/// `tostring` (`"<Table> (table: 0x...)"`, `"<Vector3> (1, 1, 1)"`).
/// Inference: types other than tables and Vector3 are named as `typeof`
/// names them, Luau's own capitalized.
fn key_text(lua: &Lua, k: &LuaValue) -> ApiResult<Vec<u8>> {
    Ok(match k {
        LuaValue::String(s) => s.as_bytes().to_vec(),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            rook_types::int64(number_of(k).unwrap_or(0.0))
                .to_string()
                .into_bytes()
        }
        LuaValue::Boolean(_) => return Err(ScriptError("Invalid table key type used".into())),
        k => {
            let kind = match type_name(k).as_str() {
                "table" => "Table".to_owned(),
                "function" => "Function".to_owned(),
                "thread" => "Thread".to_owned(),
                "buffer" => "Buffer".to_owned(),
                other => other.to_owned(),
            };
            format!("<{kind}> ({})", display_value(lua, k)).into_bytes()
        }
    })
}

/// Standard base64, padded.
fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// JSON read from bytes, as Studio reads it (measured): standard JSON
/// only (no comments, trailing commas, single quotes, bare keys, leading
/// zeros or `+`, `NaN` or `Infinity`), with surrounding whitespace, and
/// nothing after the value.
struct Parser<'a> {
    lua: &'a Lua,
    text: &'a [u8],
    at: usize,
}

fn bad<T>() -> ApiResult<T> {
    Err(ScriptError(CANT_PARSE.into()))
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.at).copied()
    }

    fn next(&mut self) -> ApiResult<u8> {
        let b = self.peek().ok_or_else(|| ScriptError(CANT_PARSE.into()))?;
        self.at += 1;
        Ok(b)
    }

    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, b: u8) -> ApiResult<()> {
        if self.next()? == b { Ok(()) } else { bad() }
    }

    fn value(&mut self, depth: usize) -> ApiResult<LuaValue> {
        match self.peek() {
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'"') => Ok(LuaValue::String(self.lua.create_string(self.string()?)?)),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => {
                for (word, value) in [
                    ("true", LuaValue::Boolean(true)),
                    ("false", LuaValue::Boolean(false)),
                    // Measured: null is nil, so in an array it leaves a
                    // gap and in an object no key.
                    ("null", LuaValue::Nil),
                ] {
                    if self.text[self.at..].starts_with(word.as_bytes()) {
                        self.at += word.len();
                        return Ok(value);
                    }
                }
                bad()
            }
        }
    }

    fn object(&mut self, depth: usize) -> ApiResult<LuaValue> {
        if depth > DECODE_DEPTH {
            return bad();
        }
        self.expect(b'{')?;
        let t = self.lua.create_table()?;
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(LuaValue::Table(t));
        }
        loop {
            self.space();
            if self.peek() != Some(b'"') {
                return bad();
            }
            let key = self.string()?;
            self.space();
            self.expect(b':')?;
            self.space();
            let value = self.value(depth)?;
            // Measured: a key given twice keeps its last value.
            t.raw_set(self.lua.create_string(key)?, value)?;
            self.space();
            match self.next()? {
                b',' => {}
                b'}' => return Ok(LuaValue::Table(t)),
                _ => return bad(),
            }
        }
    }

    fn array(&mut self, depth: usize) -> ApiResult<LuaValue> {
        if depth > DECODE_DEPTH {
            return bad();
        }
        self.expect(b'[')?;
        let t = self.lua.create_table()?;
        self.space();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(LuaValue::Table(t));
        }
        let mut i = 0;
        loop {
            self.space();
            i += 1;
            let value = self.value(depth)?;
            t.raw_set(i, value)?;
            self.space();
            match self.next()? {
                b',' => {}
                b']' => return Ok(LuaValue::Table(t)),
                _ => return bad(),
            }
        }
    }

    /// A number, measured: read to the nearest double (`9007199254740993`
    /// is 2^53), one too big for a double raises, one too small is 0, and
    /// `-0` written without a fraction or exponent is 0.
    fn number(&mut self) -> ApiResult<LuaValue> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.next()? {
            b'0' => {}
            b'1'..=b'9' => self.digits(),
            _ => return bad(),
        }
        let mut whole = true;
        if self.peek() == Some(b'.') {
            self.at += 1;
            self.required_digits()?;
            whole = false;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.required_digits()?;
            whole = false;
        }
        let token = std::str::from_utf8(&self.text[start..self.at])
            .map_err(|_| ScriptError(CANT_PARSE.into()))?;
        let n: f64 = token.parse().map_err(|_| ScriptError(CANT_PARSE.into()))?;
        if n.is_infinite() {
            return bad();
        }
        Ok(LuaValue::Number(if whole && n == 0.0 { 0.0 } else { n }))
    }

    fn digits(&mut self) {
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
    }

    fn required_digits(&mut self) -> ApiResult<()> {
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
            return bad();
        }
        self.digits();
        Ok(())
    }

    /// A string's bytes. Measured: escapes as JSON has them, `\u` pairs
    /// for characters past the BMP and a lone high surrogate raising;
    /// control characters raise; any other byte, invalid UTF-8 included,
    /// is kept as it is. Inference: a lone low surrogate raises too.
    fn string(&mut self) -> ApiResult<Vec<u8>> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            match self.next()? {
                b'"' => return Ok(out),
                b'\\' => {
                    let c = match self.next()? {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => self.escaped_char()?,
                        _ => return bad(),
                    };
                    let mut buf = [0; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                b if b < 0x20 => return bad(),
                b => out.push(b),
            }
        }
    }

    /// The character a `\u` escape (its `u` read) stands for.
    fn escaped_char(&mut self) -> ApiResult<char> {
        let first = self.hex4()?;
        let code = match first {
            0xD800..=0xDBFF => {
                self.expect(b'\\')?;
                self.expect(b'u')?;
                let second = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&second) {
                    return bad();
                }
                0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
            }
            0xDC00..=0xDFFF => return bad(),
            c => c,
        };
        char::from_u32(code).ok_or_else(|| ScriptError(CANT_PARSE.into()))
    }

    fn hex4(&mut self) -> ApiResult<u32> {
        let mut n = 0;
        for _ in 0..4 {
            let digit = (self.next()? as char)
                .to_digit(16)
                .ok_or_else(|| ScriptError(CANT_PARSE.into()))?;
            n = n * 16 + digit;
        }
        Ok(n)
    }
}
