use super::*;

#[test]
fn indices_and_names() {
    for (i, m) in TerrainMaterial::ALL.iter().enumerate() {
        assert_eq!(*m as usize, i);
        assert_eq!(m.material().name(), m.name());
    }
    assert_eq!(TerrainMaterial::from_index(23), None);
    let legacy = MaterialSet::Legacy;
    // rbx_types' defaults are the legacy set's.
    assert_eq!(
        TerrainMaterial::Grass.color(None, legacy),
        Some(TerrainMaterials::Grass.default_color())
    );
    assert!(TerrainMaterial::Water.color(None, legacy).is_none());
}

/// Spot checks of Studio's tables, and the shapes measured in a legacy
/// place (docs/behavior/terrain/materials.md).
#[test]
fn looks() {
    use TerrainMaterial::*;
    let legacy = |m: TerrainMaterial| m.look(MaterialSet::Legacy);
    assert_eq!(legacy(Concrete).quantize, 0.5);
    assert_eq!(legacy(Pavement).quantize, 0.25);
    assert_eq!(legacy(Brick).cubify, 1.0);
    assert_eq!(legacy(Slate).shift, legacy(CrackedLava).shift);
    assert!(legacy(Brick).cube && legacy(Brick).edges.flat());
    assert_eq!(legacy(Grass).top.unwrap().pack, 7547336797);
    assert_eq!(legacy(Grass).top.unwrap().studs(), 16.0);
    assert!(legacy(Air).top.is_none() && legacy(Water).side.is_none());
    let modern = Concrete.look(MaterialSet::Materials2022);
    assert_eq!(modern.quantize, 1.0);
}

/// The tints' shapes measured in Studio (docs/behavior/terrain/materials.md).
#[test]
fn tints() {
    use TerrainMaterial::*;
    let rgb = |r, g, b| Color3uint8::new(r, g, b);
    let m22 = MaterialSet::Materials2022;
    // White keeps the texture's gray, and gray dims it.
    assert_eq!(Grass.tint(rgb(255, 255, 255), m22), [1.0; 3]);
    assert!(
        Sand.tint(rgb(128, 128, 128), m22)
            .iter()
            .all(|&x| (x - 0.502).abs() < 0.001)
    );
    // Red turns magenta on Grass, and teal on Basalt.
    let [r, g, b] = Grass.tint(rgb(255, 0, 0), m22);
    assert!(r > b && b > g);
    let [r, g, b] = Basalt.tint(rgb(255, 0, 0), m22);
    assert!(g > r && b > r);
    // Rock's chroma tops out: pink and red look alike.
    assert_eq!(
        Rock.tint(rgb(255, 128, 128), m22),
        Rock.tint(rgb(255, 0, 0), m22)
    );
    // Basalt is as bright in its default color as in white.
    let luma = |[r, g, b]: [f32; 3]| (r + 2.0 * g + b) / 4.0;
    assert_eq!(luma(Basalt.tint(rgb(75, 74, 74), m22)), 0.6);
    assert_eq!(luma(Basalt.tint(rgb(255, 255, 255), m22)), 0.6);
    assert_eq!(Water.tint(rgb(255, 0, 0), m22), [1.0, 0.0, 0.0]);
    // The legacy set's textures draw as they are in its default colors.
    let legacy = MaterialSet::Legacy;
    assert_eq!(Sandstone.tint(rgb(137, 90, 71), legacy), [1.0; 3]);
}
