//! Content references in places: which properties hold them and what
//! shape their URLs take, so the content resolver's edge cases are decided
//! by what the archive uses.

/// The shape of a content URL: digit runs become `#`, `rbxasset://` paths
/// keep their first folder and extension, query values are masked and
/// keys keep their case. `(empty)` for an empty value.
pub fn shape(url: &str) -> String {
    let t = url.trim();
    if t.is_empty() {
        return if url.is_empty() {
            "(empty)".into()
        } else {
            "(whitespace)".into()
        };
    }
    let padded = if t.len() == url.len() {
        ""
    } else {
        "(padded) "
    };
    let Some(at) = t.find("://") else {
        let kind = if t.bytes().all(|b| b.is_ascii_digit()) {
            "#".to_owned()
        } else {
            format!("(no scheme) {}", truncate(&mask(t), 40))
        };
        return format!("{padded}{kind}");
    };
    let (scheme, rest) = (&t[..at], &t[at + 3..]);
    let body = match scheme.to_ascii_lowercase().as_str() {
        "rbxasset" => asset_path(rest),
        "http" | "https" => {
            let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
            let mut s = mask(path);
            if rest.contains('?') {
                s.push('?');
                s.push_str(&query_keys(query));
            }
            s
        }
        "rbxthumb" => query_keys(rest),
        _ => truncate(&mask(rest), 40),
    };
    format!("{padded}{scheme}://{body}")
}

/// `textures/face.png` → `textures/*.png`; `fonts\head.mesh` keeps its
/// backslash so it shows up separately.
fn asset_path(path: &str) -> String {
    let split = path.find(['/', '\\']);
    let folder = split.map_or("", |i| &path[..=i]);
    let ext = path
        .rsplit_once('.')
        .filter(|(_, e)| !e.contains(['/', '\\']))
        .map_or(String::new(), |(_, e)| format!(".{e}"));
    let rest = if split.is_some() { "*" } else { "" };
    format!("{folder}{rest}{ext}")
}

/// `id=123&w=4` → `id=#&w=#`; a non-numeric value becomes `*`, a missing
/// one stays empty (a script concatenating the id on).
fn query_keys(query: &str) -> String {
    query
        .split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, "")) => format!("{k}="),
            Some((k, v)) if v.bytes().all(|b| b.is_ascii_digit()) => format!("{k}=#"),
            Some((k, _)) => format!("{k}=*"),
            None => pair.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn mask(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut digit = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            if !digit {
                out.push('#');
            }
            digit = true;
        } else {
            out.push(c);
            digit = false;
        }
    }
    out
}

fn truncate(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_owned(),
    }
}

/// Content URLs written as string literals in a script's source: every
/// `scheme://...` with a content scheme, or an http(s) URL on a roblox.com
/// host, up to the closing quote. A URL a script builds by concatenation
/// (`"...?id=" .. id`) ends at its quote, so its id is empty.
pub fn urls_in_source(source: &str) -> impl Iterator<Item = &str> {
    source.match_indices("://").filter_map(move |(at, _)| {
        let bytes = source.as_bytes();
        let start = bytes[..at]
            .iter()
            .rposition(|b| !b.is_ascii_alphabetic())
            .map_or(0, |i| i + 1);
        let end = bytes[at..]
            .iter()
            .position(|b| matches!(b, b'"' | b'\'' | b'\n' | b' ' | b']' | b')'))
            .map_or(source.len(), |i| at + i);
        let url = source.get(start..end)?;
        let scheme = url[..at - start].to_ascii_lowercase();
        let wanted = match scheme.as_str() {
            "rbxasset" | "rbxassetid" | "rbxthumb" | "rbxhttp" => true,
            "http" | "https" => {
                let host = url[at - start + 3..].split(['/', '?']).next().unwrap_or("");
                host.to_ascii_lowercase().contains("roblox.com")
            }
            _ => false,
        };
        wanted.then_some(url)
    })
}

