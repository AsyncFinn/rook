//! `rook-tools content`: how content references resolve.
//!
//! `rook-tools content <reference>...` shows what each reference means and where
//! its bytes come from; `rook-tools content --place <file>` does the same for
//! every content property in a place and sums it up.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use rbx_dom_weak::WeakDom;
use rbx_types::{Ref, Variant};
use rook_content::{ContentRef, Failure, FailureKind, Fetched, Loader, Resolver, SourceKind, Want};

mod pin;
mod place;

pub use pin::pin;
pub use place::place;

pub struct Options {
    /// Never use the network: only the install, folders and the cache.
    pub offline: bool,
    /// Forget cached answers for these references first.
    pub refresh: bool,
}

fn resolver(options: &Options) -> Arc<Resolver> {
    // Asset delivery is on by default here: this command exists to show
    // where content comes from. ROOK_ASSET_DELIVERY=off also turns it off.
    let mut config = rook_engine::settings::content_config(true);
    if options.offline {
        config.network = false;
    }
    let resolver = Resolver::new(config);
    print_setup(&resolver);
    Arc::new(resolver)
}

/// What `rook-tools cache` does.
pub enum CacheCommand {
    Show,
    /// Delete what hasn't been used for this many days.
    Prune(u64),
    Trim,
    Clear,
}

/// `rook-tools cache`: the download cache's folder and size, or a clean-up.
pub fn cache(command: CacheCommand) -> anyhow::Result<()> {
    // Making the config moves a cache left in the old place first.
    let config = rook_engine::settings::content_config(false);
    let dir = config.cache_dir.ok_or_else(|| {
        anyhow::anyhow!(
            "no cache folder (set {})",
            rook_engine::settings::env::CACHE_FOLDER
        )
    })?;
    let cache = rook_content::resolver::cache::Cache::open(&dir)?;
    let mb = |bytes: u64| bytes as f64 / (1 << 20) as f64;
    let freed = match command {
        CacheCommand::Show => None,
        CacheCommand::Prune(days) => {
            Some(cache.prune_older_than(std::time::Duration::from_secs(days * 24 * 60 * 60)))
        }
        CacheCommand::Trim => Some(cache.trim(rook_engine::settings::cache_max())),
        CacheCommand::Clear => {
            let (blobs, bytes, _) = cache.stats();
            cache.clear()?;
            Some((blobs, bytes))
        }
    };
    if let Some((blobs, bytes)) = freed {
        println!("deleted {blobs} files, {:.1} MB", mb(bytes));
    }
    let (blobs, bytes, refs) = cache.stats();
    println!("cache:  {}", dir.display());
    println!(
        "holds:  {blobs} files, {:.1} MB, for {refs} references",
        mb(bytes)
    );
    println!(
        "limit:  {:.0} MB ({})",
        mb(rook_engine::settings::cache_max()),
        rook_engine::settings::env::CACHE_MAX_SIZE
    );
    Ok(())
}

fn print_setup(r: &Resolver) {
    match r.install() {
        Some(i) => println!("Roblox install: {} ({:?})", i.root.display(), i.kind),
        None => println!(
            "Roblox install: none found (set {} to a version or content folder)",
            rook_engine::settings::env::ROBLOX_INSTALL
        ),
    }
    for folder in r.folders() {
        println!("content folder: {}", folder.display());
    }
    let order: Vec<&str> = r.order().iter().map(|k| k.name()).collect();
    println!("providers:      {}", order.join(" > "));
    let delivery = match r.network() {
        (false, _) => "off (cache only)",
        (true, false) => "on, anonymous",
        (true, true) => "on, anonymous, then the Open Cloud API key",
    };
    println!("asset delivery: {delivery}");
    match r.cache() {
        Some(c) => println!("cache:          {}", c.dir().display()),
        None => println!("cache:          none"),
    }
    println!();
}

