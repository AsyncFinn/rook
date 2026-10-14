//! The material textures Studio has downloaded, read from the user's own
//! Roblox content cache (`docs/behavior/materials.md`).
//!
//! Studio's install holds only Plastic's textures; every other material's
//! colour, normal and roughness-metalness maps are downloaded when a part
//! first shows the material, and kept in `%LOCALAPPDATA%\Roblox`: an SQLite
//! database, `rbx-storage.db`, whose `files` rows hold small entries in
//! `content` and point at `rbx-storage\<xx>\<id>` for larger ones. Each
//! entry is `RBXH`, the request's URL and response headers, then the body:
//! here a KTX2 file holding some of a texture's mip levels ("packs"), named
//! by the `contentHash` in its key-value data. Rook reads the largest pack
//! cached; a cache that only holds the small ones gives a blurry texture,
//! as Studio's does from afar.
//!
//! Finding the packs means reading the start of every cached body (5.5 s
//! for 53,000 entries), so the rows read and where the wanted textures are
//! is kept in an index in Rook's own cache, and later runs only read rows
//! added since. `ROOK_ROBLOX_CACHE` names another folder holding
//! `rbx-storage.db`, or `off`.

mod materials;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rook_types::Material;

use crate::MaterialMap;
use crate::image::{Image, ImageFormat, ktx2};

pub use materials::content_hash;

/// The environment variable that overrides where the cache is.
pub const CACHE_VAR: &str = "ROOK_ROBLOX_CACHE";

/// Material textures are kept at 512 × 512 at most (Studio's are 512 or
/// 1024): 40 materials' three maps at 1024 would hold 640 MB.
pub const MATERIAL_MAX_SIZE: u32 = 512;

/// How much of a body is read to find its key-value data.
const HEAD: usize = 4096;
const INDEX_FILE: &str = "studio-materials.json";

/// One pack of a texture: the cache row holding it and its largest level's
/// width.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pack {
    id: Vec<u8>,
    width: u32,
}

#[derive(Debug, Default)]
struct Index {
    /// The last row read (rows are numbered in the order they were added).
    rowid: i64,
    packs: HashMap<String, Vec<Pack>>,
}

/// The user's Roblox content cache.
#[derive(Debug)]
pub struct StudioCache {
    root: PathBuf,
    /// Where the index is kept between runs (Rook's cache folder).
    index_dir: Option<PathBuf>,
    index: Mutex<Option<Index>>,
}

impl StudioCache {
    /// The cache in `root` (a folder holding `rbx-storage.db`), if there.
    pub fn at(root: &Path, index_dir: Option<PathBuf>) -> Option<Self> {
        root.join("rbx-storage.db").is_file().then(|| Self {
            root: root.to_path_buf(),
            index_dir,
            index: Mutex::new(None),
        })
    }

