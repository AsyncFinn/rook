//! Content references: the strings in `MeshId`, `TextureId`, `Texture`,
//! `SoundId` and the other content properties, parsed once into
//! [`ContentRef`] by the rules of what Studio loads
//! (`docs/behavior/content.md`, "Which forms Studio loads"; each form is a
//! test here).

use std::fmt;

use rook_types::{Material, MaterialSet};

/// What a content string refers to. An empty string, or anything that
/// isn't one of these forms, is no content at all ([`ContentRef::parse`]
/// returns `None`), not an error.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ContentRef {
    /// `rbxasset://<path>`: a file shipped with the Roblox client, relative
    /// to its `content` folder. Backslashes become slashes and empty
    /// segments go (Studio turns backslashes into slashes when the
    /// property is set); the case is kept, and lookups ignore it.
    File(String),
    /// An asset by id: `rbxassetid://<id>`, or a legacy
    /// `http://www.roblox.com/asset/?id=<id>` URL, optionally at one
    /// version (`&version=<n>`, honoured: version 1 of the head mesh asset
    /// is a different mesh).
    Asset { id: u64, version: Option<u32> },
    /// A legacy URL naming one asset version (`asset/?assetversionid=`).
    /// Asset delivery no longer looks these up.
    AssetVersion(u64),
    /// `rbxthumb://type=<type>&id=<id>&w=<w>&h=<h>`: a thumbnail Roblox
    /// renders. Parsed, not resolved yet.
    Thumbnail(Thumbnail),
    /// One of a built-in material's textures in one of the two sets: no
    /// string names it, but every part of that material draws it. It comes
    /// from the TexturePack asset Studio uses ([`crate::materials`]).
    Material(Material, MaterialSet, MaterialMap),
    /// One map of a TexturePack asset (terrain's textures).
    TexturePack(u64, MaterialMap),
}

/// Which of a material's textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MaterialMap {
    /// The colour, which the part's Color multiplies (sRGB).
    Color = 0,
    /// A tangent-space normal map: X in red, Y (up the image) in green.
    Normal = 1,
    /// Roughness in green and metalness in red, as Studio packs them.
    RoughnessMetalness = 2,
}

impl MaterialMap {
    pub const ALL: [Self; 3] = [Self::Color, Self::Normal, Self::RoughnessMetalness];

    pub fn name(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Normal => "normal",
            Self::RoughnessMetalness => "roughness-metalness",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Thumbnail {
    /// `Asset`, `Avatar`, `AvatarHeadShot`, `BadgeIcon`, `BundleThumbnail`,
    /// `GameIcon`, `GamePass`, `GroupIcon`, `Outfit`...
    pub kind: String,
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// `filters=` (e.g. `circular`), if any.
    pub filters: Option<String>,
}

impl ContentRef {
    /// Parse a content string. `None` means no content.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim_end();
        if let Some(path) = text.strip_prefix("rbxasset://") {
            let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
            return (!parts.is_empty()).then(|| Self::File(parts.join("/")));
        }
        if let Some(rest) = text.strip_prefix("rbxassetid://") {
            return asset_query(&format!("id={rest}"));
        }
        if let Some(rest) = text.strip_prefix("rbxthumb://") {
            return thumbnail(rest).map(Self::Thumbnail);
        }
        let (scheme, rest) = text.trim_start().split_once("://")?;
        if scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https") {
            let (location, query) = rest.split_once('?')?;
            let (_host, path) = location.split_once('/')?;
            let last_segment = path.trim_end_matches('/').rsplit('/').next();
            if last_segment.is_some_and(|p| p.eq_ignore_ascii_case("asset")) {
                return asset_query(query);
            }
        }
        None
    }

    /// The key this reference is cached under (`asset-12`, `asset-12-v3`,
    /// `assetversion-7`), for references that are downloaded.
    pub fn cache_key(&self) -> Option<String> {
        match self {
            Self::Asset { id, version: None } => Some(format!("asset-{id}")),
            Self::Asset {
                id,
                version: Some(v),
            } => Some(format!("asset-{id}-v{v}")),
            Self::AssetVersion(id) => Some(format!("assetversion-{id}")),
            Self::File(_) | Self::Thumbnail(_) | Self::Material(..) | Self::TexturePack(..) => None,
        }
    }
}

