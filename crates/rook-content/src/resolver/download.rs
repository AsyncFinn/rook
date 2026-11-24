//! Content fetched over the network: assets from the asset delivery API,
//! texture packs and thumbnails, kept in the cache once read.

use std::time::{Duration, SystemTime};

use super::*;
use crate::Thumbnail;
use crate::decode::image::ktx2;

/// How long to wait before asking again for a texture being made
/// (Studio 0.748 waited two seconds).
const BEING_GENERATED_RETRY: Duration = Duration::from_secs(2);

/// How long a stored thumbnail is used before the service is asked again:
/// avatars and icons change, and Studio asks every session. A stale one
/// still answers when the service can't be reached.
const THUMBNAIL_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// Larger thumbnails are refused (the largest Roblox renders are a few
/// hundred kilobytes).
const MAX_THUMBNAIL_BYTES: u64 = 16 << 20;

impl Resolver {
    /// One map of a TexturePack asset (a material's, [`materials`], or
    /// terrain's): the pack names an image per map, and both come through
    /// the cache and asset delivery like any asset. Comes decoded; the
    /// bytes only name it.
    pub(super) fn texture_pack(&self, pack_id: u64, map: MaterialMap) -> Result<Fetched, Failure> {
        let not_found = |why: &str| Failure::new(FailureKind::NotFound, why);
        let asset = |id| ContentRef::Asset { id, version: None };
        let pack = self.resolve(&asset(pack_id))?;
        let pack = TexturePack::parse(&pack.bytes)
            .ok_or_else(|| Failure::new(FailureKind::Invalid, "not a TexturePack"))?;
        let main = match map {
            MaterialMap::Color => pack.color,
            MaterialMap::Normal | MaterialMap::DetailNormal => pack.normal,
            MaterialMap::RoughnessMetalness => pack.roughness,
        }
        .ok_or_else(|| not_found("the material's texture pack has no such map"))?;
        let image = self.resolve(&asset(main))?;
        let extra = |id: Option<u64>| match (map, id) {
            (MaterialMap::RoughnessMetalness, Some(id)) => self.resolve(&asset(id)).map(Some),
            _ => Ok(None),
        };
        let metalness = extra(pack.metalness)?;
        let decoded = materials::build(
            map,
            &image.bytes,
            metalness.as_ref().map(|m| &*m.bytes),
            pack.tint_mask,
        )
        .map_err(|e| Failure::new(FailureKind::Invalid, e.to_string()))?;
        let name = format!(
            "{} rbxassetid://{pack_id} {} {}x{}",
            image.source.location,
            map.name(),
            decoded.width,
            decoded.height
        );
        let mut fetched = Fetched::new(name.as_bytes(), image.source.kind, name.clone());
        fetched.image = Some(std::sync::Arc::new(decoded));
        Ok(fetched)
    }

