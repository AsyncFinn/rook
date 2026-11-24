use crate::tests::*;

/// Which tables `JSONEncode` writes as arrays, measured in Studio 0.748: the
/// length decides, so an empty table is `[]` (a web API given `{}` for an
/// empty list refuses it) and a mixed one keeps only its array part.
#[test]
fn json_encode_arrays_by_length() {
    expect(
        r#"
        local http = game:GetService("HttpService")
        local gap = { 1, 2 }
        gap[4] = 4
        local only2 = {}
        only2[2] = "b"
        for _, t in { {}, { a = {} }, { {}, {} }, { 1, x = 2 }, gap, { nil, 2 }, only2, { [0] = 1 } } do
            print(http:JSONEncode(t))
        end
        print(http:JSONEncode(http:JSONDecode("{}")))
        "#,
        0,
        &[
            "[]",
            r#"{"a":[]}"#,
            "[[],[]]",
            "[1]",
            "[1,2,null,4]",
            "[null,2]",
            r#"{"2":"b"}"#,
            r#"{"0":1}"#,
            "[]",
        ],
    );
}

/// How `JSONEncode` writes numbers, values JSON has no form for, and
/// object keys, measured in Studio 0.748.
#[test]
fn json_encode_writes_studio_forms() {
    expect(
        r#"
        local http = game:GetService("HttpService")
        for _, v in { -0, 2 ^ 53, 2 ^ 63, 1e21, 1e-7, 1 / 3, 0.000001, math.huge, 0 / 0 } do
            print(http:JSONEncode(v))
        end
        print(http:JSONEncode({ buffer.fromstring("hi!?") }))
        print(http:JSONEncode({ [1.5] = 1 }), http:JSONEncode({ [-math.huge] = 1 }))
        print(http:JSONEncode({ [Vector3.one] = 1 }))
        print(pcall(http.JSONEncode, http, { [true] = 1 }))
        print(pcall(http.JSONEncode, http, "\255"))
        print(pcall(http.JSONEncode, http))
        print(http:JSONEncode("\1/\127"))
        "#,
        0,
        &[
            "-0",
            "9007199254740992",
            "9223372036854776000",
            "1e21",
            "1e-7",
            "0.3333333333333333",
            "0.000001",
            r#"{"m":null,"t":"numeric","v":"inf"}"#,
            r#"{"m":null,"t":"numeric","v":"nan"}"#,
            r#"[{"m":null,"t":"buffer","base64":"aGkhPw=="}]"#,
            r#"{"1":1} {"-9223372036854775808":1}"#,
            r#"{"<Vector3> (1, 1, 1)":1}"#,
            "false Invalid table key type used",
            "false Can't convert to JSON",
            "false Argument 1 missing or nil",
            "\"\\u0001/\u{7f}\"",
        ],
    );
}

/// What `JSONDecode` reads, measured in Studio 0.748: bytes kept as they
/// are, standard JSON only, `-0` as 0.
#[test]
fn json_decode_reads_studio_forms() {
    expect(
        r#"
        local http = game:GetService("HttpService")
        print(http:JSONDecode('"\255"') == "\255", http:JSONDecode('"\\ud83d\\ude00"') == "😀")
        print(http:JSONDecode("-0"), http:JSONDecode("1e-999"), http:JSONDecode("9007199254740993"))
        local t = http:JSONDecode("[1,null,3]")
        print(t[1], t[2], t[3], #http:JSONDecode({}))
        for _, s in { "01", "+1", ".5", "[1,]", '"\\ud83d"', '"a\nb"', "1e999", "[1] // x", "" } do
            print(pcall(http.JSONDecode, http, s))
        end
        "#,
        0,
        &[
            "true true",
            "0 0 9007199254740992",
            "1 nil 3 0",
            "false Can't parse JSON",
            "false Can't parse JSON",
            "false Can't parse JSON",
            "false Can't parse JSON",
            "false Can't parse JSON",
            "false Can't parse JSON",
            "false Can't parse JSON",
            "false Can't parse JSON",
            "false Can't parse JSON",
        ],
    );
}

/// `UrlEncode` keeps only letters and digits, and `GenerateGUID` is
/// lowercase and casts its argument, measured in Studio 0.748.
#[test]
fn url_encode_and_guids() {
    expect(
        r#"
        local http = game:GetService("HttpService")
        print(http:UrlEncode("a-_.~ b"))
        local g = http:GenerateGUID(false)
        print(#g, g == g:lower(), g:sub(15, 15), #http:GenerateGUID(nil))
        print(pcall(http.GenerateGUID, http, "x"))
        "#,
        0,
        &[
            "a%2D%5F%2E%7E%20b",
            "36 true 4 38",
            "false Unable to cast string to bool",
        ],
    );
}
