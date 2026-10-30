//! Google's Draco geometry compression, as mesh version 7.00's
//! `COREMESH` 2 chunk uses it: bitstream 2.2, sequential connectivity, and
//! attributes stored raw, quantized or as integers with difference
//! prediction. Draco's other modes (edgebreaker connectivity, mesh-aware
//! prediction, octahedral normals, metadata) aren't in any Roblox mesh
//! seen and are refused as unsupported. What Roblox puts in each attribute
//! is in `docs/behavior/meshes/file-meshes.md`.

mod rans;

use super::MeshError;
use super::reader::{Reader, checked_count};

/// A decoded Draco mesh: one entry per point, triangles by point index.
#[derive(Debug, Default)]
pub(super) struct Geometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub faces: Vec<[u32; 3]>,
}

pub(super) fn decode(bytes: &[u8]) -> Result<Geometry, MeshError> {
    let mut r = Reader::new(bytes);
    if r.bytes(5)? != b"DRACO" {
        return Err(MeshError::Damaged("COREMESH isn't Draco".into()));
    }
    let unsupported = |what: String| Err(MeshError::Unsupported(format!("7.00 (Draco {what})")));
    let (major, minor) = (r.u8()?, r.u8()?);
    if (major, minor) != (2, 2) {
        return unsupported(format!("{major}.{minor}"));
    }
    let (kind, method, flags) = (r.u8()?, r.u8()?, r.u16()?);
    if kind != 1 || method != 0 {
        return unsupported(format!("encoding {kind}/{method}"));
    }
    if flags != 0 {
        return unsupported(format!("flags {flags:#x}"));
    }
    let faces = checked_count(r.varint()?)?;
    let points = checked_count(r.varint()?)?;
    let faces = connectivity(&mut r, faces, points)?;
    let attributes = attributes(&mut r, points)?;
    let find = |pick: fn(&Attribute) -> bool, what: &str| {
        attributes
            .iter()
            .find(|a| pick(a))
            .map(|a| &a.values)
            .ok_or_else(|| MeshError::Damaged(format!("the Draco mesh has no {what}")))
    };
    // Roblox stores normals as a generic attribute, not Draco's normal
    // type (Observed in every 7.00 mesh read).
    let positions = find(|a| a.kind == POSITION && a.float(3), "positions")?;
    let normals = find(
        |a| matches!(a.kind, NORMAL | GENERIC) && a.float(3),
        "normals",
    )?;
    let uvs = find(|a| a.kind == TEX_COORD && a.float(2), "UVs")?;
    Ok(Geometry {
        positions: positions.as_chunks().0.to_vec(),
        normals: normals.as_chunks().0.to_vec(),
        uvs: uvs.as_chunks().0.to_vec(),
        faces,
    })
}

/// Sequential connectivity: each triangle's three point indices, stored
/// plainly or entropy coded as differences.
fn connectivity(r: &mut Reader, faces: usize, points: usize) -> Result<Vec<[u32; 3]>, MeshError> {
    if faces == 0 || points == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(faces);
    match r.u8()? {
        0 => {
            let symbols = rans::symbols(r, faces * 3, 1)?;
            let mut last = 0i64;
            for face in symbols.as_chunks::<3>().0 {
                out.push(face.map(|s| {
                    let diff = i64::from(s >> 1);
                    last += if s & 1 == 1 { -diff } else { diff };
                    last as u32
                }));
                if last < 0 {
                    return Err(MeshError::Damaged("a Draco index is negative".into()));
                }
            }
        }
        1 => {
            for _ in 0..faces {
                let mut face = [0u32; 3];
                for i in &mut face {
                    *i = match points {
                        ..256 => u32::from(r.u8()?),
                        256..65536 => u32::from(r.u16()?),
                        65536..2097152 => r.varint()? as u32,
                        _ => r.u32()?,
                    };
                }
                out.push(face);
            }
        }
        m => return Err(MeshError::Damaged(format!("Draco connectivity {m}"))),
    }
    Ok(out)
}

// Draco's attribute types.
const POSITION: u8 = 0;
const NORMAL: u8 = 1;
const TEX_COORD: u8 = 3;
const GENERIC: u8 = 4;

const FLOAT32: u8 = 9;

/// How an attribute's values are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Coding {
    Raw,
    Integer,
    Quantized,
}

#[derive(Debug)]
struct Attribute {
    kind: u8,
    data_type: u8,
    components: usize,
    coding: Coding,
    /// Each point's components; floats only (integer attributes are
    /// stepped over, not kept).
    values: Vec<f32>,
    /// Integer and quantized values before they're turned into floats.
    integers: Vec<i32>,
}

impl Attribute {
    fn float(&self, components: usize) -> bool {
        self.data_type == FLOAT32 && self.components == components
    }
}