    /// An asset id, from the cache or asset delivery; a kept asset
    /// ([`kept`]) at its pinned version, from the kept folder first.
    pub(super) fn download(&self, r: &ContentRef) -> Result<Fetched, Failure> {
        let pinned = match r {
            ContentRef::Asset { id, version: None } => {
                kept::version(*id).map(|v| ContentRef::Asset {
                    id: *id,
                    version: Some(v),
                })
            }
            _ => None,
        };
        let r = pinned.as_ref().unwrap_or(r);
        let key = r.cache_key().expect("downloadable reference");
        // Kept content is written to the kept folder; without one it goes
        // to the cache like the rest.
        let keep = self.kept.as_ref().filter(|_| pinned.is_some());
        if let Some(Cached::Hit {
            bytes, path, url, ..
        }) = keep.map(|k| k.get(&key))
        {
            return Ok(Fetched::new(
                bytes,
                SourceKind::Kept,
                format!("{} {url}", path.display()),
            ));
        }
        match self.cache.as_ref().map(|c| c.get(&key)) {
            Some(Cached::Hit {
                bytes, path, url, ..
            }) => {
                if let Some(kept) = keep
                    && let Err(e) = kept.put(&key, &bytes, &url)
                {
                    tracing::warn!(target: "content", "couldn't keep {r}: {e}");
                }
                return Ok(Fetched::new(
                    bytes,
                    SourceKind::Cache,
                    format!("{} {url}", path.display()),
                ));
            }
            // An anonymous refusal doesn't stand once there is a key to try.
            Some(Cached::Failed(f))
                if f.kind == FailureKind::Forbidden
                    && self.delivery.as_ref().is_some_and(|d| d.key().is_some()) => {}
            Some(Cached::Failed(f)) => return Err(f),
            Some(Cached::Miss) | None => {}
        }
        let Some(delivery) = &self.delivery else {
            return Err(Failure::new(
                FailureKind::Offline,
                "not cached, and asset delivery is off",
            ));
        };
        let fetched = match r {
            ContentRef::Asset { id, version } => delivery.fetch(*id, *version),
            ContentRef::AssetVersion(_) => Err(Failure::new(
                FailureKind::Unsupported,
                "asset delivery no longer looks assets up by asset version id",
            )),
            _ => unreachable!("only assets are downloaded"),
        };
        match fetched {
            Ok(asset) => {
                let location = match keep.or(self.cache.as_ref()) {
                    Some(store) => match store.put(&key, &asset.bytes, &asset.url) {
                        Ok(path) => format!("{} -> {}", asset.url, path.display()),
                        Err(e) => {
                            tracing::warn!(
                                target: "content",
                                "couldn't save {r} in {}: {e}",
                                store.dir().display()
                            );
                            asset.url
                        }
                    },
                    None => asset.url,
                };
                Ok(Fetched::new(asset.bytes, SourceKind::Delivery, location))
            }
            Err(f) => {
                if let Some(cache) = &self.cache
                    && let Err(e) = cache.put_failure(&key, &f)
                {
                    tracing::debug!(target: "content", "couldn't remember why {r} failed: {e}");
                }
                Err(f)
            }
        }
    }

    /// An asset id wanted as a texture: Roblox's transcode of it, its mip
    /// packs from the smallest up to the first whose largest level is at
    /// least `1 << size` pixels across (or the last), joined
    /// ([`ktx2::join`]). Each pack is kept in the cache on its own
    /// (`asset-<id>-ktx2-p<n>`), so a larger request fetches only the
    /// packs it adds, several in one request; if those fail, the packs it
    /// has still make a smaller texture. Where there is no transcode (not
    /// an image, refused without an account and no key) that is remembered
    /// like any failure, so later runs go straight to the upload
    /// ([`Self::download`]) until a key is saved; one Roblox is still
    /// making is asked for again next run. A kept asset stays as kept.
    pub(super) fn download_texture(&self, r: &ContentRef, size: u8) -> Result<Fetched, Failure> {
        let ContentRef::Asset { id, version } = r else {
            return self.download(r);
        };
        if version.is_none() && kept::version(*id).is_some() {
            return self.download(r);
        }
        let base = r.cache_key().expect("an asset");
        let key = |pack: u8| format!("{base}-ktx2-p{pack}");
        // Pack 0 names the others.
        let (first, url, mut fetched_any) = match self.cached_pack(&key(0)) {
            Some(Ok((bytes, url))) => (bytes, url, false),
            Some(Err(())) => return self.download(r),
            None => {
                let Some(delivery) = &self.delivery else {
                    return self.download(r);
                };
                let mut answer = delivery.texture_packs(*id, *version, &[0]);
                // Not made yet: Roblox starts making it on this request.
                // Studio asks again after two seconds, then draws the
                // upload.
                if let Err(f) = &answer
                    && delivery::being_generated(f)
                {
                    std::thread::sleep(BEING_GENERATED_RETRY);
                    answer = delivery.texture_packs(*id, *version, &[0]);
                }
                match answer {
                    Ok(mut got) => {
                        let first = got.swap_remove(0);
                        self.keep_pack(&key(0), &first.bytes, &first.url);
                        (first.bytes, first.url, true)
                    }
                    Err(f) => {
                        if f.kind.is_lasting()
                            && let Some(cache) = &self.cache
                            && let Err(e) = cache.put_failure(&key(0), &f)
                        {
                            tracing::debug!(target: "content", "couldn't remember why {r}'s texture failed: {e}");
                        }
                        tracing::debug!(target: "content", "{r} as uploaded: no texture transcode ({f})");
                        return self.download(r);
                    }
                }
            }
        };
        let mut packs = vec![first];
        let invalid = |e: &dyn std::fmt::Display| Failure::new(FailureKind::Invalid, e.to_string());
        let wanted = ktx2::File::read(&packs[0])
            .map_err(|e| invalid(&e))?
            .packs_up_to(1 << u32::from(size.min(31)));
        let mut missing = Vec::new();
        for pack in 1..=wanted {
            match self.cached_pack(&key(pack)) {
                Some(Ok((bytes, _))) if missing.is_empty() => packs.push(bytes),
                _ => missing.push(pack),
            }
        }
        if !missing.is_empty()
            && let Some(delivery) = &self.delivery
        {
            match delivery.texture_packs(*id, *version, &missing) {
                Ok(got) => {
                    for (pack, asset) in missing.iter().zip(got) {
                        self.keep_pack(&key(*pack), &asset.bytes, &asset.url);
                        packs.push(asset.bytes);
                    }
                    fetched_any = true;
                }
                Err(f) => {
                    tracing::debug!(target: "content", "{r}'s larger levels didn't come ({f}); drawing it smaller");
                }
            }
        }
        let files: Vec<&[u8]> = packs.iter().map(Vec::as_slice).collect();
        let bytes = ktx2::join(&files).map_err(|e| invalid(&e))?;
        let kind = if fetched_any {
            SourceKind::Delivery
        } else {
            SourceKind::Cache
        };
        Ok(Fetched::new(bytes, kind, url))
    }