impl fmt::Display for ContentRef {
    /// The canonical form: `rbxasset://path`, `rbxassetid://id`, and asset
    /// URLs for versions.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TexturePack(id, map) => write!(f, "rook-texturepack://{id}/{}", map.name()),
            Self::Material(material, set, map) => {
                let set = match set {
                    MaterialSet::Legacy => "legacy",
                    MaterialSet::Materials2022 => "2022",
                };
                write!(
                    f,
                    "rook-material://{set}/{}/{}",
                    material.name(),
                    map.name()
                )
            }
            Self::File(path) => write!(f, "rbxasset://{path}"),
            Self::Asset { id, version: None } => write!(f, "rbxassetid://{id}"),
            Self::Asset {
                id,
                version: Some(v),
            } => write!(f, "https://www.roblox.com/asset/?id={id}&version={v}"),
            Self::AssetVersion(id) => {
                write!(f, "https://www.roblox.com/asset/?assetversionid={id}")
            }
            Self::Thumbnail(t) => {
                write!(
                    f,
                    "rbxthumb://type={}&id={}&w={}&h={}",
                    t.kind, t.id, t.width, t.height
                )?;
                if let Some(filters) = &t.filters {
                    write!(f, "&filters={filters}")?;
                }
                Ok(())
            }
        }
    }
}

/// What Studio stores when a script sets a Decal's or Texture's `Texture`
/// (or a Decal's `ColorMap`) to a web URL: `rbxassetid://` and the URL's
/// first `id` value, kept as written; `None` leaves the URL as it is. Other
/// content properties (`TextureId`, `MeshId`, `Image`, ...) keep the URL.
///
/// Observed in Studio 0.742 (conformance case `content/decal`): any URL
/// starting with a lower-case `http://` or `https://` with a non-empty `id`
/// query value (the key in any case) is rewritten, whatever its host and
/// path (`example.com/foo?id=5`, `asset.aspx`, `v1/asset`) and dropping
/// `version`; `?id=5abc` becomes `rbxassetid://5abc`, `?id= 5`
/// `rbxassetid:// 5`, and `?id=5&id=6` `rbxassetid://5`. An upper-case
/// scheme, a leading space or an empty id is kept.
pub fn image_url_rewrite(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))?;
    let (_, query) = rest.split_once('?')?;
    let id = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(k, _)| k.eq_ignore_ascii_case("id"))
        .map(|(_, v)| v)
        .filter(|v| !v.is_empty())?;
    Some(format!("rbxassetid://{id}"))
}

/// An id as Studio reads it: spaces around it, an optional `+` and
/// leading zeros are fine; anything else isn't.
fn id_value(value: &str) -> Option<u64> {
    let digits = value.trim();
    let digits = digits.strip_prefix('+').unwrap_or(digits);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok().filter(|&n| n > 0)
}

/// The value of `key` in `key=value&key=value`, ignoring the key's case;
/// for a repeated key, the last.
fn last<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        .map(|(_, v)| v)
        .next_back()
}

/// An asset URL's query: `id` (with `version`), or `assetversionid`.
fn asset_query(query: &str) -> Option<ContentRef> {
    match last(query, "id") {
        Some(id) => Some(ContentRef::Asset {
            id: id_value(id)?,
            version: last(query, "version")
                .and_then(id_value)
                .and_then(|v| u32::try_from(v).ok()),
        }),
        None => last(query, "assetversionid")
            .and_then(id_value)
            .map(ContentRef::AssetVersion),
    }
}

fn thumbnail(rest: &str) -> Option<Thumbnail> {
    let number = |key| last(rest, key).and_then(id_value);
    let t = Thumbnail {
        kind: last(rest, "type")?.trim().to_owned(),
        id: number("id")?,
        width: u32::try_from(number("w").unwrap_or(0)).ok()?,
        height: u32::try_from(number("h").unwrap_or(0)).ok()?,
        filters: last(rest, "filters").map(|f| f.trim().to_owned()),
    };
    (!t.kind.is_empty()).then_some(t)
}

#[cfg(test)]
mod tests {
    #[test]
    fn image_url_rewrite() {
        use super::image_url_rewrite as rw;
        let id = |s: &str| Some(s.to_owned());
        assert_eq!(
            rw("http://www.roblox.com/asset/?id=5"),
            id("rbxassetid://5")
        );
        assert_eq!(
            rw("https://roblox.com/Asset/?ID=5&version=2"),
            id("rbxassetid://5")
        );
        assert_eq!(
            rw("http://example.com/foo?version=1&id=5&id=6"),
            id("rbxassetid://5")
        );
        assert_eq!(
            rw("http://www.roblox.com/asset/?id= 5"),
            id("rbxassetid:// 5")
        );
        assert_eq!(rw("HTTP://WWW.ROBLOX.COM/ASSET/?ID=5"), None);
        assert_eq!(rw(" http://www.roblox.com/asset/?id=5"), None);
        assert_eq!(rw("http://www.roblox.com/asset/?id="), None);
        assert_eq!(rw("rbxassetid://5"), None);
    }

    use super::*;

    fn asset(id: u64) -> Option<ContentRef> {
        Some(ContentRef::Asset { id, version: None })
    }

