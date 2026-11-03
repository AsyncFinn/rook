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

/// A script switching an effect off, changing its color, emitting and
/// moving its part changes what's drawn; the Beam follows its
/// Attachments.
#[test]
fn scripts_redraw_effects() {
    let mut dm = DataModel::load_place(PLACE.as_bytes(), &Options::default()).unwrap();
    dm.start();
    dm.take_render_changes();
    let content = Arc::new(Resolver::new(rook_engine::settings::content_config(false)));
    let mut cache = SceneCache::new(
        &dm.runtime().world().dom,
        Floors::default(),
        content,
        Vec3::ZERO,
    );
    // Fire's two emitters, then Smoke's, then the ParticleEmitter's.
    let e: Vec<&Emitter> = cache.scene.emitters.iter().flatten().collect();
    assert_eq!(e.len(), 4);
    assert!(e.iter().all(|e| e.enabled));
    assert_eq!(cache.scene.beams.len(), 1);
    assert_eq!(
        cache.scene.beams[0].as_ref().unwrap().ends[1].position.z,
        -8.0
    );
    dm.run_for(1.0);
    let changes = dm.take_render_changes();
    assert_ne!(
        cache.apply(&dm.runtime().world().dom, &changes),
        SceneChange::None
    );
    let e: Vec<&Emitter> = cache.scene.emitters.iter().flatten().collect();
    assert!(!e[0].enabled && !e[1].enabled && e[2].enabled);
    assert_eq!(e[2].look.color.at(0.0), [1.0, 0.0, 0.0]);
    assert_eq!(e[3].emitted, 7);
    assert!(
        e.iter().all(|e| e.position.x == 10.0),
        "{:?}",
        e[0].position
    );
    assert_eq!(
        cache.scene.beams[0].as_ref().unwrap().ends[0].position.x,
        10.0
    );
}

/// An effect coming and going touches only its own slots: the others keep
/// theirs (and their particles), and a freed slot is taken again.
#[test]
fn effects_come_and_go_slot_by_slot() {
    let place = PLACE.replace(
        "task.wait(0.5)\nlocal holder = workspace.Holder\nholder.Fire.Enabled = false",
        "task.wait(0.5)\nlocal holder = workspace.Holder\nholder.Fire:Destroy()\nInstance.new(\"Sparkles\", holder)\ndo return end",
    );
    let mut dm = DataModel::load_place(place.as_bytes(), &Options::default()).unwrap();
    dm.start();
    dm.take_render_changes();
    let content = Arc::new(Resolver::new(rook_engine::settings::content_config(false)));
    let mut cache = SceneCache::new(
        &dm.runtime().world().dom,
        Floors::default(),
        content,
        Vec3::ZERO,
    );
    cache.effects.take_changes();
    // Fire takes slots 0 and 1.
    dm.run_for(1.0);
    let changes = dm.take_render_changes();
    cache.apply(&dm.runtime().world().dom, &changes);
    let changed = cache.effects.take_changes();
    assert!(changed.emitters.iter().all(|&i| i < 2), "{changed:?}");
    assert!(changed.beams.is_empty(), "{changed:?}");
    assert_eq!(cache.scene.emitters.len(), 4);
    assert_eq!(cache.scene.emitters.iter().flatten().count(), 3);
}

