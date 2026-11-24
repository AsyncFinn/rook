//! `cargo xtask vendor-packages`: copy the packages Rook's CoreScripts
//! require into `crates/rook-engine/corescripts/Packages/`, at the pins
//! in `corescripts/packages.toml`, from GitHub archives and Wally. Only
//! Luau modules are kept (no tests or Jest configs), plus each package's
//! license.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pins {
    #[serde(default)]
    github: Vec<GitHub>,
    #[serde(default)]
    wally: BTreeMap<String, String>,
    #[serde(default)]
    license: Vec<License>,
}

/// Packages from one repository's archive, each a folder in it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitHub {
    repo: String,
    commit: String,
    license: String,
    packages: BTreeMap<String, String>,
}

/// A license file for packages whose own archive has none.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct License {
    repo: String,
    commit: String,
    path: String,
    packages: Vec<String>,
}

/// An archive's files by their path in it (`/`-separated).
type Files = BTreeMap<String, Vec<u8>>;

pub(crate) fn run(corescripts: &Path) -> anyhow::Result<()> {
    let pins_path = corescripts.join("packages.toml");
    let pins: Pins = toml::from_str(&fs_err::read_to_string(&pins_path)?)
        .with_context(|| pins_path.display().to_string())?;
    let mut packages: BTreeMap<String, Files> = BTreeMap::new();

    for gh in &pins.github {
        let url = format!("https://codeload.github.com/{}/zip/{}", gh.repo, gh.commit);
        let files = unzip(&get(&url, &[])?)?;
        // GitHub's archives hold one top folder, `<repo>-<commit>/`.
        let files: Files = files
            .into_iter()
            .filter_map(|(k, v)| Some((k.split_once('/')?.1.to_owned(), v)))
            .collect();
        let license = files
            .get(&gh.license)
            .with_context(|| format!("{} has no {}", gh.repo, gh.license))?;
        for (name, folder) in &gh.packages {
            let mut kept = modules(&files, folder);
            if kept.is_empty() {
                bail!("{}: {folder} holds no modules", gh.repo);
            }
            kept.insert("LICENSE".into(), license.clone());
            packages.insert(name.clone(), kept);
        }
    }

    for (name, spec) in &pins.wally {
        let (package, version) = spec
            .split_once('@')
            .with_context(|| format!("{name}: {spec} isn't scope/name@version"))?;
        let url = format!("https://api.wally.run/v1/package-contents/{package}/{version}");
        let files = unzip(&get(&url, &[("Wally-Version", "0.3.2")])?)?;
        // Its Rojo project names the folder the package is.
        let project: serde_json::Value = serde_json::from_slice(
            files
                .get("default.project.json")
                .with_context(|| format!("{spec} has no default.project.json"))?,
        )?;
        let root = project["tree"]["$path"]
            .as_str()
            .with_context(|| format!("{spec}: its project names no $path"))?;
        let mut kept = modules(&files, root);
        if kept.is_empty() {
            bail!("{spec}: {root} holds no modules");
        }
        if let Some(license) = ["LICENSE", "LICENSE.txt", "LICENSE.md"]
            .iter()
            .find_map(|l| files.get(*l))
        {
            kept.insert("LICENSE".into(), license.clone());
        }
        packages.insert(name.clone(), kept);
    }

    for license in &pins.license {
        let url = format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            license.repo, license.commit, license.path
        );
        let text = get(&url, &[])?;
        for name in &license.packages {
            packages
                .get_mut(name)
                .with_context(|| format!("[[license]] names {name}, which isn't pinned"))?
                .insert("LICENSE".into(), text.clone());
        }
    }

    if let Some((name, _)) = packages.iter().find(|(_, f)| !f.contains_key("LICENSE")) {
        bail!("{name} has no license file; add a [[license]] for it");
    }

    let out = corescripts.join("Packages");
    if out.exists() {
        fs_err::remove_dir_all(&out)?;
    }
    let mut count = 0;
    for (name, files) in &packages {
        for (path, bytes) in files {
            let to: PathBuf = out.join(name).join(path);
            fs_err::create_dir_all(to.parent().expect("a file's folder"))?;
            fs_err::write(&to, normalize(bytes))?;
            count += 1;
        }
    }
    println!(
        "wrote {} packages ({count} files) to {}",
        packages.len(),
        out.display()
    );
    Ok(())
}

/// The Luau modules under `folder`, by their path below it, leaving out
/// tests and Jest's configuration.
fn modules(files: &Files, folder: &str) -> Files {
    let prefix = format!("{}/", folder.trim_end_matches('/'));
    files
        .iter()
        .filter_map(|(path, bytes)| {
            let rel = path.strip_prefix(&prefix)?;
            let luau = rel.ends_with(".lua") || rel.ends_with(".luau");
            let test = rel.split('/').any(|p| p == "__tests__" || p == "__testUtils__")
                || [".spec.lua", ".test.lua", ".spec.luau", ".test.luau"]
                    .iter()
                    .any(|s| rel.ends_with(s))
                || rel.rsplit('/').next() == Some("jest.config.lua");
            (luau && !test).then(|| (rel.to_owned(), bytes.clone()))
        })
        .collect()
}

/// Line endings as `\n`, so a refresh on Windows makes no diff.
fn normalize(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn get(url: &str, headers: &[(&str, &str)]) -> anyhow::Result<Vec<u8>> {
    let mut request = ureq::get(url).header("User-Agent", "rook-xtask");
    for (k, v) in headers {
        request = request.header(*k, *v);
    }
    let mut response = request.call().with_context(|| url.to_owned())?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(256 * 1024 * 1024)
        .read_to_vec()
        .with_context(|| url.to_owned())?;
    Ok(bytes)
}

fn unzip(bytes: &[u8]) -> anyhow::Result<Files> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut files = Files::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name()?.replace('\\', "/");
        if name.split('/').any(|p| p == ".." || p.contains(':')) {
            bail!("{name} leaves the archive's folder");
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        files.insert(name, data);
    }
    Ok(files)
}