    #[test]
    fn no_content() {
        for s in [
            "",
            "   ",
            "abc",
            "123",
            "rbxassetid://",
            "rbxassetid://0",
            "rbxasset://",
            "ftp://x",
            "http://www.roblox.com/asset/?id=",
            "http://www.roblox.com/thumbs/asset.ashx?assetid=5",
            // Each drew nothing in Studio.
            " rbxassetid://5",
            "\trbxassetid://5",
            "Rbxassetid://5",
            "RBXASSET://fonts/head.mesh",
            "rbxassetid://5/",
            "rbxassetid://5abc",
            "http://www.roblox.com/asset.aspx?id=5",
            "http://www.roblox.com/foo/?id=5",
            "https://www.roblox.com/asset/Default.ashx?id=5",
            "http://www.roblox.com/asset/?id=5abc",
            "http://www.roblox.com/asset/?id=5;",
            "http://www.roblox.com/asset/?id=5#frag",
            "www.roblox.com/asset/?id=5",
        ] {
            assert_eq!(ContentRef::parse(s), None, "{s:?}");
        }
    }

    #[test]
    fn files() {
        assert_eq!(
            ContentRef::parse("rbxasset://textures/face.png"),
            Some(ContentRef::File("textures/face.png".into()))
        );
        // Each drew the head in Studio.
        for path in [
            "rbxasset://fonts\\head.mesh",
            "rbxasset:///fonts/head.mesh",
            "rbxasset://fonts//head.mesh",
            "rbxasset://fonts/head.mesh  ",
        ] {
            assert_eq!(
                ContentRef::parse(path),
                Some(ContentRef::File("fonts/head.mesh".into())),
                "{path}"
            );
        }
    }

    #[test]
    fn asset_ids_and_legacy_urls() {
        assert_eq!(ContentRef::parse("rbxassetid://12221462"), asset(12221462));
        // Each drew the head mesh in Studio (with its id).
        for url in [
            "rbxassetid://5 ",
            "rbxassetid:// 5",
            "rbxassetid://05",
            "rbxassetid://5&x=1",
            "http://www.roblox.com/asset/?id=5",
            "http://www.roblox.com/asset?id=5",
            "https://www.roblox.com/asset/?id=5",
            "http://roblox.com/asset/?id=5",
            "HTTP://www.roblox.com/asset/?id=5",
            "http://www.roblox.com/Asset/?ID=5",
            "http://www.roblox.com//asset/?id=5",
            "http://www.roblox.com/asset/?id= 5",
            "http://www.roblox.com/asset/?id=+5",
            "http://www.roblox.com/asset/?id=5 ",
            "  http://www.roblox.com/asset/?id=5",
            "http://www.roblox.com/asset/?ID=5&placeid=99",
            "http://www.roblox.com/asset/?x=1&id=5",
            "http://www.roblox.com/asset?id=1&id=5",
            "http://www.roblox.com/asset/?id=5&version=",
            "http://www.roblox.com/asset/?id=5&version=abc",
            "http://www.roblox.com/asset/?version1&id=5",
            "http://www.rblox.com/asset/?id=5",
            "http://www.subter.org/asset/?id=5",
            "http://example.com/asset/?id=5",
            "http://c0.roblox.com/asset/?id=5",
            "https://assetdelivery.roblox.com/v1/asset/?id=5",
        ] {
            assert_eq!(ContentRef::parse(url), asset(5), "{url}");
        }
        assert_eq!(
            ContentRef::parse("http://www.roblox.com/asset/?version=2&id=5"),
            Some(ContentRef::Asset {
                id: 5,
                version: Some(2)
            })
        );
        assert_eq!(
            ContentRef::parse("http://www.roblox.com/asset/?assetversionid=77"),
            Some(ContentRef::AssetVersion(77))
        );
    }

    #[test]
    fn thumbnails() {
        let t = ContentRef::parse("rbxthumb://type=Asset&id=12&w=150&h=150").unwrap();
        assert_eq!(
            t,
            ContentRef::Thumbnail(Thumbnail {
                kind: "Asset".into(),
                id: 12,
                width: 150,
                height: 150,
                filters: None
            })
        );
        assert_eq!(t.to_string(), "rbxthumb://type=Asset&id=12&w=150&h=150");
    }

    #[test]
    fn canonical_forms_and_keys() {
        let r = ContentRef::parse("http://www.roblox.com/asset/?id=5").unwrap();
        assert_eq!(r.to_string(), "rbxassetid://5");
        assert_eq!(r.cache_key().as_deref(), Some("asset-5"));
        let v = ContentRef::parse("http://www.roblox.com/asset/?id=5&version=2").unwrap();
        assert_eq!(ContentRef::parse(&v.to_string()), Some(v.clone()));
        assert_eq!(v.cache_key().as_deref(), Some("asset-5-v2"));
        assert_eq!(ContentRef::File("a".into()).cache_key(), None);
    }
}