    /// The user's cache: `ROOK_ROBLOX_CACHE`, or `%LOCALAPPDATA%\Roblox`
    /// on Windows (the only place it has been seen).
    pub fn find(index_dir: Option<PathBuf>) -> Option<Self> {
        let root = match std::env::var_os(CACHE_VAR) {
            Some(v) if v.eq_ignore_ascii_case("off") || v.is_empty() => return None,
            Some(v) => PathBuf::from(v),
            None => PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Roblox"),
        };
        Self::at(&root, index_dir)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the cache holds any of `material`'s textures.
    pub fn has_material(&self, material: Material) -> bool {
        MaterialMap::ALL
            .into_iter()
            .any(|map| !self.packs(material, map).is_empty())
    }

    /// The cached packs of `material`'s `map`, largest first.
    fn packs(&self, material: Material, map: MaterialMap) -> Vec<Pack> {
        let Some(hash) = content_hash(material, map) else {
            return Vec::new();
        };
        let mut guard = self.index.lock().expect("studio cache index");
        let index = guard.get_or_insert_with(|| self.load_index());
        self.refresh(index);
        let mut packs = index.packs.get(hash).cloned().unwrap_or_default();
        packs.sort_by_key(|p| std::cmp::Reverse(p.width));
        packs
    }

    /// `material`'s `map`, decoded, and where it came from; `None` when
    /// Studio has no such texture or hasn't downloaded it.
    pub fn material(&self, material: Material, map: MaterialMap) -> Option<(Image, String)> {
        let hash = content_hash(material, map)?;
        let packs = self.packs(material, map);
        for pack in packs {
            let Some(body) = self.read(&pack.id, None) else {
                continue;
            };
            let Ok(image) = decode(&body, map) else {
                continue;
            };
            return Some((image, format!("{} {hash}", self.root.display())));
        }
        None
    }

    fn connection(&self) -> Option<rusqlite::Connection> {
        let flags =
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
        rusqlite::Connection::open_with_flags(self.root.join("rbx-storage.db"), flags).ok()
    }

    /// A body (after its `RBXH` header) by row id, or its first `limit`
    /// bytes, from the database or its file.
    fn read(&self, id: &[u8], limit: Option<usize>) -> Option<Vec<u8>> {
        let db = self.connection()?;
        let content: Option<Vec<u8>> = db
            .query_row("SELECT content FROM files WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .ok()?;
        let entry = match content {
            Some(bytes) => bytes,
            None => read_file(&self.file(id), limit)?,
        };
        let at = find(&entry, ktx2::MAGIC)?;
        Some(entry[at..].to_vec())
    }

    fn file(&self, id: &[u8]) -> PathBuf {
        let hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
        self.root.join("rbx-storage").join(&hex[..2]).join(hex)
    }

    /// Read the rows added since the index was last brought up to date,
    /// noting where the wanted textures' packs are.
    fn refresh(&self, index: &mut Index) {
        profiling::scope!("StudioCache::refresh");
        let Some(db) = self.connection() else {
            return;
        };
        let wanted: std::collections::HashSet<&str> = materials::all().collect();
        let Ok(mut rows) = db.prepare(
            "SELECT rowid, id, substr(content, 1, ?2), content IS NULL FROM files \
             WHERE rowid > ?1 AND category = 10 ORDER BY rowid",
        ) else {
            return;
        };
        let Ok(found) = rows.query_map(rusqlite::params![index.rowid, HEAD as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Option<Vec<u8>>>(2)?,
                r.get::<_, bool>(3)?,
            ))
        }) else {
            return;
        };
        let before = index.rowid;
        for (rowid, id, head, on_disk) in found.flatten() {
            index.rowid = index.rowid.max(rowid);
            let head = match (head, on_disk) {
                (Some(head), false) => head,
                (_, true) => match read_file(&self.file(&id), Some(HEAD)) {
                    Some(head) => head,
                    None => continue,
                },
                (None, false) => continue,
            };
            let Some(at) = find(&head, ktx2::MAGIC) else {
                continue;
            };
            let Some(kv) = ktx2::key_values(&head[at..]) else {
                continue;
            };
            let Some(hash) = kv
                .iter()
                .find(|(k, _)| k == "contentHash")
                .map(|(_, v)| String::from_utf8_lossy(v).into_owned())
                .filter(|h| wanted.contains(h.as_str()))
            else {
                continue;
            };
            let width = head
                .get(at + 20..at + 24)
                .map_or(0, |w| u32::from_le_bytes(w.try_into().expect("4 bytes")));
            let packs = index.packs.entry(hash).or_default();
            packs.retain(|p| p.id != id);
            packs.push(Pack { id, width });
        }
        if index.rowid != before {
            self.save_index(index);
        }
    }

    fn load_index(&self) -> Index {
        let Some(text) = self
            .index_dir
            .as_ref()
            .and_then(|d| std::fs::read_to_string(d.join(INDEX_FILE)).ok())
        else {
            return Index::default();
        };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Index::default();
        };
        // An index of another cache folder doesn't apply.
        if json["root"].as_str() != Some(&self.root.display().to_string()) {
            return Index::default();
        }
        let mut index = Index {
            rowid: json["rowid"].as_i64().unwrap_or(0),
            packs: HashMap::new(),
        };
        for (hash, packs) in json["packs"].as_object().into_iter().flatten() {
            let packs = packs.as_array().into_iter().flatten().filter_map(|p| {
                Some(Pack {
                    id: from_hex(p["id"].as_str()?)?,
                    width: p["width"].as_u64()? as u32,
                })
            });
            index.packs.insert(hash.clone(), packs.collect());
        }
        index
    }

    fn save_index(&self, index: &Index) {
        let Some(dir) = &self.index_dir else {
            return;
        };
        let packs: serde_json::Map<String, serde_json::Value> = index
            .packs
            .iter()
            .map(|(hash, packs)| {
                let list = packs
                    .iter()
                    .map(|p| {
                        let id: String = p.id.iter().map(|b| format!("{b:02x}")).collect();
                        serde_json::json!({ "id": id, "width": p.width })
                    })
                    .collect();
                (hash.clone(), serde_json::Value::Array(list))
            })
            .collect();
        let json = serde_json::json!({
            "root": self.root.display().to_string(),
            "rowid": index.rowid,
            "packs": packs,
        });
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(dir.join(INDEX_FILE), json.to_string());
    }
}

