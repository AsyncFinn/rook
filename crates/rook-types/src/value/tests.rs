use super::*;

fn round_trip(variant: &Variant) {
    let value = Value::from_variant(variant.clone(), |_| Some(InstanceRef(7))).unwrap();
    let back = value.to_variant(|_| rbx_types::Ref::none());
    assert_eq!(&back, variant);
}

#[test]
fn lossless_round_trips() {
    round_trip(&Variant::Bool(true));
    round_trip(&Variant::Int32(-5));
    round_trip(&Variant::Int64(1 << 40));
    round_trip(&Variant::Float32(0.1));
    round_trip(&Variant::Float64(0.1));
    round_trip(&Variant::String("hello".into()));
    round_trip(&Variant::Enum(rbx_types::Enum::from_u32(3)));
    round_trip(&Variant::Vector3(rbx_types::Vector3::new(1.0, -2.0, 3.5)));
    round_trip(&Variant::CFrame(rbx_types::CFrame::new(
        rbx_types::Vector3::new(1.0, 2.0, 3.0),
        rbx_types::Matrix3::new(
            rbx_types::Vector3::new(0.0, -1.0, 0.0),
            rbx_types::Vector3::new(1.0, 0.0, 0.0),
            rbx_types::Vector3::new(0.0, 0.0, 1.0),
        ),
    )));
    round_trip(&Variant::OptionalCFrame(None));
    round_trip(&Variant::UDim2(rbx_types::UDim2::new(
        rbx_types::UDim::new(0.5, 10),
        rbx_types::UDim::new(1.0, -3),
    )));
    round_trip(&Variant::Color3(rbx_types::Color3::new(0.2, 0.4, 0.6)));
    round_trip(&Variant::Color3uint8(rbx_types::Color3uint8::new(1, 2, 3)));
    round_trip(&Variant::Ref(rbx_types::Ref::none()));
}

#[test]
fn refs_are_mapped() {
    let value = Value::from_variant(Variant::Ref(rbx_types::Ref::new()), |_| {
        Some(InstanceRef(42))
    })
    .unwrap();
    assert_eq!(value, Value::Ref(Some(InstanceRef(42))));
}