/// Every attribute decoder's attributes, in the order Draco stores them:
/// each attribute's values, then what quantized ones need to turn back
/// into floats.
fn attributes(r: &mut Reader, points: usize) -> Result<Vec<Attribute>, MeshError> {
    let mut out = Vec::new();
    for _ in 0..r.u8()? {
        let first = out.len();
        let count = checked_count(r.varint()?)?;
        for _ in 0..count {
            let (kind, data_type, components) = (r.u8()?, r.u8()?, usize::from(r.u8()?));
            r.u8()?; // Normalized.
            r.varint()?; // Unique id.
            if components == 0 {
                return Err(MeshError::Damaged(
                    "a Draco attribute has no components".into(),
                ));
            }
            out.push(Attribute {
                kind,
                data_type,
                components,
                coding: Coding::Raw,
                values: Vec::new(),
                integers: Vec::new(),
            });
        }
        for a in &mut out[first..] {
            a.coding = match r.u8()? {
                0 => Coding::Raw,
                1 => Coding::Integer,
                2 => Coding::Quantized,
                c => {
                    return Err(MeshError::Unsupported(format!(
                        "7.00 (Draco attribute coding {c})"
                    )));
                }
            };
        }
        for a in &mut out[first..] {
            values(r, a, points)?;
            if a.coding == Coding::Integer {
                a.integers = Vec::new();
            }
        }
        for a in out[first..]
            .iter_mut()
            .filter(|a| a.coding == Coding::Quantized)
        {
            dequantize(r, a)?;
        }
    }
    Ok(out)
}

fn values(r: &mut Reader, a: &mut Attribute, points: usize) -> Result<(), MeshError> {
    let n = points
        .checked_mul(a.components)
        .ok_or_else(|| MeshError::Damaged("a Draco attribute's size overflows".into()))?;
    if a.coding == Coding::Raw {
        let size = match a.data_type {
            1 | 2 | 11 => 1,
            3 | 4 => 2,
            5 | 6 | FLOAT32 => 4,
            7 | 8 | 10 => 8,
            t => return Err(MeshError::Damaged(format!("Draco data type {t}"))),
        };
        let raw = r.records(n, size)?;
        if a.data_type == FLOAT32 {
            a.values = raw
                .map(|v| f32::from_le_bytes(v.try_into().expect("4 bytes")))
                .collect();
        }
        return Ok(());
    }
    // Difference prediction: each value is stored as its difference from
    // the point before's, wrapped into the range the encoder saw.
    let prediction = r.u8()? as i8;
    let wrap = match prediction {
        -2 => false,
        0 => match r.u8()? as i8 {
            1 => true,
            t => {
                return Err(MeshError::Unsupported(format!(
                    "7.00 (Draco transform {t})"
                )));
            }
        },
        p => {
            return Err(MeshError::Unsupported(format!(
                "7.00 (Draco prediction {p})"
            )));
        }
    };
    let symbols = if r.u8()? > 0 {
        rans::symbols(r, n, a.components)?
    } else {
        let size = usize::from(r.u8()?);
        if !(1..=4).contains(&size) {
            return Err(MeshError::Damaged(format!("{size}-byte Draco values")));
        }
        r.records(n, size)?
            .map(|v| v.iter().rev().fold(0u32, |acc, &b| acc << 8 | u32::from(b)))
            .collect()
    };
    let corrections = symbols.into_iter().map(|s| {
        let half = (s >> 1) as i32;
        if s & 1 == 1 { -half - 1 } else { half }
    });
    a.integers = if wrap {
        let (min, max) = (r.i32()?, r.i32()?);
        if min > max {
            return Err(MeshError::Damaged(
                "a Draco wrap range runs backwards".into(),
            ));
        }
        let span = i64::from(max) - i64::from(min) + 1;
        let mut out: Vec<i32> = Vec::with_capacity(n);
        for (i, c) in corrections.enumerate() {
            let predicted = if i >= a.components {
                out[i - a.components]
            } else {
                0
            };
            let mut v = i64::from(predicted.clamp(min, max)) + i64::from(c);
            if v > i64::from(max) {
                v -= span;
            } else if v < i64::from(min) {
                v += span;
            }
            out.push(v as i32);
        }
        out
    } else {
        corrections.collect()
    };
    Ok(())
}

/// A quantized attribute's minimum per component, its range and its bit
/// count, then its values as floats (Draco's dequantizer, step for step).
fn dequantize(r: &mut Reader, a: &mut Attribute) -> Result<(), MeshError> {
    let min: Vec<f32> = (0..a.components)
        .map(|_| r.f32())
        .collect::<Result<_, _>>()?;
    let range = r.f32()?;
    let bits = r.u8()?;
    if !(1..=31).contains(&bits) {
        return Err(MeshError::Damaged(format!("{bits}-bit Draco quantization")));
    }
    let delta = range / ((1u32 << bits) - 1) as f32;
    a.values = (a.integers.iter().enumerate())
        .map(|(i, &q)| q as f32 * delta + min[i % a.components])
        .collect();
    a.integers = Vec::new();
    Ok(())
}

#[cfg(test)]
pub(super) mod tests;
