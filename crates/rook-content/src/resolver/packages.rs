//! Roblox's client packages, downloaded from Roblox's deployment CDN as
//! Roblox's own bootstrapper installs them, so `rbxasset://` paths and the
//! CoreScripts resolve with no Roblox install. A package is downloaded only
//! when a path needs its folder, checked against the version's manifest,
//! and extracted into the data folder's `content\packages`, laid out
//! like an install's with its `content` folder as the top. A marker names
//! the version the folder holds; when the live version differs, the
//! packages are removed
//! and come again as they're needed. Without the network, what is there
//! answers. See docs/behavior/content/client-packages.md.

use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use rook_platform::endpoints::Endpoints;
use rook_platform::network::Request;

/// Studio's live version (`clientVersionUpload`). Studio's packages hold
/// the CoreScripts model Rook runs, `DataModelPatch.rbxm`; the player's
/// `InExperience.rbxm` doesn't load in Rook yet. Their content packages
/// are the same files.
const CLIENT_VERSION: &str = "/v2/client-version/WindowsStudio64";

/// The folders `rbxasset://` paths are looked up in, in the order an
/// install's are ([`super::install::Install::content`]): its `content`
/// folder is the top here.
const ROOTS: [&str; 3] = ["", "ExtraContent", "PlatformContent/pc"];

/// Each content package and the folder it extracts to. Checked against
/// the manifests and an install on 2026-10-23.
const FOLDERS: &[(&str, &str)] = &[
    ("content-avatar.zip", "avatar"),
    ("content-configs.zip", "configs"),
    ("content-fonts.zip", "fonts"),
    ("content-models.zip", "models"),
    ("content-sky.zip", "sky"),
    ("content-sounds.zip", "sounds"),
    ("content-textures2.zip", "textures"),
    ("extracontent-luapackages.zip", "ExtraContent/LuaPackages"),
    ("extracontent-models.zip", "ExtraContent/models"),
    ("extracontent-textures.zip", "ExtraContent/textures"),
    ("extracontent-translations.zip", "ExtraContent/translations"),
    (
        "content-platform-dictionaries.zip",
        "PlatformContent/pc/shared_compression_dictionaries",
    ),
    ("content-platform-fonts.zip", "PlatformContent/pc/fonts"),
    ("content-terrain.zip", "PlatformContent/pc/terrain"),
    ("content-textures3.zip", "PlatformContent/pc/textures"),
];

/// The version the folder's packages are from.
const VERSION: &str = "version.txt";
/// That version's package manifest, so it's downloaded once.
const MANIFEST: &str = "manifest.txt";

/// Roblox's packages in Rook's content folder, downloaded on first need.
/// Shared by every resolver of a run, so the live version is asked once.
#[derive(Debug)]
pub struct Packages {
    dir: PathBuf,
    /// The live version's endpoint; `None` downloads nothing.
    client_version: Option<String>,
    /// The CDN's base; `None` downloads nothing.
    setup: Option<String>,
    live: OnceLock<Option<Live>>,
    downloading: Mutex<()>,
    /// Packages that failed this run, so they aren't tried again.
    failed: Mutex<HashSet<&'static str>>,
}

/// The live version and its manifest.
#[derive(Debug)]
struct Live {
    version: String,
    manifest: Vec<Package>,
}

/// One manifest entry.
#[derive(Debug, PartialEq, Eq)]
struct Package {
    name: String,
    md5: String,
    packed: u64,
}

