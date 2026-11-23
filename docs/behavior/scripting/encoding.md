# Encoding

Checked against Studio 0.748 on 2026-11-22.

EncodingService's Base64 and Zstandard methods (creator-docs
EncodingService), as Rook's `rook_lua::api::methods::encoding` implements
them. Every method takes a `buffer` and returns a new one.

- `Base64Encode` uses the standard alphabet with padding. `Base64Decode`
  requires the padding and refuses whitespace and the URL-safe alphabet,
  all with `Base64Decode: input is not a base64 string`; an empty buffer
  decodes to an empty one.
- `CompressBuffer` writes a zstd frame that records its size, so
  `GetDecompressedBufferSize` reads it back (250 bytes of text packed to 21
  in Studio). Levels from -7 to 22 compress, a fraction too (Inference:
  it's truncated); -8, 23, NaN and infinity raise `CompressBuffer:
  unsupported compression level for Zstd`. The default level is 1, as the
  API dump gives it.
- `DecompressBuffer` given something that isn't a zstd frame, or an empty
  buffer, raises `DecompressBuffer: decompression failed - Unspecified
  error code`; `GetDecompressedBufferSize` returns nil for both. Rook also
  refuses a frame that would decompress past Luau's largest buffer, 1 GiB.
- The `CompressionAlgorithm` argument must be given: without it a call is
  `Argument 2 missing or nil`, and a string that names no item is `Unable
  to cast string to CompressionAlgorithm`.
- A string in place of the buffer is `Argument 1 missing or nil`, like a
  missing one.
