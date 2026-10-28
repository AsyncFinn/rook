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
    let legacy = |m: TerrainMaterial| m.look();
    assert_eq!(legacy(Concrete).quantize, 0.5);
    assert_eq!(legacy(Pavement).quantize, 0.25);
    assert_eq!(legacy(Brick).cubify, 1.0);
    assert_eq!(legacy(Slate).shift, legacy(CrackedLava).shift);
    assert!(legacy(Brick).cube && legacy(Brick).edges.flat());
    assert_eq!(legacy(Grass).top.unwrap().pack, 7547336797);
    assert_eq!(legacy(Grass).top.unwrap().studs(), 16.0);
    assert!(legacy(Air).top.is_none() && legacy(Water).side.is_none());
}

/// Studio's recoloring, as measured on every material in both sets
/// (docs/behavior/terrain/materials.md#colors).
#[test]
fn recolors() {
    use TerrainMaterial::*;
    let rgb = |r, g, b| Color3uint8::new(r, g, b);
    let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4);
    // The legacy default color leaves the texture as it is.
    let texel = [0.3, 0.33, 0.15];
    assert!(near(Grass.recolor(rgb(106, 127, 63)).apply(texel), texel));
    assert!(near(
        Sandstone.recolor(rgb(137, 90, 71)).apply(texel),
        texel
    ));
    // White keeps only the luma, brightened at most twice.
    let luma = (0.3 + 0.66 + 0.15) / 4.0;
    assert!(near(
        Grass.recolor(rgb(255, 255, 255)).apply(texel),
        [luma * 2.0; 3]
    ));
    // Mud's dark default: gray 128 is already as bright as white.
    assert_eq!(
        Mud.recolor(rgb(128, 128, 128)).gain,
        Mud.recolor(rgb(255, 255, 255)).gain
    );
    // Red turns a green texel magenta on Grass and Basalt's beige teal.
    let [r, g, b] = Grass.recolor(rgb(255, 0, 0)).apply(texel);
    assert!(r > b && b > g);
    let [r, g, b] = Basalt.recolor(rgb(255, 0, 0)).apply([0.18, 0.18, 0.16]);
    assert!(g > r && b > r);
    // Strong chroma passes 1 in a channel, as Studio's YCoCg does.
    assert!(Sand.recolor(rgb(255, 0, 0)).apply([0.6, 0.5, 0.35])[0] > 1.0);
    assert_eq!(Water.recolor(rgb(255, 0, 0)), Recolor::NONE);
}
