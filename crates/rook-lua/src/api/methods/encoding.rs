//! EncodingService: Base64 and Zstandard over buffers (creator-docs
//! EncodingService), with the errors measured in Studio 0.748. Every method
//! takes a `buffer` and gives a new one. Zstd is the only
//! `CompressionAlgorithm`, so the algorithm argument is checked and not
//! otherwise read (docs/behavior/scripting/encoding.md).

use base64::Engine;
use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError};

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("EncodingService", "Base64Encode") => base64_encode,
        ("EncodingService", "Base64Decode") => base64_decode,
        ("EncodingService", "CompressBuffer") => compress,
        ("EncodingService", "DecompressBuffer") => decompress,
        ("EncodingService", "GetDecompressedBufferSize") => decompressed_size,
        _ => return None,
    })
}

fn bytes(a: &Args, i: usize) -> ApiResult<Vec<u8>> {
    match a.get(i) {
        LuaValue::Buffer(b) => Ok(b.to_vec()),
        // Measured: a string is "missing or nil" too.
        _ => Err(a.missing(i)),
    }
}

fn ret_buffer(lua: &Lua, data: &[u8]) -> ApiResult<Outcome> {
    Outcome::ret(lua, LuaValue::Buffer(lua.create_buffer(data)?))
}

fn base64_encode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = base64::engine::general_purpose::STANDARD.encode(bytes(a, 0)?);
    ret_buffer(lua, text.as_bytes())
}

fn base64_decode(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    // Measured: padding is required, and whitespace and the URL alphabet
    // are refused.
    let data = base64::engine::general_purpose::STANDARD
        .decode(bytes(a, 0)?)
        .map_err(|_| ScriptError("Base64Decode: input is not a base64 string".into()))?;
    ret_buffer(lua, &data)
}

/// The `CompressionAlgorithm` argument, which must be given (measured:
/// `Argument 2 missing or nil`, `Unable to cast string to
/// CompressionAlgorithm`).
fn algorithm(lua: &Lua, a: &Args) -> ApiResult<()> {
    a.enum_value(lua, 1, "CompressionAlgorithm").map(drop)
}

/// Luau's largest buffer, 1 GiB: a frame that would decompress past it
/// fails instead of exhausting memory.
const MAX_BUFFER: u64 = 1 << 30;

/// `input`'s frames decompressed, or `None` when it isn't zstd, would pass
/// [`MAX_BUFFER`], or is empty (measured: an empty buffer fails).
fn inflate(input: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    if input.is_empty() {
        return None;
    }
    let decoder = zstd::stream::read::Decoder::new(input).ok()?;
    let mut out = Vec::new();
    decoder.take(MAX_BUFFER + 1).read_to_end(&mut out).ok()?;
    (out.len() as u64 <= MAX_BUFFER).then_some(out)
}

fn compress(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let input = bytes(a, 0)?;
    algorithm(lua, a)?;
    // The API dump's default is 1. Measured: -7 to 22 compress (a fraction
    // too); -8, 23, NaN and infinity raise.
    let level = a.number_or(2, 1.0)?;
    if !(-7.0..=22.0).contains(&level) {
        return Err(ScriptError(
            "CompressBuffer: unsupported compression level for Zstd".into(),
        ));
    }
    let data = zstd::bulk::compress(&input, level as i32)
        .map_err(|e| ScriptError(format!("CompressBuffer: {e}")))?;
    ret_buffer(lua, &data)
}

fn decompress(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let input = bytes(a, 0)?;
    algorithm(lua, a)?;
    // Measured for input that isn't a zstd frame and an empty buffer;
    // Inference for the rest.
    let data = inflate(&input).ok_or_else(|| {
        ScriptError("DecompressBuffer: decompression failed - Unspecified error code".into())
    })?;
    ret_buffer(lua, &data)
}

/// The size the frame header records, or nil for input that isn't a zstd
/// frame or is empty (measured). Inference: a frame that doesn't record
/// its size is decompressed to measure it, and one that fails to is nil.
fn decompressed_size(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let input = bytes(a, 0)?;
    algorithm(lua, a)?;
    let size = match zstd::zstd_safe::get_frame_content_size(&input) {
        Ok(Some(n)) => Some(n as f64),
        Ok(None) => inflate(&input).map(|d| d.len() as f64),
        Err(_) => None,
    };
    Outcome::ret(lua, size)
}
