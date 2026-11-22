use crate::tests::*;

#[test]
fn encoding_service_round_trips() {
    expect(
        r#"
        local es = game:GetService("EncodingService")
        local zstd = Enum.CompressionAlgorithm.Zstd
        local text = buffer.fromstring(string.rep("rook ", 50))
        local packed = es:CompressBuffer(text, zstd)
        print(buffer.len(packed) < buffer.len(text), es:GetDecompressedBufferSize(packed, zstd))
        print(buffer.tostring(es:DecompressBuffer(packed, zstd)) == buffer.tostring(text))
        local b64 = es:Base64Encode(buffer.fromstring("hello"))
        print(buffer.tostring(b64), buffer.tostring(es:Base64Decode(b64)))
        print(buffer.len(es:CompressBuffer(text, zstd, 22)) > 0, buffer.len(es:CompressBuffer(text, zstd, -7)) > 0)
        "#,
        0,
        &["true 250", "true", "aGVsbG8= hello", "true true"],
    );
}

/// Errors and edge cases as Studio 0.748 gives them.
#[test]
fn encoding_service_refuses_what_studio_does() {
    expect(
        r#"
        local es = game:GetService("EncodingService")
        local zstd = Enum.CompressionAlgorithm.Zstd
        local nope = buffer.fromstring("nope")
        print(pcall(es.Base64Decode, es, buffer.fromstring("aGVsbG8")))
        print(pcall(es.Base64Decode, es, buffer.fromstring(" aGVsbG8=\n")))
        print(buffer.len(es:Base64Decode(buffer.fromstring(""))))
        print(pcall(es.DecompressBuffer, es, nope, zstd))
        print(es:GetDecompressedBufferSize(nope, zstd))
        print(pcall(es.CompressBuffer, es, "x", zstd))
        print(pcall(es.CompressBuffer, es, nope, zstd, 23))
        print(pcall(es.CompressBuffer, es, nope, zstd, -8))
        print(pcall(es.CompressBuffer, es, nope, zstd, 0 / 0))
        print(pcall(es.CompressBuffer, es, nope))
        print(pcall(es.CompressBuffer, es, nope, "bad"))
        print(pcall(es.DecompressBuffer, es, buffer.create(0), zstd))
        print(es:GetDecompressedBufferSize(buffer.create(0), zstd))
        "#,
        0,
        &[
            "false Base64Decode: input is not a base64 string",
            "false Base64Decode: input is not a base64 string",
            "0",
            "false DecompressBuffer: decompression failed - Unspecified error code",
            "nil",
            "false Argument 1 missing or nil",
            "false CompressBuffer: unsupported compression level for Zstd",
            "false CompressBuffer: unsupported compression level for Zstd",
            "false CompressBuffer: unsupported compression level for Zstd",
            "false Argument 2 missing or nil",
            "false Unable to cast string to CompressionAlgorithm",
            "false DecompressBuffer: decompression failed - Unspecified error code",
            "nil",
        ],
    );
}
