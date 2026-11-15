//! `rook cache`: the download cache's folder and size, or a clean-up.

/// What `rook cache` does.
#[derive(Clone, Copy)]
pub(crate) enum CacheCommand {
    Show,
    /// Delete what hasn't been used for this many days.
    Prune(u64),
    Trim,
    Clear,
}

/// `rook cache`: the download cache's folder and size, or a clean-up.
pub(crate) fn cache(command: CacheCommand) -> anyhow::Result<()> {
    let config = rook_engine::settings::content_config(false);
    let dir = config
        .cache_dir
        .ok_or_else(|| anyhow::anyhow!("no data folder for the cache"))?;
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
        "limit:  {:.0} MB (the build's cacheMaxSize)",
        mb(rook_engine::settings::cache_max()),
    );
    Ok(())
}