const WIRE: &str = r#"<roblox version="4">
<Item class="Workspace" referent="W"><Properties><string name="Name">Workspace</string></Properties>
<Item class="Part" referent="A"><Properties><string name="Name">From</string><bool name="Anchored">true</bool>
<CoordinateFrame name="CFrame"><X>0</X><Y>20</Y><Z>0</Z><R00>1</R00><R01>0</R01><R02>0</R02><R10>0</R10><R11>1</R11><R12>0</R12><R20>0</R20><R21>0</R21><R22>1</R22></CoordinateFrame>
<Vector3 name="size"><X>2</X><Y>2</Y><Z>2</Z></Vector3></Properties></Item>
<Item class="Part" referent="B"><Properties><string name="Name">To</string><bool name="Anchored">true</bool>
<CoordinateFrame name="CFrame"><X>0</X><Y>20</Y><Z>30</Z><R00>1</R00><R01>0</R01><R02>0</R02><R10>0</R10><R11>1</R11><R12>0</R12><R20>0</R20><R21>0</R21><R22>1</R22></CoordinateFrame>
<Vector3 name="size"><X>2</X><Y>2</Y><Z>2</Z></Vector3></Properties></Item>
<Item class="Part" referent="F"><Properties><string name="Name">Floor</string><bool name="Anchored">true</bool>
<CoordinateFrame name="CFrame"><X>0</X><Y>5</Y><Z>0</Z><R00>1</R00><R01>0</R01><R02>0</R02><R10>0</R10><R11>1</R11><R12>0</R12><R20>0</R20><R21>0</R21><R22>1</R22></CoordinateFrame>
<Vector3 name="size"><X>6</X><Y>2</Y><Z>6</Z></Vector3></Properties></Item>
<Item class="FloorWire" referent="FW"><Properties><string name="Name">Wire</string><Ref name="From">A</Ref><Ref name="To">B</Ref></Properties></Item>
</Item>
<Item class="ServerScriptService" referent="SS"><Properties><string name="Name">ServerScriptService</string></Properties>
<Item class="Script" referent="X"><Properties><string name="Name">Script</string><ProtectedString name="Source"><![CDATA[
local floor = workspace.Floor
task.wait(0.5)
floor.Position = Vector3.new(0, 8, 0)
task.wait(0.5)
floor.Size = Vector3.new(6, 6, 6)
task.wait(0.5)
floor:Destroy()
local low = Instance.new("Part")
low.Anchored = true
low.Size = Vector3.new(4, 2, 4)
low.Position = Vector3.new(0, 2, 0)
low.Parent = workspace
local aside = low:Clone()
aside.Position = Vector3.new(20, 4, 0)
aside.Parent = workspace
task.wait(0.5)
aside.Position = Vector3.new(0, 4, 0)
]]></ProtectedString></Properties></Item></Item>
</roblox>"#;

/// A FloorWire's wire runs on what is under where it drops, as that
/// changes: its floor moving or growing, going, and parts put or moved
/// there, without the wire or its parts changing.
#[test]
fn floor_wires_follow_their_floor() {
    let mut dm = DataModel::load_place(WIRE.as_bytes(), &Options::default()).unwrap();
    dm.start();
    let content = Arc::new(Resolver::new(rook_engine::settings::content_config(false)));
    let mut cache = SceneCache::for_data_model(&mut dm, &content, Vec3::ZERO);
    // Where the wire runs along the floor: its images' path, and the
    // lowest of its tubes.
    let runs = |cache: &SceneCache| {
        let path = &cache.scene.floor_wires[0].as_ref().unwrap().path;
        let tubes = (cache.scene.parts.iter())
            .filter(|p| {
                matches!(
                    p.shape,
                    DrawShape::Adorn(rook_render::AdornShape::Tube { .. })
                )
            })
            .map(|p| p.position.y)
            .reduce(f32::min)
            .unwrap();
        (path.iter().map(|p| p.y).reduce(f32::min).unwrap(), tubes)
    };
    let close = |(path, tubes): (f32, f32), y: f32, when: &str| {
        assert!((path - y).abs() < 1e-4, "{when}: images at {path}, not {y}");
        assert!(
            (tubes - y).abs() < 1e-4,
            "{when}: tubes at {tubes}, not {y}"
        );
    };
    close(runs(&cache), 6.2, "placed");
    let steps = [
        (9.2, "floor moved up"),
        (11.2, "floor grew"),
        (3.2, "floor gone, a lower part put there"),
        (5.2, "a part moved in above it"),
    ];
    for (y, when) in steps {
        dm.run_for(0.5);
        cache.apply_data_model(&mut dm);
        close(runs(&cache), y, when);
    }
}
