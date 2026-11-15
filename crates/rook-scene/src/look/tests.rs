use rook_types::{Material, MeshType, Value, Vector3};

use super::*;
use crate::content::pending::Settled;

/// A dom with a Marble part, and what `part` makes of it.
fn drawn_material(mesh: impl FnOnce(&mut Dom, InstanceId)) -> Material {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let part = dom.create_by_name("Part").unwrap();
    dom.set_parent(part, Some(ws)).unwrap();
    let marble = Value::Enum(Material::Marble.value());
    dom.set_property_by_name(part, "Material", marble).unwrap();
    mesh(&mut dom, part);
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, Floors::default(), content, Vec3::ZERO);
    cache.part(&dom, part).unwrap().part.material
}

fn add_mesh(dom: &mut Dom, part: InstanceId, class: &str, mesh_type: Option<MeshType>, url: &str) {
    let mesh = dom.create_by_name(class).unwrap();
    dom.set_parent(mesh, Some(part)).unwrap();
    if let Some(mesh_type) = mesh_type {
        dom.set_property_by_name(mesh, "MeshType", Value::Enum(mesh_type as u32))
            .unwrap();
    }
    if !url.is_empty() {
        let content = rook_types::Content::from_uri(url.to_owned());
        dom.set_property_by_name(mesh, "MeshContent", Value::Content(content))
            .unwrap();
    }
}

/// File meshes, the empty-`MeshId` box and the Head leave the part's
/// material out; the primitive shapes and a part without a mesh draw it.
#[test]
fn only_primitive_meshes_draw_their_parts_material() {
    use MeshType::*;
    let file = "rbxasset://rook/none.mesh";
    let plain = [
        ("SpecialMesh", Some(FileMesh), file),
        ("SpecialMesh", Some(FileMesh), ""),
        ("SpecialMesh", Some(Head), ""),
        ("FileMesh", None, file),
    ];
    for (class, mesh_type, url) in plain {
        let drawn = drawn_material(|dom, part| add_mesh(dom, part, class, mesh_type, url));
        assert_eq!(
            drawn,
            Material::SmoothPlastic,
            "{class} {mesh_type:?} {url}"
        );
    }
    for mesh_type in [Sphere, Brick, Cylinder, Wedge] {
        let drawn = drawn_material(|dom, part| {
            add_mesh(dom, part, "SpecialMesh", Some(mesh_type), "");
        });
        assert_eq!(drawn, Material::Marble, "{mesh_type:?}");
    }
    assert_eq!(drawn_material(|_, _| ()), Material::Marble);
}

#[test]
fn a_mesh_parts_double_sided_reaches_its_mesh() {
    let drawn = |double_sided: bool| {
        let mut dom = Dom::new();
        let ws = dom.get_service("Workspace").unwrap();
        let part = dom.create_by_name("MeshPart").unwrap();
        dom.set_parent(part, Some(ws)).unwrap();
        let url = rook_types::Content::from_uri("rbxasset://rook/none.mesh".to_owned());
        dom.set_property_by_name(part, "MeshContent", Value::Content(url))
            .unwrap();
        let one = Vector3::new(1.0, 1.0, 1.0);
        dom.set_property_by_name(part, "InitialSize", Value::Vector3(one))
            .unwrap();
        dom.set_property_by_name(part, "DoubleSided", Value::Bool(double_sided))
            .unwrap();
        let content = Arc::new(Resolver::new(rook_content::Config::empty()));
        let mut cache = SceneCache::new(&dom, Floors::default(), content, Vec3::ZERO);
        let settled = Settled::File(rook_render::MeshHash([1; 32]));
        cache.shown.insert(
            part,
            Shown {
                mesh: part,
                settled,
            },
        );
        cache.part(&dom, part).unwrap().part.double_sided
    };
    assert!(drawn(true));
    assert!(!drawn(false));
}

/// A part is drawn in the world of its nearest ViewportFrame, else in
/// Workspace's if it's in Workspace; anywhere else, not at all
/// (docs/behavior/gui/viewport-frames.md).
#[test]
fn parts_are_drawn_in_their_nearest_frames_world() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let gui = dom.get_service("StarterGui").unwrap();
    let make = |dom: &mut Dom, class: &str, parent: InstanceId| {
        let id = dom.create_by_name(class).unwrap();
        dom.set_parent(id, Some(parent)).unwrap();
        id
    };
    let outer = make(&mut dom, "ViewportFrame", gui);
    let model = make(&mut dom, "WorldModel", outer);
    let framed = make(&mut dom, "Part", model);
    let inner = make(&mut dom, "ViewportFrame", model);
    let nested = make(&mut dom, "Part", inner);
    let in_workspace = make(&mut dom, "ViewportFrame", ws);
    let walled = make(&mut dom, "Part", in_workspace);
    let plain = make(&mut dom, "Part", ws);
    let stored = make(&mut dom, "Part", gui);
    let loose = dom.create_by_name("Part").unwrap();
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, Floors::default(), content, Vec3::ZERO);
    let mut space = |id| cache.part(&dom, id).map(|d| d.part.space);
    let key = |frame: InstanceId| Space::Viewport(frame.to_ref().0);
    assert_eq!(space(framed), Some(key(outer)));
    assert_eq!(space(nested), Some(key(inner)));
    assert_eq!(space(walled), Some(key(in_workspace)));
    assert_eq!(space(plain), Some(Space::Workspace));
    assert_eq!(space(stored), None);
    assert_eq!(space(loose), None);
}