impl Packages {
    /// Packages kept in `dir`, downloaded through the `clientsettings` and
    /// `setup` domains' endpoints.
    pub fn new(dir: PathBuf, endpoints: &Endpoints) -> Self {
        Self {
            dir,
            client_version: endpoints.url("clientsettings", CLIENT_VERSION),
            setup: endpoints.base("setup"),
            live: OnceLock::new(),
            downloading: Mutex::new(()),
            failed: Mutex::new(HashSet::new()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether packages may be downloaded at all.
    pub fn downloads(&self) -> bool {
        self.client_version.is_some() && self.setup.is_some()
    }

    /// The version the folder holds, if any.
    pub fn version(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.dir.join(VERSION)).ok()?;
        Some(text.trim().to_owned()).filter(|v| is_version(v))
    }

    /// The file an `rbxasset://` path names, downloading the package that
    /// holds its folder first when `download` allows.
    pub fn find(&self, path: &str, download: bool) -> Option<PathBuf> {
        let first = path.split('/').find(|p| !p.is_empty() && *p != ".")?;
        let live = if download && self.downloads() {
            self.live.get_or_init(|| self.ask_live()).as_ref()
        } else {
            None
        };
        for root in ROOTS {
            let folder = match root {
                "" => first.to_owned(),
                _ => format!("{root}/{first}"),
            };
            let Some(&(package, folder)) = FOLDERS
                .iter()
                .find(|(_, f)| f.eq_ignore_ascii_case(&folder))
            else {
                continue;
            };
            let target = self.dir.join(folder);
            if !target.is_dir() {
                let Some(live) = live else { continue };
                if let Err(e) = self.fetch(live, package, &target) {
                    tracing::warn!(target: "content", "Roblox's {package}: {e}");
                    self.failed.lock().expect("failed packages").insert(package);
                    continue;
                }
            }
            if let Some(file) = super::find_file(&self.dir.join(root), path) {
                return Some(file);
            }
        }
        None
    }

    /// The live version and its manifest. When the folder holds another
    /// version, its packages are removed first. Failures are logged: the
    /// packages already downloaded still answer.
    fn ask_live(&self) -> Option<Live> {
        match self.live_version() {
            Ok(live) => {
                tracing::debug!(target: "content", "Roblox's live client is {}", live.version);
                Some(live)
            }
            Err(e) => {
                tracing::warn!(
                    target: "content",
                    "couldn't ask Roblox for its client version ({e}); using the packages already downloaded"
                );
                None
            }
        }
    }

    fn live_version(&self) -> Result<Live, String> {
        let url = self.client_version.as_deref().expect("downloads are on");
        let answer = get(url, 1 << 20)?;
        let json: serde_json::Value =
            serde_json::from_slice(&answer).map_err(|_| format!("unexpected answer from {url}"))?;
        let version = json["clientVersionUpload"]
            .as_str()
            .filter(|v| is_version(v))
            .ok_or_else(|| format!("no clientVersionUpload from {url}"))?
            .to_owned();
        let _one_at_a_time = self.downloading.lock().expect("package downloads");
        let manifest_file = self.dir.join(MANIFEST);
        let held = self.version();
        let saved = match held.as_deref() == Some(version.as_str()) {
            true => std::fs::read_to_string(&manifest_file).ok(),
            false => None,
        };
        let text = match saved {
            Some(text) => text,
            None => {
                let setup = self.setup.as_deref().expect("downloads are on");
                let bytes = get(&format!("{setup}/{version}-rbxPkgManifest.txt"), 1 << 20)?;
                let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                if held.is_some_and(|h| h != version) {
                    self.remove_packages(&version)?;
                }
                let write = |name: &str, text: &str| {
                    std::fs::write(self.dir.join(name), text)
                        .map_err(|e| format!("{}: {e}", self.dir.join(name).display()))
                };
                std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
                write(MANIFEST, &text)?;
                write(VERSION, &version)?;
                text
            }
        };
        let manifest = parse_manifest(&text).ok_or("the package manifest isn't valid")?;
        Ok(Live { version, manifest })
    }

    /// Download `package`, check it and extract it to `target`.
    fn fetch(&self, live: &Live, package: &'static str, target: &Path) -> Result<(), String> {
        let _one_at_a_time = self.downloading.lock().expect("package downloads");
        if target.is_dir() {
            return Ok(());
        }
        if self
            .failed
            .lock()
            .expect("failed packages")
            .contains(package)
        {
            return Err("failed earlier this run".into());
        }
        let entry = live
            .manifest
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(package))
            .ok_or("not in this version's manifest")?;
        tracing::info!(
            target: "content",
            "downloading Roblox's {package} ({:.1} MB) for {} into {}",
            entry.packed as f64 / 1e6,
            live.version,
            self.dir.display()
        );
        let setup = self.setup.as_deref().expect("downloads are on");
        let bytes = get(
            &format!("{setup}/{}-{package}", live.version),
            entry.packed + 1,
        )?;
        let md5 = format!("{:x}", md5::compute(&bytes));
        if !md5.eq_ignore_ascii_case(&entry.md5) {
            return Err(format!("MD5 {md5} isn't the manifest's {}", entry.md5));
        }
        let partial = partial(target);
        let _ = std::fs::remove_dir_all(&partial);
        extract(&bytes, &partial).map_err(|e| format!("extracting: {e}"))?;
        if let Err(e) = std::fs::rename(&partial, target) {
            let _ = std::fs::remove_dir_all(&partial);
            // Another Rook finished it first.
            if !target.is_dir() {
                return Err(format!("{}: {e}", target.display()));
            }
        }
        Ok(())
    }

    /// Remove every package's folder, before `version`'s come. A file
    /// still open elsewhere (another Rook running) can't go: the folder
    /// keeps its version this run, and the removal is tried again next run.
    fn remove_packages(&self, version: &str) -> Result<(), String> {
        tracing::info!(
            target: "content",
            "Roblox's client is now {version}: removing the packages of the version before"
        );
        for (_, folder) in FOLDERS {
            let path = self.dir.join(folder);
            if path.exists() {
                std::fs::remove_dir_all(&path)
                    .map_err(|e| format!("couldn't remove {}: {e}", path.display()))?;
            }
        }
        Ok(())
    }
}

/// Where a package is extracted before it's moved into place: beside it,
/// named for this process so two Rooks don't share one.
fn partial(target: &Path) -> PathBuf {
    let leaf = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("package");
    target.with_file_name(format!(".{leaf}.partial-{}", std::process::id()))
}

/// `version-<hex>`, as Roblox names them.
fn is_version(name: &str) -> bool {
    name.strip_prefix("version-")
        .is_some_and(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))
}

