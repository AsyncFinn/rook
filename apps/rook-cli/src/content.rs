//! `rook content`: how content references resolve.
//!
//! `rook content <reference>...` shows what each reference means and where
//! its bytes come from; `rook content --place <file>` does the same for
//! every content property in a place and sums it up.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use rbx_dom_weak::WeakDom;
use rbx_types::{Ref, Variant};
use rook_content::{ContentRef, Failure, FailureKind, Fetched, Loader, Resolver, SourceKind, Want};

pub struct Options {
    /// Never use the network: only the install, folders and the cache.
    pub offline: bool,
    /// Forget cached answers for these references first.
    pub refresh: bool,
    /// `--place`: list every reference, not just failures.
    pub verbose: bool,
}

fn resolver(options: &Options) -> Arc<Resolver> {
    // Asset delivery is on by default here: this command exists to show
    // where content comes from. ROOK_ASSET_DELIVERY=off also turns it off.
    let mut config = rook_content::Config::from_env(true);
    if options.offline {
        config.network = false;
    }
    let resolver = Resolver::new(config);
    print_setup(&resolver);
    Arc::new(resolver)
}

/// What `rook cache` does.
pub enum CacheCommand {
    Show,
    /// Delete what hasn't been used for this many days.
    Prune(u64),
    Trim,
    Clear,
}

/// `rook cache`: the download cache's folder and size, or a clean-up.
pub fn cache(command: CacheCommand) -> anyhow::Result<()> {
    // Making the config moves a cache left in the old place first.
    let config = rook_content::Config::from_env(false);
    let dir = config.cache_dir.ok_or_else(|| {
        anyhow::anyhow!("no cache folder (set {})", rook_content::cache::CACHE_VAR)
    })?;
    let cache = rook_content::Cache::open(&dir)?;
    let mb = |bytes: u64| bytes as f64 / (1 << 20) as f64;
    let freed = match command {
        CacheCommand::Show => None,
        CacheCommand::Prune(days) => {
            Some(cache.prune_older_than(std::time::Duration::from_secs(days * 24 * 60 * 60)))
        }
        CacheCommand::Trim => Some(cache.trim(rook_content::Cache::max_from_env())),
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
        mb(rook_content::Cache::max_from_env()),
        rook_content::cache::MAX_VAR
    );
    Ok(())
}

