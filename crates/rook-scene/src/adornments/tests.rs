use super::*;
use rook_engine::{DataModel, Options};

const PLACE: &str = r#"<roblox version="4">
<Item class="Workspace" referent="W"><Properties><string name="Name">Workspace</string></Properties>
<Item class="Model" referent="M"><Properties><string name="Name">M</string></Properties>
<Item class="Part" referent="A"><Properties><string name="Name">A</string><bool name="Anchored">true</bool></Properties></Item>
<Item class="Part" referent="B"><Properties><string name="Name">B</string><bool name="Anchored">true</bool></Properties></Item>
<Item class="Highlight" referent="H"><Properties><string name="Name">Glow</string></Properties></Item>
<Item class="SelectionBox" referent="S"><Properties><string name="Name">Box</string><Ref name="Adornee">A</Ref></Properties></Item>
</Item></Item>
<Item class="ServerScriptService" referent="SS"><Properties><string name="Name">ServerScriptService</string></Properties>
<Item class="Script" referent="X"><Properties><string name="Name">Script</string><ProtectedString name="Source"><![CDATA[
task.wait(0.5)
workspace.M.Glow.FillColor = Color3.new(0, 1, 0)
workspace.M.Box.Color3 = Color3.new(1, 0, 0)
task.wait(1)
workspace.M.Glow.Enabled = false
]]></ProtectedString></Properties></Item></Item>
</roblox>"#;

/// A Highlight's look and an adornment change without the parts they're
/// on changing: the look in one table, the box as its own entry. Which
/// Highlight covers a part changes only that part's covering.
#[test]
fn adornments_and_highlights_change_alone() {
    let mut dm = DataModel::load_place(PLACE.as_bytes(), &Options::default()).unwrap();
    dm.start();
    dm.take_render_changes();
    let content = Arc::new(Resolver::new(rook_engine::settings::content_config(false)));
    let (mut cache, [a, b, glow]) = {
        let world = dm.runtime().world();
        let dom = &world.dom;
        let cache = SceneCache::new(dom, Floors::default(), content, Vec3::ZERO);
        let ws = dom.find_service("Workspace").unwrap();
        let m = dom.find_first_child(ws, "M", false).unwrap();
        let ids = ["A", "B", "Glow"].map(|n| dom.find_first_child(m, n, false).unwrap());
        (cache, ids)
    };
    let key = glow.to_ref().0;
    let [ia, ib] = [a, b].map(|p| cache.index[&p]);
    let boxes = cache.adornment_layers[&a].clone();
    assert!(!boxes.is_empty());
    assert_eq!(cache.scene.parts[ia].highlight, Some(key));
    assert_eq!(cache.scene.parts[ib].highlight, Some(key));

    dm.run_for(1.0);
    let changes = dm.take_render_changes();
    let SceneChange::Parts {
        changed,
        highlights,
        ..
    } = cache.apply(&dm.runtime().world().dom, &changes)
    else {
        panic!("nothing changed");
    };
    assert!(highlights);
    assert_eq!(cache.scene.highlights[&key].fill[..3], [0.0, 1.0, 0.0]);
    assert!(
        !changed.contains(&ia) && !changed.contains(&ib),
        "{changed:?}"
    );
    assert!(boxes.iter().all(|i| changed.contains(i)));
    assert_eq!(cache.scene.parts[boxes[0]].color, [1.0, 0.0, 0.0]);

    dm.run_for(1.0);
    let changes = dm.take_render_changes();
    let SceneChange::Parts { changed, .. } = cache.apply(&dm.runtime().world().dom, &changes)
    else {
        panic!("nothing changed");
    };
    assert!(changed.contains(&ia) && changed.contains(&ib));
    assert_eq!(cache.scene.parts[ia].highlight, None);
}
