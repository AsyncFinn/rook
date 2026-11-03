//! MaterialVariants and MaterialService overrides redraw only the parts
//! (and terrain) that can draw them.

use super::*;

/// A variant's change redraws the parts of its base material asking for
/// it by name; once MaterialService's override names it, the plain parts
/// of that material and the terrain too; the same when it leaves. Never
/// the whole scene.
#[test]
fn variant_changes_redraw_only_the_parts_drawing_them() {
    let mut s = Server::new(&Options::default()).unwrap();
    // Scripts can't set a variant's BaseMaterial; places come with theirs.
    let service = service(&s, "MaterialService");
    let mossy = add_under(&s, service, "MaterialVariant", "Mossy", "");
    let brick = Value::Enum(rook_types::Material::Brick.value());
    (s.runtime.world_mut().dom)
        .set_property_by_name(mossy, "BaseMaterial", brick)
        .unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Variants",
        r#"
        local ms = game:GetService("MaterialService")
        local v = ms.Mossy
        local function part(name, material, variant)
            local p = Instance.new("Part") p.Name = name p.Anchored = true
            p.Material = material p.MaterialVariant = variant p.Parent = workspace
        end
        part("Named", Enum.Material.Brick, "Mossy")
        part("Plain", Enum.Material.Brick, "")
        part("Plastic", Enum.Material.Plastic, "")
        part("OtherBase", Enum.Material.Plastic, "Mossy")
        task.wait(0.5)
        v.StudsPerTile = 4
        task.wait(0.5)
        ms:SetBaseMaterialOverride(Enum.Material.Brick, "Mossy")
        task.wait(0.5)
        v.StudsPerTile = 8
        task.wait(0.5)
        v.Parent = nil
        "#,
    );
    s.start();
    s.run_for(0.2);
    s.take_render_changes();
    let step = |s: &mut Server| {
        s.run_for(0.5);
        let changes = s.take_render_changes();
        assert!(!changes.full);
        let dom = &s.runtime().world().dom;
        let mut names: Vec<_> = (changes.parts.iter())
            .map(|&p| dom.name(p).to_owned())
            .collect();
        names.sort();
        (names, changes.terrain)
    };
    assert_eq!(step(&mut s), (vec!["Named".to_owned()], false));
    assert_eq!(step(&mut s), (vec!["Plain".to_owned()], true));
    let both = (vec!["Named".to_owned(), "Plain".to_owned()], true);
    assert_eq!(step(&mut s), both);
    // Leaving MaterialService, it's drawn by none of them.
    assert_eq!(step(&mut s), both);
}