fn print_setup(r: &Resolver) {
    match r.install() {
        Some(i) => println!("Roblox install: {} ({:?})", i.root.display(), i.kind),
        None => println!(
            "Roblox install: none found (set {} to a version or content folder)",
            rook_content::install::INSTALL_VAR
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
        let detail = match rook_content::mesh::Mesh::decode(bytes) {
            Ok(m) => mesh_detail(&m),
            Err(e) => e.to_string(),
        };
        return format!("mesh, {}, {detail}", line.trim_end());
    }
    if starts(b"<roblox")
        && let Ok(data) = rook_content::mesh::union::asset_mesh_data(bytes)
    {
        let kind = rook_content::mesh::union::MeshDataKind::of(&data).name();
        return match rook_content::mesh::Mesh::decode(&data) {
            Ok(m) => format!("union asset, MeshData {kind}, {}", mesh_detail(&m)),
            Err(e) => format!("union asset, MeshData {kind}, {e}"),
        };
    }
    let kind = if starts(b"\x89PNG") {
        "PNG image"
    } else if starts(b"\xFF\xD8\xFF") {
        "JPEG image"
    } else if starts(b"DDS ") {
        "DDS texture"
    } else if let Some(format) = rook_content::audio::AudioFormat::sniff(bytes) {
        return match rook_content::audio::Audio::decode(bytes) {
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

/// A decoded mesh's triangles, size and centre.
fn mesh_detail(m: &rook_content::mesh::Mesh) -> String {
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
        ContentRef::File(path) => rook_content::defaults::legacy_asset(path)
            .and_then(|id| ContentRef::Asset { id, version: None }.cache_key()),
        other => other.cache_key(),
    };
    if let (Some(cache), Some(key)) = (resolver.cache(), key) {
        cache.forget(&key);
    }
}

/// One distinct content string in a place.
#[derive(Default)]
struct Use {
    count: usize,
    /// Uses as a mesh (`MeshId` of a mesh class or MeshPart, a union's
    /// `AssetId`).
    mesh: usize,
    /// Uses as an image (textures, decals, images, sky faces).
    image: usize,
    /// Uses as a sound (Sound's `SoundId`).
    audio: usize,
    /// `Class.Property` of the first use.
    property: String,
    /// Full name of the first instance using it.
    instance: String,
}

impl Use {
    fn wants(&self, want: Want) -> usize {
        match want {
            Want::Mesh => self.mesh,
            Want::Image => self.image,
            Want::Audio => self.audio,
            Want::Bytes => 0,
        }
    }
}

/// What a property of `class` holds (`Want::for_property`).
fn property_want(class: &str, key: &str) -> Want {
    let mesh_owner = matches!(
        class,
        "SpecialMesh"
            | "FileMesh"
            | "CharacterMesh"
            | "MeshPart"
            | "UnionOperation"
            | "NegateOperation"
            | "IntersectOperation"
            | "PartOperation"
    );
    Want::for_property(key, mesh_owner)
}

/// What a decoded mesh was: a mesh file's version (`version 2.00`), or a
/// union asset's `MeshData` form.
fn mesh_version(bytes: &[u8]) -> String {
    use rook_content::mesh::union::{MeshDataKind, asset_mesh_data};
    if bytes.starts_with(b"<roblox") {
        let data = asset_mesh_data(bytes).unwrap_or_default();
        return format!("union MeshData {}", MeshDataKind::of(&data).name());
    }
    let head = &bytes[..bytes.len().min(16)];
    let line = head.split(|&b| b == b'\n').next().unwrap_or_default();
    let line = std::str::from_utf8(line).unwrap_or_default().trim_end();
    line.strip_prefix("version ").unwrap_or(line).to_owned()
}

/// The mesh, image or sound references in a place: how many decode, by
/// version or format, and which fail and why.
fn print_decoded(
    want: Want,
    refs: &BTreeMap<ContentRef, Vec<&str>>,
    uses: &BTreeMap<String, Use>,
    decoded: &BTreeMap<ContentRef, Result<Fetched, Failure>>,
    first_use: &dyn Fn(&[&str]) -> String,
) {
    if decoded.is_empty() {
        return;
    }
    let what = match want {
        Want::Mesh => "Meshes",
        Want::Image => "Images",
        _ => "Sounds",
    };
    let mut by_result: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failed = Vec::new();
    for (r, outcome) in decoded {
        let texts = &refs[r];
        let n: usize = texts.iter().map(|t| uses[*t].wants(want)).sum();
        let key = match outcome {
            Ok(f) => match (&f.image, &f.audio) {
                (Some(image), _) => format!("decoded, {}", image.format.name()),
                (_, Some(audio)) => format!(
                    "decoded, {} {} Hz",
                    audio.format.map_or("?", |f| f.name()),
                    audio.sample_rate
                ),
                _ => format!("decoded, version {}", mesh_version(&f.bytes)),
            },
            Err(e) if e.kind == FailureKind::Invalid => {
                failed.push((r, texts, e));
                format!("not decoded: {}", e.message)
            }
            Err(e) => format!("not loaded: {}", e.kind.name()),
        };
        let slot = by_result.entry(key).or_default();
        slot.0 += n;
        slot.1 += 1;
    }
    let total: usize = by_result.values().map(|v| v.0).sum();
    println!("\n{what}: {total} uses, {} distinct targets", decoded.len());
    println!("{:>8} {:>9}", "uses", "distinct");
    for (what, (n, d)) in &by_result {
        println!("{n:>8} {d:>9}  {what}");
    }
    if !failed.is_empty() {
        println!("\n{what} that load but don't decode:");
        for (r, texts, e) in failed {
            println!("  {r}  {}\n      {}", e.message, first_use(texts));
        }
    }
}

fn full_name(dom: &WeakDom, mut id: Ref) -> String {
    let mut parts = Vec::new();
    while let Some(inst) = dom.get_by_ref(id) {
        if inst.parent().is_none() {
            break;
        }
        parts.push(inst.name.as_str());
        id = inst.parent();
    }
    parts.reverse();
    parts.join(".")
}

pub fn place(file: &Path, options: &Options) -> anyhow::Result<()> {
    let bytes = fs_err::read(file)?;
    let loaded = rook_formats::load(&bytes)?;
    drop(bytes);
    let dom = &loaded.dom;
    let mut uses: BTreeMap<String, Use> = BTreeMap::new();
    for inst in dom.descendants() {
        for (key, value) in &inst.properties {
            let url = match value {
                Variant::ContentId(c) => c.as_str(),
                Variant::Content(c) => c.as_uri().unwrap_or(""),
                _ => continue,
            };
            if url.is_empty() {
                continue;
            }
            let u = uses.entry(url.to_owned()).or_default();
            if u.count == 0 {
                u.property = format!("{}.{key}", inst.class);
                u.instance = full_name(dom, inst.referent());
            }
            u.count += 1;
            match property_want(&inst.class, key) {
                Want::Mesh => u.mesh += 1,
                Want::Image => u.image += 1,
                Want::Audio => u.audio += 1,
                Want::Bytes => {}
            }
        }
    }
    println!("{}", file.display());
    let resolver = resolver(options);

    // Each distinct reference resolves once, on the loader's workers.
    let mut refs: BTreeMap<ContentRef, Vec<&str>> = BTreeMap::new();
    let mut unparsed = Vec::new();
    for text in uses.keys() {
        match ContentRef::parse(text) {
            Some(r) => refs.entry(r).or_default().push(text),
            None => unparsed.push(text.as_str()),
        }
    }
    let mut loader = Loader::new(resolver.clone());
    let mut tickets = BTreeMap::new();
    let mut mesh_tickets = BTreeMap::new();
    let mut image_tickets = BTreeMap::new();
    let mut audio_tickets = BTreeMap::new();
    for (r, texts) in &refs {
        if options.refresh {
            forget(&resolver, r);
        }
        tickets.insert(loader.request(r.clone()), r.clone());
        if texts.iter().any(|t| uses[*t].mesh > 0) {
            mesh_tickets.insert(loader.request_as(r.clone(), Want::Mesh), r.clone());
        }
        if texts.iter().any(|t| uses[*t].image > 0) {
            image_tickets.insert(loader.request_as(r.clone(), Want::Image), r.clone());
        }
        if texts.iter().any(|t| uses[*t].audio > 0) {
            audio_tickets.insert(loader.request_as(r.clone(), Want::Audio), r.clone());
        }
    }
    let started = std::time::Instant::now();
    let mut results: BTreeMap<ContentRef, Result<Fetched, Failure>> = BTreeMap::new();
    let mut meshes: BTreeMap<ContentRef, Result<Fetched, Failure>> = BTreeMap::new();
    let mut images: BTreeMap<ContentRef, Result<Fetched, Failure>> = BTreeMap::new();
    let mut sounds: BTreeMap<ContentRef, Result<Fetched, Failure>> = BTreeMap::new();
    for (t, outcome) in loader.wait_all() {
        if let Some(r) = tickets.remove(&t) {
            results.insert(r, outcome);
        } else if let Some(r) = mesh_tickets.remove(&t) {
            meshes.insert(r, outcome);
        } else if let Some(r) = image_tickets.remove(&t) {
            images.insert(r, outcome);
        } else if let Some(r) = audio_tickets.remove(&t) {
            sounds.insert(r, outcome);
        }
    }

    // Totals over uses (property values), with distinct references in
    // brackets.
    let mut by_source: BTreeMap<SourceKind, (usize, usize)> = BTreeMap::new();
    let mut by_failure: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let total_uses: usize = uses.values().map(|u| u.count).sum();
    for (r, texts) in &refs {
        let n: usize = texts.iter().map(|t| uses[*t].count).sum();
        let slot = match &results[r] {
            Ok(f) => by_source.entry(f.source.kind).or_default(),
            Err(e) => by_failure.entry(e.to_string()).or_default(),
        };
        slot.0 += n;
        slot.1 += 1;
    }
    if !unparsed.is_empty() {
        let n = unparsed.iter().map(|t| uses[*t].count).sum();
        by_failure.insert("no content".to_owned(), (n, unparsed.len()));
    }

    println!(
        "{total_uses} content references, {} distinct values, {} distinct targets ({:.1}s)\n",
        uses.len(),
        refs.len(),
        started.elapsed().as_secs_f64()
    );
    println!("{:>8} {:>9}", "uses", "distinct");
    for (kind, (n, d)) in &by_source {
        println!("{n:>8} {d:>9}  from {}", kind.name());
    }
    for (why, (n, d)) in &by_failure {
        println!("{n:>8} {d:>9}  failed: {why}");
    }

    let first_use = |texts: &[&str]| {
        let u = &uses[texts[0]];
        format!(
            "{} ({}, {}x)",
            u.instance,
            u.property,
            texts.iter().map(|t| uses[*t].count).sum::<usize>()
        )
    };
    let failed: Vec<_> = refs
        .iter()
        .filter_map(|(r, texts)| results[r].as_ref().err().map(|e| (r, texts, e)))
        .collect();
    if !failed.is_empty() || !unparsed.is_empty() {
        println!("\nFailures:");
        for (r, texts, e) in &failed {
            println!("  {r}  {e}\n      {}", first_use(texts));
        }
        for text in &unparsed {
            println!("  {text:?}  no content\n      {}", first_use(&[text]));
        }
    }
    print_decoded(Want::Mesh, &refs, &uses, &meshes, &first_use);
    print_decoded(Want::Image, &refs, &uses, &images, &first_use);
    print_decoded(Want::Audio, &refs, &uses, &sounds, &first_use);
    if options.verbose {
        println!("\nResolved:");
        for (r, texts) in &refs {
            if let Ok(f) = &results[r] {
                println!(
                    "  {r}  {} {} bytes ({})\n      {}",
                    f.source.kind.name(),
                    f.bytes.len(),
                    describe(&f.bytes),
                    first_use(texts)
                );
            }
        }
    }
    Ok(())
}
