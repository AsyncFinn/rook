use super::*;

#[test]
fn values_and_names_round_trip() {
    for m in Material::ALL {
        assert_eq!(Material::from_value(m.value()), Some(m));
        assert_eq!(Material::from_name(m.name()), Some(m));
    }
    assert_eq!(Material::of_part(None), Material::Plastic);
    assert_eq!(Material::of_part(Some(9999)), Material::Plastic);
    assert_eq!(Material::of_part(Some(288)), Material::Neon);
    assert!(Material::Plastic.shows_surfaces() && !Material::SmoothPlastic.shows_surfaces());
}
