//! `rook terrain`: what a file's Terrain holds, or how many files in a
//! folder hold smooth voxels (`SmoothGrid`) and how many legacy blocks
//! (`ClusterGrid`, `ClusterGridV2`, `ClusterGridV3`).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use rayon::prelude::*;
use rbx_dom_weak::WeakDom;
use rbx_types::Variant;
use rook_terrain::{Grid, TerrainMaterial, chunks_to_mesh, mesh_chunk};

use crate::commands::corpus::find_files;

const EXTENSIONS: [&str; 4] = ["rbxl", "rbxlx", "rbxm", "rbxmx"];
const LEGACY: [&str; 3] = ["ClusterGrid", "ClusterGridV2", "ClusterGridV3"];

/// One Terrain's saved data.
#[derive(Debug, Default)]
struct Found {
    /// The decoded `SmoothGrid`, or why it didn't decode.
    smooth: Option<Result<Grid, String>>,
    /// Bytes in each legacy property that has any.
    legacy: Vec<(&'static str, usize)>,
}

fn bytes(v: &Variant) -> Option<&[u8]> {
    match v {
        Variant::BinaryString(b) => Some(b.as_ref()),
        Variant::String(s) => Some(s.as_bytes()),
        _ => None,
    }
}

fn terrains(dom: &WeakDom) -> Vec<Found> {
    dom.descendants()
        .filter(|i| i.class == "Terrain")
        .map(|t| {
            let mut found = Found::default();
            for (name, value) in &t.properties {
                let Some(data) = bytes(value) else { continue };
                if name.as_str() == "SmoothGrid" {
                    found.smooth = Some(Grid::decode(data).map_err(|e| e.to_string()));
                } else if let Some(legacy) = LEGACY.iter().find(|l| **l == name.as_str())
                    && !data.is_empty()
                {
                    found.legacy.push((legacy, data.len()));
                }
            }
            found
        })
        .collect()
}

fn material_line(counts: &[u64; 23]) -> String {
    let mut by: Vec<(u64, TerrainMaterial)> = TerrainMaterial::ALL
        .into_iter()
        .map(|m| (counts[m as usize], m))
        .filter(|(n, _)| *n > 0)
        .collect();
    by.sort_by_key(|b| std::cmp::Reverse(b.0));
    by.iter()
        .map(|(n, m)| format!("{} {n}", m.name()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Print what each file's Terrain holds, and how long meshing it takes.
pub fn inspect(files: &[PathBuf]) -> anyhow::Result<()> {
    for file in files {
        let loaded = rook_formats::load(&fs_err::read(file)?)?;
        println!("{}", file.display());
        let found = terrains(&loaded.dom);
        if found.is_empty() {
            println!("  no Terrain");
        }
        for t in found {
            for (name, len) in &t.legacy {
                println!("  {name}: {len} bytes (legacy terrain)");
            }
            match t.smooth {
                None => println!("  no SmoothGrid"),
                Some(Err(e)) => println!("  SmoothGrid doesn't decode: {e}"),
                Some(Ok(grid)) => {
                    let chunks: Vec<_> = grid.chunks().collect();
                    let solid: u64 = grid.counts().iter().sum();
                    println!("  SmoothGrid: {} chunks, {solid} voxels", chunks.len());
                    if solid == 0 {
                        continue;
                    }
                    println!("    {}", material_line(&grid.counts()));
                    let lo = chunks.iter().fold(chunks[0], |a, c| a.min(*c));
                    let hi = chunks.iter().fold(chunks[0], |a, c| a.max(*c));
                    println!("    chunks {lo} to {hi} (128 studs each)");
                    let started = Instant::now();
                    let meshed = chunks_to_mesh(&grid);
                    let triangles: usize = meshed
                        .par_iter()
                        .map(|c| mesh_chunk(&grid, *c).triangles())
                        .sum();
                    println!(
                        "    mesh: {} chunks, {triangles} triangles, {:.0} ms",
                        meshed.len(),
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
            }
        }
    }
    Ok(())
}

/// What one file holds, for the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    NoTerrain,
    Empty,
    Smooth,
    Legacy,
    Both,
    Damaged,
}

/// Count, over the unique files under `dir`, which hold smooth voxels and
/// which legacy blocks.
pub fn scan(dir: &Path, jobs: Option<usize>) -> anyhow::Result<()> {
    let started = Instant::now();
    let files = find_files(dir, &EXTENSIONS);
    eprintln!("found {} place/model files", files.len());
    let seen = Mutex::new(HashSet::new());
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.unwrap_or(8))
        .build()?;
    let results: Vec<(PathBuf, Kind, [u64; 23], usize)> = pool.install(|| {
        files
            .par_iter()
            .filter_map(|entry| {
                let path = entry.path();
                let data = fs_err::read(path).ok()?;
                if !seen.lock().unwrap().insert(blake3::hash(&data)) {
                    return None;
                }
                // XML without a Terrain element needn't be decoded.
                let xml = data.starts_with(b"<roblox") || data.starts_with(b"\xef\xbb\xbf<roblox");
                if xml && !data.windows(15).any(|w| w == b"class=\"Terrain\"") {
                    return Some((path.to_owned(), Kind::NoTerrain, [0; 23], 0));
                }
                let loaded = rook_formats::load(&data).ok()?;
                let mut kind = Kind::NoTerrain;
                let mut counts = [0u64; 23];
                let mut legacy_bytes = 0;
                for t in terrains(&loaded.dom) {
                    let smooth = match &t.smooth {
                        Some(Ok(grid)) => {
                            let c = grid.counts();
                            for (total, n) in counts.iter_mut().zip(c) {
                                *total += n;
                            }
                            c.iter().any(|&n| n > 0)
                        }
                        Some(Err(_)) => {
                            kind = kind.max(Kind::Damaged);
                            false
                        }
                        None => false,
                    };
                    legacy_bytes += t.legacy.iter().map(|(_, n)| n).sum::<usize>();
                    let this = match (smooth, !t.legacy.is_empty()) {
                        (true, true) => Kind::Both,
                        (true, false) => Kind::Smooth,
                        (false, true) => Kind::Legacy,
                        (false, false) => Kind::Empty,
                    };
                    kind = kind.max(this);
                }
                Some((path.to_owned(), kind, counts, legacy_bytes))
            })
            .collect()
    });
    let mut by_kind: BTreeMap<Kind, usize> = BTreeMap::new();
    let mut totals = [0u64; 23];
    let mut legacy_sizes: BTreeMap<usize, usize> = BTreeMap::new();
    for (_, kind, counts, legacy) in &results {
        *by_kind.entry(*kind).or_default() += 1;
        if matches!(kind, Kind::Smooth | Kind::Both) {
            for (t, n) in totals.iter_mut().zip(counts) {
                *t += n;
            }
        }
        if *legacy > 0 {
            *legacy_sizes.entry(*legacy).or_default() += 1;
        }
    }
    println!(
        "{} unique files loaded ({:.0} s)",
        results.len(),
        started.elapsed().as_secs_f64()
    );
    for (kind, n) in &by_kind {
        println!("  {kind:?}: {n}");
    }
    println!("smooth voxels by material: {}", material_line(&totals));
    let mut common: Vec<_> = legacy_sizes.iter().collect();
    common.sort_by(|a, b| b.1.cmp(a.1));
    println!(
        "most common legacy sizes (bytes: files): {:?}",
        &common[..common.len().min(8)]
    );
    let mut largest: Vec<_> = results
        .iter()
        .filter(|r| matches!(r.1, Kind::Smooth | Kind::Both))
        .map(|r| (r.2.iter().sum::<u64>(), &r.0))
        .collect();
    largest.sort_by_key(|l| std::cmp::Reverse(l.0));
    println!("most smooth voxels:");
    for (n, path) in largest.iter().take(12) {
        println!("  {n} {}", path.display());
    }
    let mut legacy: Vec<_> = results
        .iter()
        .filter(|r| matches!(r.1, Kind::Legacy | Kind::Both))
        .map(|r| (r.3, &r.0))
        .collect();
    legacy.sort_by_key(|l| std::cmp::Reverse(l.0));
    println!("most legacy terrain:");
    for (n, path) in legacy.iter().take(12) {
        println!("  {n} bytes {}", path.display());
    }
    Ok(())
}
