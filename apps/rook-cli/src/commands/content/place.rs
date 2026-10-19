//! `rook content <place>`: every content reference in a place or model,
//! how many resolve and decode, and a load report of each one's outcome.

use super::*;

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
    use rook_content::decode::mesh::union::{MeshDataKind, asset_mesh_data};
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
            Err(e) if matches!(e.kind, FailureKind::Invalid | FailureKind::UnknownVersion) => {
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
    rook_engine::session::threads::size_threads();
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
    // The load report: every reference and what became of it.
    println!("\nEvery reference:");
    for (r, texts) in &refs {
        let decoded = [&meshes, &images, &sounds]
            .into_iter()
            .find_map(|d| d.get(r));
        let outcome = match (&results[r], decoded) {
            (Err(e), _) => format!("failed: {e}"),
            (Ok(_), Some(Err(e))) => format!("loaded, didn't decode: {e}"),
            (Ok(f), _) => format!(
                "{}, {} bytes ({})",
                f.source.kind.name(),
                f.bytes.len(),
                describe(&f.bytes)
            ),
        };
        println!("  {r}  {outcome}\n      {}", first_use(texts));
    }
    Ok(())
}
