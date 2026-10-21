use super::*;
use rook_engine::{DataModel, Options};

const PLACE: &str = r#"<roblox version="4">
<Item class="Workspace" referent="W"><Properties><string name="Name">Workspace</string></Properties>
<Item class="Part" referent="P"><Properties><string name="Name">Holder</string><bool name="Anchored">true</bool></Properties>
<Item class="Fire" referent="F"><Properties><string name="Name">Fire</string></Properties></Item>
<Item class="Smoke" referent="S"><Properties><string name="Name">Smoke</string></Properties></Item>
<Item class="ParticleEmitter" referent="E"><Properties><string name="Name">Emitter</string></Properties></Item>
<Item class="Attachment" referent="A0"><Properties><string name="Name">A0</string></Properties></Item>
<Item class="Attachment" referent="A1"><Properties><string name="Name">A1</string>
<CoordinateFrame name="CFrame"><X>0</X><Y>0</Y><Z>-8</Z><R00>1</R00><R01>0</R01><R02>0</R02><R10>0</R10><R11>1</R11><R12>0</R12><R20>0</R20><R21>0</R21><R22>1</R22></CoordinateFrame>
</Properties></Item>
<Item class="Beam" referent="B"><Properties><string name="Name">Beam</string><Ref name="Attachment0">A0</Ref><Ref name="Attachment1">A1</Ref></Properties></Item>
</Item></Item>
<Item class="ServerScriptService" referent="SS"><Properties><string name="Name">ServerScriptService</string></Properties>
<Item class="Script" referent="X"><Properties><string name="Name">Script</string><ProtectedString name="Source"><![CDATA[
task.wait(0.5)
local holder = workspace.Holder
holder.Fire.Enabled = false
holder.Smoke.Color = Color3.new(1, 0, 0)
holder.Emitter:Emit(7)
holder.Position = Vector3.new(10, 0, 0)
]]></ProtectedString></Properties></Item></Item>
</roblox>"#;

/// A script switching an effect off, changing its colour, emitting and
/// moving its part changes what's drawn; the Beam follows its
/// Attachments.
#[test]
fn scripts_redraw_effects() {
    let mut dm = DataModel::load_place(PLACE.as_bytes(), &Options::default()).unwrap();
    dm.start();
    dm.take_render_changes();
    let content = Arc::new(Resolver::new(rook_engine::settings::content_config(false)));
    let mut cache = SceneCache::new(&dm.runtime().world().dom, content, Vec3::ZERO);
    // Fire's two emitters, then Smoke's, then the ParticleEmitter's.
    let e = &cache.scene.emitters;
    assert_eq!(e.len(), 4);
    assert!(e.iter().all(|e| e.enabled));
    assert_eq!(cache.scene.beams.len(), 1);
    assert_eq!(cache.scene.beams[0].ends[1].position.z, -8.0);
    dm.run_for(1.0);
    let changes = dm.take_render_changes();
    assert_ne!(
        cache.apply(&dm.runtime().world().dom, &changes),
        SceneChange::None
    );
    let e = &cache.scene.emitters;
    assert!(!e[0].enabled && !e[1].enabled && e[2].enabled);
    assert_eq!(e[2].look.color.at(0.0), [1.0, 0.0, 0.0]);
    assert_eq!(e[3].emitted, 7);
    assert!(
        e.iter().all(|e| e.position.x == 10.0),
        "{:?}",
        e[0].position
    );
    assert_eq!(cache.scene.beams[0].ends[0].position.x, 10.0);
}
