//! Unions in places, for the scan's counts (M4 step 6): where each keeps
//! its geometry (`MeshData` in the file, or behind its `AssetId`), which
//! `MeshData` and `PhysicsData` forms appear, and what decides how it
//! looks (`UsePartColor`, a Size other than `InitialSize`).

use rbx_types::Variant;
use rook_content::decode::mesh::union::MeshDataKind;

/// The union classes.
pub const UNIONS: [&str; 4] = [
    "UnionOperation",
    "NegateOperation",
    "IntersectOperation",
    "PartOperation",
];

/// What a union instance is made of (none for other classes).
pub fn union_kinds(dom: &rbx_dom_weak::WeakDom, inst: &rbx_dom_weak::Instance) -> Vec<String> {
    let class = inst.class.as_str();
    if !UNIONS.contains(&class) {
        return Vec::new();
    }
    let prop = |name: &str| inst.properties.get(&name.into());
    let bytes = |name: &str| -> Option<&[u8]> {
        match prop(name)? {
            Variant::BinaryString(b) => Some(b.as_ref()),
            Variant::SharedString(s) => Some(s.data()),
            _ => None,
        }
    };
    let asset = ["AssetId", "Content"].iter().any(|k| match prop(k) {
        Some(Variant::ContentId(c)) => !c.as_str().is_empty(),
        Some(Variant::Content(c)) => c.as_uri().is_some_and(|u| !u.is_empty()),
        _ => false,
    });
    let mesh = bytes("MeshData").map_or(MeshDataKind::Empty, MeshDataKind::of);
    let mesh2 = bytes("MeshData2").map_or(MeshDataKind::Empty, MeshDataKind::of);
    let mut out = vec![
        format!(
            "{class} {}",
            if asset {
                "with AssetId"
            } else {
                "without AssetId"
            }
        ),
        format!("{class} MeshData {}", mesh.name()),
        format!(
            "{class} PhysicsData {}",
            physics_kind(bytes("PhysicsData").unwrap_or_default())
        ),
    ];
    let geometry = |k: MeshDataKind| matches!(k, MeshDataKind::Csgmdl(_));
    if geometry(mesh) || geometry(mesh2) {
        out.push(format!(
            "{class} geometry in the file, {}",
            if asset { "with AssetId" } else { "only" }
        ));
    }
    // Newer files also hold `MeshData2`, a shared string.
    if let Some(data) = bytes("MeshData2") {
        out.push(format!(
            "{class} MeshData2 {}",
            MeshDataKind::of(data).name()
        ));
    }
    if bytes("ChildData").is_some_and(|b| !b.is_empty()) {
        out.push(format!("{class} with ChildData"));
    }
    if let Some(Variant::Bool(true)) = prop("UsePartColor") {
        out.push(format!("{class} UsePartColor true"));
    }
    match (prop("size").or(prop("Size")), prop("InitialSize")) {
        (Some(Variant::Vector3(s)), Some(Variant::Vector3(i))) if s != i => {
            out.push(format!("{class} resized (Size not InitialSize)"));
        }
        (_, None) => out.push(format!("{class} without InitialSize")),
        _ => {}
    }
    if let Some(parent) = dom.get_by_ref(inst.parent())
        && UNIONS.contains(&parent.class.as_str())
    {
        out.push(format!("{class} under {}", parent.class));
    }
    out
}

/// `PhysicsData`'s form by its first bytes (not decoded: collision hulls
/// are M15's).
fn physics_kind(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        "empty".into()
    } else if bytes.starts_with(b"CSGK") {
        "CSGK".into()
    } else if bytes.starts_with(b"CSGPHS\0\0\0\0BLOCK") {
        "CSGPHS block".into()
    } else if let Some(&v) = bytes.strip_prefix(b"CSGPHS").and_then(|b| b.first()) {
        format!("CSGPHS{v}")
    } else {
        "unknown".into()
    }
}
