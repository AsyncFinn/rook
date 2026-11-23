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
