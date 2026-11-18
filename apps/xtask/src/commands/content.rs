//! `cargo xtask content`: how content references resolve.
//!
//! `cargo xtask content <reference>...` shows what each reference means and where
//! its bytes come from; `cargo xtask content --place <file>` does the same for
//! every content property in a place and sums it up.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use rbx_dom_weak::WeakDom;
use rbx_types::{Ref, Variant};
use rook_content::{ContentRef, Failure, FailureKind, Fetched, Loader, Resolver, SourceKind, Want};

mod describe;
mod place;
mod references;

use describe::describe;
pub(crate) use place::place;
pub(crate) use references::references;

pub(crate) struct Options {
    /// Never use the network: only the install, folders and the cache.
    pub offline: bool,
    /// Forget cached answers for these references first.
    pub refresh: bool,
}

fn resolver(options: &Options) -> Arc<Resolver> {
    // Asset delivery is on by default here: this command exists to show
    // where content comes from. assetDelivery off also turns it off.
    let mut config = rook_engine::settings::content_config(true);
    if options.offline {
        config.network = false;
    }
    let resolver = Resolver::new(config);
    print_setup(&resolver);
    Arc::new(resolver)
}

fn print_setup(r: &Resolver) {
    match r.packages() {
        Some(p) => println!(
            "Roblox packages: {} ({}, {})",
            p.dir().display(),
            p.version().as_deref().unwrap_or("none yet"),
            if r.downloads_packages() {
                "downloaded as needed"
            } else {
                "downloads off"
            }
        ),
        None => println!("Roblox packages: off (no data folder)"),
    }
    match r.install() {
        Some(i) => println!("Roblox install: {} ({:?})", i.root.display(), i.kind),
        None => println!(
            "Roblox install: none found (set {} to a version or content folder)",
            rook_engine::settings::ROBLOX_INSTALL.name
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
        (true, true) => "on, anonymous, then an Open Cloud key if one is saved",
    };
    println!("asset delivery: {delivery}");
    match r.cache() {
        Some(c) => println!("cache:          {}", c.dir().display()),
        None => println!("cache:          none"),
    }
    println!();
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