    /// A texture pack from the cache: `Some(Err(()))` when the cache
    /// remembers there is none (and no key could change that).
    fn cached_pack(&self, key: &str) -> Option<Result<(Vec<u8>, String), ()>> {
        match self.cache.as_ref()?.get(key) {
            Cached::Hit { bytes, url, .. } => Some(Ok((bytes.to_vec(), url))),
            // An anonymous refusal doesn't stand once there is a key to try.
            Cached::Failed(f)
                if f.kind == FailureKind::Forbidden
                    && self.delivery.as_ref().is_some_and(|d| d.key().is_some()) =>
            {
                None
            }
            Cached::Failed(_) => Some(Err(())),
            Cached::Miss => None,
        }
    }

    fn keep_pack(&self, key: &str, bytes: &[u8], url: &str) {
        if let Some(cache) = &self.cache
            && let Err(e) = cache.put(key, bytes, url)
        {
            tracing::warn!(target: "content", "couldn't save {key} in {}: {e}", cache.dir().display());
        }
    }

    /// A thumbnail: from the cache while fresh ([`THUMBNAIL_TTL`]), else
    /// from the thumbnails service and its CDN, like an asset.
    pub(super) fn thumbnail(&self, t: &Thumbnail) -> Result<Fetched, Failure> {
        let key = t.cache_key();
        let stale = match self.cache.as_ref().map(|c| c.get(&key)) {
            Some(Cached::Hit {
                bytes,
                path,
                url,
                stored,
                ..
            }) => {
                let fetched = Fetched::new(
                    bytes,
                    SourceKind::Cache,
                    format!("{} {url}", path.display()),
                );
                let fresh = SystemTime::now()
                    .duration_since(stored)
                    .is_ok_and(|age| age < THUMBNAIL_TTL);
                if fresh {
                    return Ok(fetched);
                }
                Some(fetched)
            }
            Some(Cached::Failed(f)) => return Err(f),
            Some(Cached::Miss) | None => None,
        };
        let Some(service) = &self.thumbnails else {
            return stale.ok_or_else(|| {
                Failure::new(FailureKind::Offline, "not cached, and thumbnails are off")
            });
        };
        let fetched = service.locate(t).and_then(|url| {
            let bytes = web::download(&url, MAX_THUMBNAIL_BYTES)?;
            Ok((url, bytes))
        });
        match fetched {
            Ok((url, bytes)) => {
                let location = match &self.cache {
                    Some(cache) => match cache.put(&key, &bytes, &url) {
                        Ok(path) => format!("{url} -> {}", path.display()),
                        Err(e) => {
                            tracing::warn!(target: "content", "couldn't save {t} in {}: {e}", cache.dir().display());
                            url
                        }
                    },
                    None => url,
                };
                Ok(Fetched::new(bytes, SourceKind::Thumbnails, location))
            }
            // The last image, when the service can't be reached.
            Err(f) if !f.kind.is_lasting() => stale.ok_or(f),
            Err(f) => {
                if let Some(cache) = &self.cache
                    && let Err(e) = cache.put_failure(&key, &f)
                {
                    tracing::debug!(target: "content", "couldn't remember why {t} failed: {e}");
                }
                Err(f)
            }
        }
    }
}