/// A pack's largest level as an image `map` can use: normal maps come
/// DXT5nm-style (X in alpha, Y in green; measured) and become ordinary
/// RGB normal maps; roughness-metalness keeps red (metalness) and green
/// (roughness) with an opaque alpha, so its mips aren't weighted by it.
fn decode(ktx: &[u8], map: MaterialMap) -> Result<Image, crate::image::ImageError> {
    let mut rgba = ktx2::decode(ktx)?;
    match map {
        MaterialMap::Color => {}
        MaterialMap::Normal => {
            for p in rgba.pixels_mut() {
                let x = f32::from(p[3]) / 127.5 - 1.0;
                let y = f32::from(p[1]) / 127.5 - 1.0;
                let z = (1.0 - x * x - y * y).max(0.0).sqrt();
                p.0 = [p[3], p[1], ((z * 0.5 + 0.5) * 255.0).round() as u8, 255];
            }
        }
        MaterialMap::RoughnessMetalness => {
            for p in rgba.pixels_mut() {
                p.0 = [p[0], p[1], 0, 255];
            }
        }
    }
    Image::from_rgba(rgba, ImageFormat::Ktx2, MATERIAL_MAX_SIZE)
}

fn read_file(path: &Path, limit: Option<usize>) -> Option<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut out = Vec::new();
    match limit {
        Some(n) => file.take(n as u64).read_to_end(&mut out).ok()?,
        None => (&file).read_to_end(&mut out).ok()?,
    };
    Some(out)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cache folder with one body inline and one in a file, as Studio
    /// writes them.
    #[test]
    fn finds_and_decodes_cached_packs() {
        let dir = crate::tests::temp_dir("studio-cache");
        let db = rusqlite::Connection::open(dir.join("rbx-storage.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE files (id BLOB PRIMARY KEY NOT NULL, content BLOB, size INTEGER DEFAULT 0 NOT NULL, \
             hits INTEGER DEFAULT 0 NOT NULL, atime INTEGER DEFAULT 0 NOT NULL, category INTEGER DEFAULT 0 NOT NULL, \
             score INTEGER DEFAULT 0 NOT NULL, ttl INTEGER DEFAULT 0 NOT NULL)",
        )
        .unwrap();
        let wood_color = content_hash(Material::Wood, MaterialMap::Color).unwrap();
        let wood_normal = content_hash(Material::Wood, MaterialMap::Normal).unwrap();
        let entry = |ktx: Vec<u8>| [b"RBXH\x02\0\0\0\0\0\0\0".as_slice(), &ktx].concat();
        let inline = entry(ktx2::tests::bc1(0xF800, wood_color, true));
        db.execute(
            "INSERT INTO files (id, content, category) VALUES (?1, ?2, 10)",
            rusqlite::params![vec![1u8; 16], inline],
        )
        .unwrap();
        let on_disk = entry(ktx2::tests::bc1(0x07E0, wood_normal, false));
        db.execute(
            "INSERT INTO files (id, content, category) VALUES (?1, NULL, 10)",
            rusqlite::params![vec![0xABu8; 16]],
        )
        .unwrap();
        drop(db);
        std::fs::create_dir_all(dir.join("rbx-storage/ab")).unwrap();
        std::fs::write(dir.join("rbx-storage/ab").join("ab".repeat(16)), on_disk).unwrap();

        let index_dir = dir.join("index");
        let cache = StudioCache::at(&dir, Some(index_dir.clone())).unwrap();
        let (color, _) = cache.material(Material::Wood, MaterialMap::Color).unwrap();
        assert_eq!(
            (color.width, &color.mips[0][..4]),
            (4, &[255, 0, 0, 255][..])
        );
        // Green 255 and alpha 255: Y and X both +1, flattened into RGB.
        let (normal, _) = cache.material(Material::Wood, MaterialMap::Normal).unwrap();
        assert_eq!(&normal.mips[0][..4], &[255, 255, 128, 255]);
        assert!(
            cache
                .material(Material::Brick, MaterialMap::Color)
                .is_none()
        );
        // A second cache reads the saved index instead of every row.
        let again = StudioCache::at(&dir, Some(index_dir)).unwrap();
        assert_eq!(again.load_index().rowid, 2);
    }
}
