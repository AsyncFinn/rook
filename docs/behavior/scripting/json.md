# JSON

Checked against Studio 0.748 on 2026-11-23.

How HttpService's `JSONEncode` writes Luau values and `JSONDecode` reads
JSON (`rook_lua`'s `api::methods::web::json`), and how `UrlEncode` and
`GenerateGUID` answer (`api::methods::web::http`). Measured in Studio,
with `JSONEncode` and `conformance/cases/scripting/json_edges.luau`,
except where tagged.

## Tables

- A table with a length (`#t` above 0) is an array of its entries 1 to
  that length: a gap is `null` (`{1, 2}` with `[4]` set is `[1,2,null,4]`,
  `{nil, 2}` is `[null,2]`) and any other keys are dropped (`{1, x = 2}`
  is `[1]`, `{[1] = 1, [1000] = 2}` is `[1]`).
- An empty table is `[]`, nested too (`{a = {}}` is `{"a":[]}`), so
  `JSONDecode("{}")` encodes back as `[]`.
- Any other table is an object. Its keys:
  - a string is itself;
  - a number is cut to a whole 64-bit integer (`[1.5]` is `"1"`, `[-1]`
    `"-1"`, `[1000]` `"1000"`), and one out of that range, `math.huge`
    included, is its least, `"-9223372036854775808"`;
  - a boolean raises `Invalid table key type used`;
  - anything else is its type and `tostring`: `"<Table> (table:
    0x...)"`, `"<Vector3> (1, 1, 1)"`. *Inference:* other types are named
    as `typeof` names them, Luau's own (`function`, `thread`, `buffer`)
    capitalized.
- *Approximation:* an object's keys are written sorted, as Roblox's order
  isn't defined.
- *Approximation:* a table nested over 100 deep, a cyclic one included,
  raises `Can't convert to JSON`; Studio's own limit isn't measured
  (Studio 0.748 crashed on one nested 1000 deep).

## Other values

- Called with nothing, `JSONEncode()` raises `Argument 1 missing or nil`;
  `JSONEncode(nil)` is `null`.
- Numbers are written in their shortest form that reads back the same,
  as a plain number up to 21 digits before the point (`2^63` is
  `9223372036854776000`) and up to 6 zeros after it, otherwise with an
  exponent without `+` (`1e21`, `1e300`, `1e-7`). `-0` is `-0`.
- `math.huge`, `-math.huge` and NaN, which JSON can't hold, are objects:
  `{"m":null,"t":"numeric","v":"inf"}`, with `"-inf"` and `"nan"`.
- Strings must be UTF-8, or raise `Can't convert to JSON`. `"` and `\`
  are escaped, `\n`, `\r`, `\t`, `\b` and `\f` by name, other control
  characters as `\u00XX`; `/`, DEL and everything past ASCII are written
  as they are. *Inference:* the hex digits are capitals.
- A buffer is `{"m":null,"t":"buffer","base64":"<its bytes>"}`, its
  bytes in padded base64.
- Every other value JSON has no form for (Vector3, Color3, functions,
  threads, instances, EnumItems) is `null`, alone, in an array or in an
  object.

## JSONDecode

- `JSONDecode()` and `JSONDecode(nil)` raise `Argument 1 missing or nil`;
  a number is read as its text (`5` is 5); a table gives an empty table
  back (measured with an empty one).
- Only standard JSON reads; anything else raises `Can't parse JSON`: an
  empty or blank string, two values, comments, trailing commas, single
  quotes, bare keys, leading zeros or `+`, `.5`, `NaN`, `Infinity`, hex,
  a misspelt word, unclosed brackets. Whitespace around the value is
  fine.
- `null` is nil: alone, `JSONDecode` returns nil; in an array it leaves a
  gap (`[1,null,3]` has 1 and 3), in an object no key. A key given twice
  keeps its last value. Object keys stay strings (`{"1":"a"}` has `"1"`).
  `{}` and `[]` are both empty tables.
- Numbers read to the nearest double (`9007199254740993` is 2^53,
  `12345678901234567890` is about 1.2e19); one too big for a double
  (`1e999`) raises, one too small (`1e-999`) is 0, and `-0` is 0.
- Strings take JSON's escapes, `\u` pairs for characters past the BMP
  and `\u0000` as a NUL byte; a lone high surrogate, an unknown escape
  and a raw control character (a line break) raise. *Inference:* a lone
  low surrogate raises too. Any other byte is kept as it is, invalid
  UTF-8 included (`"\255"` is that one byte).
- *Approximation:* arrays and objects nested over 128 deep raise `Can't
  parse JSON`; Studio's own limit isn't measured.

## UrlEncode and GenerateGUID

- `UrlEncode` keeps ASCII letters and digits and writes every other byte
  as `%XX` in capitals, `-`, `_`, `.` and `~` included (`a b` is
  `a%20b`, `é` is `%C3%A9`). A number is its text; nil raises `Argument
  1 missing or nil`.
- `GenerateGUID` gives a random version 4 GUID in lowercase hex, in
  braces unless its argument is false (nil and true are braces); one that
  isn't a boolean raises `Unable to cast string to bool`.

## Not yet

How `JSONDecode` reads back the `{"m":null,"t":...}` objects for
infinities, NaN and buffers, and Studio's nesting limits.