/// What a resolved file looks like, from its first bytes.
fn describe(bytes: &[u8]) -> String {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"version ") {
        let line = bytes.iter().take(16).take_while(|&&b| b != b'\n');
        let line: String = line.map(|&b| b as char).collect();
        let detail = match rook_content::decode::mesh::Mesh::decode(bytes) {
            Ok(m) => mesh_detail(&m),
            Err(e) => e.to_string(),
        };
        return format!("mesh, {}, {detail}", line.trim_end());
    }
    if starts(b"<roblox")
        && let Ok(data) = rook_content::decode::mesh::union::asset_mesh_data(bytes)
    {
        let kind = rook_content::decode::mesh::union::MeshDataKind::of(&data).name();
        let mesh = match rook_content::decode::mesh::Mesh::decode(&data) {
            Ok(m) => mesh_detail(&m),
            Err(e) => e.to_string(),
        };
        return format!(
            "union asset, MeshData {kind}, {mesh}; PhysicsData {}",
            physics_detail(bytes)
        );
    }
    let kind = if starts(b"\x89PNG") {
        "PNG image"
    } else if starts(b"\xFF\xD8\xFF") {
        "JPEG image"
    } else if starts(b"DDS ") {
        "DDS texture"
    } else if let Some(format) = rook_content::decode::audio::AudioFormat::sniff(bytes) {
        return match rook_content::decode::audio::Audio::decode(bytes) {
            Ok(a) => format!(
                "{} sound, {:.6} s, {} Hz, {} channel(s)",
                format.name(),
                a.seconds(),
                a.sample_rate,
                a.channels
            ),
            Err(e) => e.to_string(),
        };
    } else if starts(b"<roblox!") {
        "binary model"
    } else if starts(b"<roblox") {
        "XML model"
    } else {
        "unknown"
    };
    kind.to_owned()
}

/// A union asset's collision geometry: its form, and its hulls' count,
/// points and bounds.
fn physics_detail(bytes: &[u8]) -> String {
    use rook_content::decode::mesh::union::physics;
    use rook_types::UnionCollision;
    let hulls = match physics::decode_asset(bytes) {
        Ok(None) => return "none".into(),
        Ok(Some(UnionCollision::Block)) => return "BLOCK".into(),
        Ok(Some(UnionCollision::Hulls(hulls))) => hulls,
        Err(e) => return e.to_string(),
    };
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in hulls.iter().flat_map(|h| &h.positions) {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let points: usize = hulls.iter().map(|h| h.positions.len()).sum();
    let size = [0, 1, 2].map(|i| format!("{:.3}", hi[i] - lo[i]));
    format!(
        "{} hulls, {points} points, {} studs",
        hulls.len(),
        size.join(" x ")
    )
}

/// A decoded mesh's triangles, size and centre.
fn mesh_detail(m: &rook_content::decode::mesh::Mesh) -> String {
    let (lo, hi) = m.bounds();
    let size = [0, 1, 2].map(|i| format!("{:.3}", hi[i] - lo[i]));
    let centre = [0, 1, 2].map(|i| format!("{:.3}", (hi[i] + lo[i]) / 2.0));
    let mut uv = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for t in &m.uvs {
        uv = [
            uv[0].min(t[0]),
            uv[1].min(t[1]),
            uv[2].max(t[0]),
            uv[3].max(t[1]),
        ];
    }
    format!(
        "{} triangles, {} studs, centred at ({}), UVs {:.2}..{:.2} x {:.2}..{:.2}",
        m.indices.len() / 3,
        size.join(" x "),
        centre.join(", "),
        uv[0],
        uv[2],
        uv[1],
        uv[3]
    )
}

fn print_fetched(f: &Fetched) {
    println!("  from:     {}", f.source.kind.name());
    println!("  location: {}", f.source.location);
    println!(
        "  size:     {} bytes ({})",
        f.bytes.len(),
        describe(&f.bytes)
    );
    println!("  blake3:   {}", f.hash.to_hex());
}

pub fn references(refs: &[String], options: &Options) -> anyhow::Result<()> {
    let resolver = resolver(options);
    for text in refs {
        println!("{text}");
        let Some(r) = ContentRef::parse(text) else {
            println!("  no content (not a form Roblox loads)");
            continue;
        };
        println!("  parsed:   {r:?}");
        if options.refresh {
            forget(&resolver, &r);
        }
        match resolver.resolve(&r) {
            Ok(f) => print_fetched(&f),
            Err(e) => println!("  failed:   {e}"),
        }
    }
    Ok(())
}

fn forget(resolver: &Resolver, r: &ContentRef) {
    let key = match r {
        ContentRef::File(path) => rook_content::resolver::defaults::legacy_asset(path)
            .and_then(|id| ContentRef::Asset { id, version: None }.cache_key()),
        other => other.cache_key(),
    };
    if let (Some(cache), Some(key)) = (resolver.cache(), key) {
        cache.forget(&key);
    }
}