/// What `rook-content` makes of a reference, for the scan's counts:
/// `asset`, `asset+version`, `asset version id`, `thumbnail`,
/// `rbxasset: install` (found in the user's install), `rbxasset: legacy
/// asset` (an old path that stands for an asset), `rbxasset: outside
/// content` (`../` paths), `rbxasset: missing`, or `no content`. The
/// second value is the path of a missing `rbxasset://` file, lowercased.
pub fn classify(url: &str) -> (&'static str, Option<String>) {
    use rook_content::{ContentRef, defaults};
    use std::collections::HashMap;
    use std::sync::{LazyLock, Mutex};

    static RESOLVER: LazyLock<rook_content::Resolver> = LazyLock::new(|| {
        let mut config = rook_content::Config::from_env(false);
        config.cache_dir = None;
        rook_content::Resolver::new(config)
    });
    static FOUND: LazyLock<Mutex<HashMap<String, bool>>> = LazyLock::new(Default::default);

    let kind = match ContentRef::parse(url) {
        None => "no content",
        Some(ContentRef::Asset { version: None, .. }) => "asset",
        Some(ContentRef::Asset { .. }) => "asset+version",
        Some(ContentRef::AssetVersion(_)) => "asset version id",
        Some(ContentRef::Thumbnail(_)) => "thumbnail",
        Some(ContentRef::Material(..)) => "material",
        Some(ContentRef::File(path)) => {
            let key = path.to_ascii_lowercase();
            if key.split('/').any(|p| p == "..") {
                return ("rbxasset: outside content", Some(key));
            }
            let found = *FOUND
                .lock()
                .expect("lookup memo")
                .entry(key.clone())
                .or_insert_with(|| {
                    RESOLVER
                        .locate_local(&ContentRef::File(path.clone()))
                        .is_some()
                });
            if found {
                "rbxasset: install"
            } else if defaults::legacy_asset(&path).is_some() {
                return ("rbxasset: legacy asset", Some(key));
            } else {
                return ("rbxasset: missing", Some(key));
            }
        }
    };
    (kind, None)
}

/// What a mesh instance is, for the scan's mesh counts (none for other
/// classes): its class and, for a SpecialMesh, whether its `MeshType` is
/// FileMesh, with or without a `MeshId`; for a MeshPart, also whether it
/// saved `InitialSize` (what scripts read as `MeshSize`) and its
/// `CollisionFidelity`; and the class of the part a mesh is under when that
/// isn't a plain part.
pub fn mesh_kinds(dom: &rbx_dom_weak::WeakDom, inst: &rbx_dom_weak::Instance) -> Vec<String> {
    use rbx_types::Variant;
    let class = inst.class.as_str();
    let prop = |name: &str| inst.properties.get(&name.into());
    let mesh_id = ["MeshContent", "MeshId", "MeshID"]
        .iter()
        .filter_map(|k| match prop(k)? {
            Variant::ContentId(c) => Some(c.as_str().to_owned()),
            Variant::Content(c) => c.as_uri().map(str::to_owned),
            _ => None,
        })
        .find(|s| !s.is_empty());
    let id = if mesh_id.is_some() {
        "with MeshId"
    } else {
        "empty MeshId"
    };
    let mut out = Vec::new();
    match class {
        "SpecialMesh" => {
            let mesh_type = match prop("MeshType") {
                Some(Variant::Enum(e)) => e.to_u32(),
                _ => 0,
            };
            if mesh_type == 5 {
                out.push(format!("SpecialMesh FileMesh {id}"));
            } else {
                out.push(format!("SpecialMesh type {mesh_type} {id}"));
            }
        }
        "FileMesh" | "CharacterMesh" => out.push(format!("{class} {id}")),
        "MeshPart" => {
            out.push(format!("MeshPart {id}"));
            if prop("InitialSize").is_none() {
                out.push("MeshPart without InitialSize".into());
            }
            if prop("MeshID").is_some() {
                out.push("MeshPart saved as MeshID".into());
            }
            if let Some(Variant::Enum(e)) = prop("CollisionFidelity") {
                out.push(format!("MeshPart CollisionFidelity {}", e.to_u32()));
            }
        }
        _ => {}
    }
    if matches!(
        class,
        "SpecialMesh" | "FileMesh" | "BlockMesh" | "CylinderMesh"
    ) && let Some(parent) = dom.get_by_ref(inst.parent())
        && matches!(
            parent.class.as_str(),
            "MeshPart" | "UnionOperation" | "NegateOperation" | "TrussPart" | "PartOperation"
        )
    {
        out.push(format!("{class} under {}", parent.class));
    }
    out
}

