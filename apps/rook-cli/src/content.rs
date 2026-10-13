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
            Ok(m) => {
                let (lo, hi) = m.bounds();
                let size = [0, 1, 2].map(|i| format!("{:.2}", hi[i] - lo[i]));
                let centre = [0, 1, 2].map(|i| format!("{:.2}", (hi[i] + lo[i]) / 2.0));
                format!(
                    "{} triangles, {} studs, centred at ({})",
                    m.indices.len() / 3,
                    size.join(" x "),
                    centre.join(", ")
                )
            }
            Err(e) => e.to_string(),
        };
        return format!("mesh, {}, {detail}", line.trim_end());
    }
    let kind = if starts(b"\x89PNG") {
        "PNG image"
    } else if starts(b"\xFF\xD8\xFF") {
        "JPEG image"
    } else if starts(b"DDS ") {
        "DDS texture"
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        "WAV sound"
    } else if starts(b"OggS") {
        "Ogg sound"
    } else if starts(b"ID3") || starts(b"\xFF\xFB") || starts(b"\xFF\xF3") {
        "MP3 sound"
    } else if starts(b"<roblox!") {
        "binary model"
    } else if starts(b"<roblox") {
        "XML model"
    } else {
        "unknown"
    };
    kind.to_owned()
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
    /// Uses as a mesh (`MeshId` of a mesh class or MeshPart).
    mesh: usize,
    /// Uses as an image (textures, decals, images, sky faces).
    image: usize,
    /// `Class.Property` of the first use.
    property: String,
    /// Full name of the first instance using it.
    instance: String,
}

/// What a property of `class` holds (`Want::for_property`).
fn property_want(class: &str, key: &str) -> Want {
    let mesh_owner = matches!(
        class,
        "SpecialMesh" | "FileMesh" | "CharacterMesh" | "MeshPart"
    );
    Want::for_property(key, mesh_owner)
}

/// The mesh version a file starts with (`version 2.00`).
fn mesh_version(bytes: &[u8]) -> Option<String> {
    let head = bytes.get(..bytes.len().min(16))?;
    let line = head.split(|&b| b == b'\n').next()?;
    let line = std::str::from_utf8(line).ok()?;
    Some(line.trim_end().strip_prefix("version ")?.to_owned())
}

/// The mesh or image references in a place: how many decode, by version
/// or format, and which fail and why.
fn print_decoded(
    what: &str,
    refs: &BTreeMap<ContentRef, Vec<&str>>,
    uses: &BTreeMap<String, Use>,
    decoded: &BTreeMap<ContentRef, Result<Fetched, Failure>>,
    first_use: &dyn Fn(&[&str]) -> String,
) {
    if decoded.is_empty() {
        return;
    }
    let mut by_result: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failed = Vec::new();
    for (r, outcome) in decoded {
        let texts = &refs[r];
        let n: usize = texts
            .iter()
            .map(|t| {
                if what == "Meshes" {
                    uses[*t].mesh
                } else {
                    uses[*t].image
                }
            })
            .sum();
        let key = match outcome {
            Ok(f) => match &f.image {
                Some(image) => format!("decoded, {}", image.format.name()),
                None => format!(
                    "decoded, version {}",
                    mesh_version(&f.bytes).unwrap_or_default()
                ),
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
    }
    let started = std::time::Instant::now();
    let mut results: BTreeMap<ContentRef, Result<Fetched, Failure>> = BTreeMap::new();
    let mut meshes: BTreeMap<ContentRef, Result<Fetched, Failure>> = BTreeMap::new();
    let mut images: BTreeMap<ContentRef, Result<Fetched, Failure>> = BTreeMap::new();
    for (t, outcome) in loader.wait_all() {
        if let Some(r) = tickets.remove(&t) {
            results.insert(r, outcome);
        } else if let Some(r) = mesh_tickets.remove(&t) {
            meshes.insert(r, outcome);
        } else if let Some(r) = image_tickets.remove(&t) {
            images.insert(r, outcome);
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
    print_decoded("Meshes", &refs, &uses, &meshes, &first_use);
    print_decoded("Images", &refs, &uses, &images, &first_use);
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
