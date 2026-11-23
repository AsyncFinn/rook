# JSON

Checked against Studio 0.748 on 2026-11-22.

How `HttpService:JSONEncode` writes Luau tables (`rook_lua`'s
`api::methods::web::http`). Measured with `JSONEncode` in Studio:

- A table with a length (`#t` above 0) is an array of its entries 1 to
  that length: a gap is `null` (`{1, 2}` with `[4]` set is `[1,2,null,4]`,
  `{nil, 2}` is `[null,2]`) and any other keys are dropped (`{1, x = 2}`
  is `[1]`).
- An empty table is `[]`, nested too (`{a = {}}` is `{"a":[]}`), so
  `JSONDecode("{}")` encodes back as `[]`.
- Any other table is an object, its number keys written as strings
  (`{[0] = 1}` is `{"0":1}`; a table with only `[2]` set is `{"2":"b"}`).