fn get(url: &str, max_bytes: u64) -> Result<Vec<u8>, String> {
    let response = rook_platform::get().network.send(&Request {
        method: "GET",
        url,
        headers: &[],
        body: &[],
        max_bytes,
    })?;
    match response.status {
        200 => Ok(response.body),
        status => Err(format!("HTTP {status} from {url}")),
    }
}

/// `v0`, then four lines per package: its name, MD5, packed and unpacked
/// size.
fn parse_manifest(text: &str) -> Option<Vec<Package>> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if lines.next()? != "v0" {
        return None;
    }
    let lines: Vec<&str> = lines.collect();
    lines
        .chunks(4)
        .map(|entry| match *entry {
            [name, md5, packed, _unpacked] => Some(Package {
                name: name.to_owned(),
                md5: md5.to_owned(),
                packed: packed.parse().ok()?,
            }),
            _ => None,
        })
        .collect()
}

/// Unzip `zip` into `into`. Roblox's packages name entries relative to
/// their folder, with backslashes; entries that would leave it are
/// refused.
fn extract(zip: &[u8], into: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().map_err(|e| e.to_string())?.into_owned();
        let parts: Vec<&str> = name
            .split(['/', '\\'])
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        if parts.iter().any(|p| *p == ".." || p.contains(':')) {
            return Err(format!("{name} leaves the package's folder"));
        }
        let path = parts.iter().fold(into.to_path_buf(), |p, c| p.join(c));
        if file.is_dir() || parts.is_empty() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        let mut bytes = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_list_four_lines_per_package() {
        let text = "v0\r\ncontent-fonts.zip\r\nabc\r\n10\r\n20\r\nssl.zip\r\ndef\r\n1\r\n2\r\n";
        let manifest = parse_manifest(text).unwrap();
        assert_eq!(manifest.len(), 2);
        assert_eq!(manifest[0].name, "content-fonts.zip");
        assert_eq!(manifest[0].packed, 10);
        assert!(parse_manifest("v1\r\n").is_none());
        assert!(parse_manifest("v0\r\nx.zip\r\nabc\r\n").is_none());
    }

    #[test]
    fn versions_are_roblox_version_names() {
        assert!(is_version("version-9ac8595712b447a7"));
        assert!(!is_version("version-"));
        assert!(!is_version("version-../x"));
    }

    #[test]
    fn already_extracted_packages_answer_offline() {
        let dir = crate::tests::temp_dir("packages");
        std::fs::create_dir_all(dir.join("fonts")).unwrap();
        std::fs::create_dir_all(dir.join("ExtraContent/models/DataModelPatch")).unwrap();
        std::fs::write(dir.join("fonts/a.ttf"), b"font").unwrap();
        std::fs::write(dir.join("ExtraContent/models/DataModelPatch/p.rbxm"), b"m").unwrap();
        let packages = Packages::new(dir.clone(), &Endpoints::offline());
        assert!(!packages.downloads());
        assert_eq!(
            packages.find("fonts/a.ttf", true),
            Some(dir.join("fonts/a.ttf"))
        );
        assert_eq!(
            packages.find("models/DataModelPatch/p.rbxm", true),
            Some(dir.join("ExtraContent/models/DataModelPatch/p.rbxm"))
        );
        assert_eq!(packages.find("blobs/x", true), None);
        assert_eq!(packages.find("sounds/x.ogg", true), None);
    }
}