/// What decides how an instance looks beyond its shape, for the scan's
/// appearance counts (M4 step 4): a part's `Material` and each face's
/// surface type (counted per face); a Decal's or Texture's face, tint,
/// transparency and `ZIndex`, a Texture's tiling, and whether its part is
/// invisible; a mesh's `VertexColor` and `TextureId`, and MeshPart
/// `TextureID`.
pub fn looks(dom: &rbx_dom_weak::WeakDom, inst: &rbx_dom_weak::Instance) -> Vec<String> {
    use rbx_types::Variant;
    let db = rook_reflection::database();
    let item = |enum_name: &str, value: u32| {
        db.enum_by_name(enum_name)
            .and_then(|e| e.items.iter().find(|(_, v)| *v == value))
            .map_or_else(|| format!("#{value}"), |(name, _)| (*name).to_owned())
    };
    let prop = |name: &str| inst.properties.get(&name.into());
    let number = |name: &str| match prop(name) {
        Some(Variant::Float32(f)) => Some(f64::from(*f)),
        Some(Variant::Float64(f)) => Some(*f),
        Some(Variant::Int32(i)) => Some(f64::from(*i)),
        _ => None,
    };
    let enum_value = |name: &str| match prop(name) {
        Some(Variant::Enum(e)) => Some(e.to_u32()),
        _ => None,
    };
    let content = |name: &str| match prop(name) {
        Some(Variant::ContentId(c)) => !c.as_str().is_empty(),
        Some(Variant::Content(c)) => c.as_uri().is_some_and(|u| !u.is_empty()),
        _ => false,
    };
    let class = inst.class.as_str();
    let mut out = Vec::new();
    let is_part = db
        .class_by_name(class)
        .zip(db.class_by_name("BasePart"))
        .is_some_and(|(c, part)| db.is_a(c.id, part.id));
    if is_part {
        match enum_value("Material") {
            Some(m) => out.push(format!("Material {}", item("Material", m))),
            None => out.push("Material (not saved)".into()),
        }
        for face in ["Top", "Bottom", "Front", "Back", "Left", "Right"] {
            if let Some(s) = enum_value(&format!("{face}Surface")) {
                out.push(format!("Surface {}", item("SurfaceType", s)));
                out.push(format!("Surface {} on {face}", item("SurfaceType", s)));
            }
        }
        if class == "MeshPart" && (content("TextureID") || content("TextureContent")) {
            out.push("MeshPart with TextureID".into());
        }
        if number("Reflectance").is_some_and(|r| r > 0.0) {
            out.push("Reflectance above 0".into());
        }
    }
    if matches!(class, "Decal" | "Texture") {
        if let Some(face) = enum_value("Face") {
            out.push(format!("{class} Face {}", item("NormalId", face)));
        }
        if let Some(z) = number("ZIndex") {
            out.push(format!("{class} ZIndex {z}"));
        }
        if number("Transparency").is_some_and(|t| t != 0.0) {
            out.push(format!("{class} Transparency not 0"));
        }
        let tint = match prop("Color3") {
            Some(Variant::Color3(c)) => [c.r, c.g, c.b] != [1.0; 3],
            _ => false,
        };
        if tint {
            out.push(format!("{class} Color3 not white"));
        }
        if let Some(parent) = dom.get_by_ref(inst.parent()) {
            let invisible = match parent.properties.get(&"Transparency".into()) {
                Some(Variant::Float32(t)) => *t >= 1.0,
                _ => false,
            };
            if invisible {
                out.push(format!("{class} on an invisible part"));
            }
            out.push(format!("{class} on {}", parent.class));
        }
        if class == "Texture" {
            let (u, v) = (number("StudsPerTileU"), number("StudsPerTileV"));
            if let (Some(u), Some(v)) = (u, v) {
                out.push(format!("Texture StudsPerTile {u} x {v}"));
            }
            if ["OffsetStudsU", "OffsetStudsV"]
                .iter()
                .any(|k| number(k).is_some_and(|o| o != 0.0))
            {
                out.push("Texture Offset not 0".into());
            }
        }
    }
    if matches!(
        class,
        "SpecialMesh" | "FileMesh" | "BlockMesh" | "CylinderMesh"
    ) {
        if let Some(Variant::Vector3(c)) = prop("VertexColor")
            && [c.x, c.y, c.z] != [1.0; 3]
        {
            out.push(format!("{class} VertexColor not (1, 1, 1)"));
        }
        if content("TextureId") || content("TextureContent") {
            let kind = match (class, enum_value("MeshType")) {
                ("SpecialMesh", Some(t)) => format!("SpecialMesh {}", item("MeshType", t)),
                _ => class.to_owned(),
            };
            out.push(format!("{kind} with TextureId"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert_eq!(shape(""), "(empty)");
        assert_eq!(shape("rbxassetid://12345"), "rbxassetid://#");
        assert_eq!(
            shape("http://www.roblox.com/asset/?id=12345"),
            "http://www.roblox.com/asset/?id=#"
        );
        assert_eq!(
            shape("http://www.roblox.com/Asset/?ID=1&version=2"),
            "http://www.roblox.com/Asset/?ID=#&version=#"
        );
        assert_eq!(
            shape("rbxasset://textures/face.png"),
            "rbxasset://textures/*.png"
        );
        assert_eq!(
            shape("rbxasset://fonts\\head.mesh"),
            "rbxasset://fonts\\*.mesh"
        );
        assert_eq!(
            shape("rbxthumb://type=Asset&id=12&w=150&h=150"),
            "rbxthumb://type=*&id=#&w=#&h=#"
        );
        assert_eq!(shape(" 123"), "(padded) #");
    }

    #[test]
    fn source_urls() {
        let src = r#"local a = "rbxassetid://5" x.Image = 'http://www.roblox.com/asset/?id=' .. id
            local b = "http://example.com/x""#;
        let urls: Vec<_> = urls_in_source(src).collect();
        assert_eq!(urls, ["rbxassetid://5", "http://www.roblox.com/asset/?id="]);
    }
}
