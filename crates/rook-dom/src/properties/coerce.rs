//! Values stored in the property's declared type.

use super::*;

/// Store values in the property's declared type. Files and the database
/// sometimes hold an equivalent encoding (`Color3uint8` for a `Color3`
/// property, `Int32` for an `Int64` one).
pub(crate) fn coerce(ty: PropertyType, value: Value) -> Value {
    use rbx_types::VariantType as T;
    let PropertyType::Value(target) = ty else {
        return value;
    };
    match (target, &value) {
        (T::Color3, Value::Color3uint8(c)) => Value::Color3((*c).into()),
        (T::Float32, v) if !matches!(v, Value::Float32(_)) => {
            v.as_number().map_or(value, |n| Value::Float32(n as f32))
        }
        (T::Float64, v) if !matches!(v, Value::Float64(_)) => {
            v.as_number().map_or(value, Value::Float64)
        }
        (T::Int32, v) if !matches!(v, Value::Int32(_)) => {
            v.as_number().map_or(value, |n| Value::Int32(n as i32))
        }
        (T::Int64, v) if !matches!(v, Value::Int64(_)) => {
            v.as_number().map_or(value, |n| Value::Int64(n as i64))
        }
        _ => value,
    }
}
