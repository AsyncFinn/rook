//! Reading a property's default from the text the API dump prints, such as
//! `0, 1, 0` for a Vector3 or `0 1 1 1 0 1 1 1 1 0 ` for a ColorSequence
//! (time, r, g, b and envelope per keypoint).
//!
//! Types the dump prints ambiguously (Content, Rect, Instance) and the
//! dump's placeholders (`__api_dump_class_not_creatable__`) give no
//! default: `rook-tools studio-defaults` measures those.

use std::collections::HashMap;

use rbx_reflection::DataType;
use rbx_types::{
    CFrame, Color3, ColorSequence, ColorSequenceKeypoint, Enum, Matrix3, NumberRange,
    NumberSequence, NumberSequenceKeypoint, UDim, UDim2, Variant, VariantType, Vector2, Vector3,
};

use super::dump;

pub(super) fn parse(
    text: &str,
    ty: &DataType,
    enums: &HashMap<&str, &dump::Enum>,
) -> Option<Variant> {
    if text.starts_with("__api_dump_") {
        return None;
    }
    let ty = match ty {
        DataType::Enum(name) => {
            let item = enums.get(name)?.items.iter().find(|i| i.name == text)?;
            return Some(Variant::Enum(Enum::from_u32(item.value)));
        }
        DataType::Value(ty) => *ty,
        _ => return None,
    };
    Some(match ty {
        VariantType::Bool => Variant::Bool(text.parse().ok()?),
        VariantType::Int32 => Variant::Int32(text.parse().ok()?),
        VariantType::Int64 => Variant::Int64(text.parse().ok()?),
        VariantType::Float32 => Variant::Float32(finite(text)? as f32),
        VariantType::Float64 => Variant::Float64(finite(text)?),
        VariantType::String => Variant::String(text.to_owned()),
        VariantType::Vector2 => match numbers(text, ',')?[..] {
            [x, y] => Variant::Vector2(Vector2::new(x, y)),
            _ => return None,
        },
        VariantType::Vector3 => Variant::Vector3(vector3(&numbers(text, ',')?)?),
        VariantType::Color3 => match numbers(text, ',')?[..] {
            [r, g, b] => Variant::Color3(Color3::new(r, g, b)),
            _ => return None,
        },
        VariantType::CFrame => {
            let n = numbers(text, ',')?;
            if n.len() != 12 {
                return None;
            }
            let rows = Matrix3::new(vector3(&n[3..6])?, vector3(&n[6..9])?, vector3(&n[9..])?);
            Variant::CFrame(CFrame::new(vector3(&n[..3])?, rows))
        }
        VariantType::UDim2 => {
            let flat = text.replace(['{', '}'], "");
            match numbers(&flat, ',')?[..] {
                [xs, xo, ys, yo] => Variant::UDim2(UDim2::new(
                    UDim::new(xs, xo as i32),
                    UDim::new(ys, yo as i32),
                )),
                _ => return None,
            }
        }
        VariantType::NumberRange => match numbers(text, ' ')?[..] {
            [min, max] => Variant::NumberRange(NumberRange::new(min, max)),
            _ => return None,
        },
        VariantType::NumberSequence => {
            let n = numbers(text, ' ')?;
            if n.is_empty() || n.len() % 3 != 0 {
                return None;
            }
            let keypoints = n
                .chunks(3)
                .map(|k| NumberSequenceKeypoint::new(k[0], k[1], k[2]))
                .collect();
            Variant::NumberSequence(NumberSequence { keypoints })
        }
        VariantType::ColorSequence => {
            let n = numbers(text, ' ')?;
            if n.is_empty() || n.len() % 5 != 0 {
                return None;
            }
            let keypoints = n
                .chunks(5)
                .map(|k| ColorSequenceKeypoint::new(k[0], Color3::new(k[1], k[2], k[3])))
                .collect();
            Variant::ColorSequence(ColorSequence { keypoints })
        }
        _ => return None,
    })
}

/// JSON can't hold infinities, so those are left to the measured table.
fn finite(text: &str) -> Option<f64> {
    text.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn numbers(text: &str, sep: char) -> Option<Vec<f32>> {
    text.split(sep)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| finite(s).map(|v| v as f32))
        .collect()
}

fn vector3(n: &[f32]) -> Option<Vector3> {
    match n {
        [x, y, z] => Some(Vector3::new(*x, *y, *z)),
        _ => None,
    }
}
