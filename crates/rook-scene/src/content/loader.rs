//! Mesh files and images for drawing, loaded off the engine thread.
//!
//! The scene asks for a file when a part's mesh or image appears or
//! changes ([`Files::state`]); the content loader's workers resolve and
//! decode it, and [`Files::poll`] hands back what finished at a fixed point
//! in the frame, so a frame never waits for disk, network or decoding. Each
//! file is kept once by the hash of its bytes; one that fails keeps why
//! ([`Files::failure`]) for the load report. A texture loads small
//! (`Want::Texture(0)`); [`Files::want`] says how large it should be and
//! [`Files::step`] grows it there a level at a time, as Studio does; each
//! larger version replaces it under the hash it first loaded by
//! ([`Files::take_upgraded`]).

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rook_content::decode::image::Image;
use rook_content::decode::mesh::Mesh;
use rook_content::{ContentRef, Failure, FailureKind, Loader, Resolver, Ticket, Want};
use rook_render::{ImageHash, MeshHash};

pub(crate) type FileRef = (ContentRef, Want);

#[derive(Debug, Clone)]
pub(crate) enum Loaded {
    Mesh(MeshHash, Arc<Mesh>),
    Image(ImageHash, Arc<Image>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileState {
    Loading,
    /// Loaded, by the hash of its bytes.
    Loaded([u8; 32]),
    /// Not found, refused, or not what was asked for (not a mesh or an
    /// image, or a version Rook can't read).
    Failed,
}

/// How long Studio 0.748 took for each level a texture grew by
/// (docs/behavior/content/asset-fetching.md#streaming).
pub(crate) const STEP: Duration = Duration::from_millis(350);
/// The largest level there is, as `Want::Texture`'s exponent (4096).
pub(crate) const LARGEST: u8 = 12;
/// How long before a texture whose larger levels failed for now (a
/// network failure, a pack being made) tries again.
const RETRY: Duration = Duration::from_secs(2);

/// A texture that loaded small, by the hash it first loaded under.
#[derive(Debug)]
struct Streamed {
    r: ContentRef,
    /// The level it's wanted at so far, as `Want::Texture`'s exponent:
    /// one more each step until `target`.
    level: u8,
    /// The level it should grow to ([`Files::want`]).
    target: u8,
    /// Its largest level's longer side now.
    side: u32,
    /// It has no larger levels to fetch: it holds them all, or they
    /// can't come.
    whole: bool,
    loading: bool,
    /// When it may step again.
    next: Instant,
    /// What its fetches queue by (lower first).
    priority: f32,
}

impl Streamed {
    fn new(r: ContentRef, side: u32, now: Instant) -> Self {
        let level = level_of(side);
        Self {
            r,
            level,
            target: level,
            side,
            whole: false,
            loading: false,
            next: now,
            priority: 0.0,
        }
    }

    /// Whether it can still come larger.
    fn can_grow(&self) -> bool {
        !self.whole && self.level < LARGEST
    }

    /// Whether it's still to step (below its target, or loading).
    fn stepping(&self) -> bool {
        self.loading || (self.can_grow() && self.level < self.target)
    }

    /// Grow a level towards the target (straight to it with `hurry`) if
    /// it last stepped [`STEP`] before `now` and isn't loading; the level
    /// to fetch when that's past what it holds.
    fn step(&mut self, now: Instant, hurry: bool) -> Option<u8> {
        if self.loading || !self.stepping() || (!hurry && now < self.next) {
            return None;
        }
        self.level = if hurry { self.target } else { self.level + 1 };
        self.next = now + STEP;
        self.loading = side_of(self.level) > self.side;
        self.loading.then_some(self.level)
    }
}

#[derive(Debug)]
pub(crate) struct Files {
    resolver: Arc<Resolver>,
    /// Made on the first request.
    loader: Option<Loader>,
    states: FxHashMap<FileRef, FileState>,
    failures: FxHashMap<FileRef, Failure>,
    tickets: FxHashMap<Ticket, FileRef>,
    streamed: FxHashMap<ImageHash, Streamed>,
    /// The streamed textures still to step: below their target, or
    /// loading a step.
    stepping: FxHashSet<ImageHash>,
    /// Larger versions of textures being loaded.
    upgrades: FxHashMap<Ticket, ImageHash>,
    /// Larger versions that arrived, for [`Files::take_upgraded`].
    upgraded: Vec<(ImageHash, Arc<Image>)>,
}

impl Files {
    pub(crate) fn new(resolver: Arc<Resolver>) -> Self {
        Self {
            resolver,
            loader: None,
            states: FxHashMap::default(),
            failures: FxHashMap::default(),
            tickets: FxHashMap::default(),
            streamed: FxHashMap::default(),
            stepping: FxHashSet::default(),
            upgrades: FxHashMap::default(),
            upgraded: Vec::new(),
        }
    }

    /// Treat `hash` as a texture that loaded small at `side` with larger
    /// levels to come, or with none (`whole`).
    #[cfg(test)]
    pub(crate) fn stream_for_test(&mut self, hash: ImageHash, side: u32, whole: bool) {
        let mut s = Streamed::new(
            ContentRef::Asset {
                id: 1,
                version: None,
            },
            side,
            Instant::now(),
        );
        s.whole = whole;
        self.streamed.insert(hash, s);
    }

    /// Whether the texture under `hash` loaded small and can still come
    /// larger.
    pub(crate) fn can_grow(&self, hash: &ImageHash) -> bool {
        self.streamed.get(hash).is_some_and(Streamed::can_grow)
    }

    /// Have each texture that loaded small (`Want::Texture(0)`) in
    /// `wanted` grow to at least `1 << size` pixels across, its fetches
    /// behind files of a lower priority ([`Files::step`] grows it); those
    /// growing but not in `wanted` (out of view) stop where they are.
    /// Nothing shrinks: a size below what a texture holds keeps it.
    pub(crate) fn want(&mut self, wanted: &FxHashMap<ImageHash, (u8, f32)>) {
        for hash in &self.stepping {
            if !wanted.contains_key(hash)
                && let Some(s) = self.streamed.get_mut(hash)
            {
                s.target = s.level;
            }
        }
        for (hash, &(size, priority)) in wanted {
            let Some(s) = self.streamed.get_mut(hash) else {
                continue;
            };
            s.priority = priority;
            s.target = size.min(LARGEST).max(s.level);
            if s.stepping() {
                self.stepping.insert(*hash);
            }
        }
    }

    /// Grow each texture below the size it's wanted at by a level, those
    /// that last stepped at least [`STEP`] before `now` and aren't still
    /// loading the last; a level past what it holds asks for the packs
    /// that make it. With `hurry`, each goes straight to its size
    /// (screenshots).
    pub(crate) fn step(&mut self, now: Instant, hurry: bool) {
        let Some(loader) = &mut self.loader else {
            return;
        };
        let streamed = &mut self.streamed;
        let upgrades = &mut self.upgrades;
        self.stepping.retain(|hash| {
            let Some(s) = streamed.get_mut(hash) else {
                return false;
            };
            if let Some(level) = s.step(now, hurry) {
                let ticket = loader.request_at(s.r.clone(), Want::Texture(level), s.priority);
                upgrades.insert(ticket, *hash);
            }
            s.stepping()
        });
    }

    /// The larger versions of textures that arrived since the last call,
    /// each under the hash it first loaded by.
    pub(crate) fn take_upgraded(&mut self) -> Vec<(ImageHash, Arc<Image>)> {
        std::mem::take(&mut self.upgraded)
    }

    fn upgrade_done(&mut self, hash: ImageHash, outcome: Result<rook_content::Fetched, Failure>) {
        let Some(s) = self.streamed.get_mut(&hash) else {
            return;
        };
        s.loading = false;
        match outcome {
            Ok(f) => {
                s.whole = !f.larger;
                if let Some(image) = f.image {
                    let side = image.width.max(image.height);
                    if side > s.side {
                        s.side = side;
                        self.upgraded.push((hash, image));
                    }
                }
            }
            // For now: back to what it holds, to step again later.
            Err(f) if !f.kind.is_lasting() => {
                s.level = level_of(s.side).min(s.level);
                s.next = Instant::now() + RETRY;
            }
            Err(_) => s.whole = true,
        }
        if !s.stepping() {
            self.stepping.remove(&hash);
        }
    }

    /// A file's state, asking for it the first time, behind files of a
    /// lower `priority`.
    pub(crate) fn state(&mut self, r: &ContentRef, want: Want, priority: f32) -> FileState {
        let key = (r.clone(), want);
        if let Some(&state) = self.states.get(&key) {
            return state;
        }
        let resolver = &self.resolver;
        let loader = self
            .loader
            .get_or_insert_with(|| Loader::new(resolver.clone()));
        self.tickets
            .insert(loader.request_at(r.clone(), want, priority), key.clone());
        self.states.insert(key, FileState::Loading);
        FileState::Loading
    }

    pub(crate) fn failure(&self, key: &FileRef) -> Option<&Failure> {
        self.failures.get(key)
    }

    pub(crate) fn idle(&self) -> bool {
        self.tickets.is_empty() && self.upgrades.is_empty() && self.stepping.is_empty()
    }

    pub(crate) fn set_priority(&self, key: &FileRef, priority: f32) {
        if let Some(loader) = &self.loader {
            loader.set_priority(key.0.clone(), key.1, priority);
        }
    }

    /// Files finished since the last call, with what they hold or why they
    /// failed. With `wait`, blocks until nothing is loading (screenshots).
    pub(crate) fn poll(&mut self, wait: bool) -> Vec<(FileRef, Result<Loaded, Failure>)> {
        let Some(loader) = &mut self.loader else {
            return Vec::new();
        };
        if self.tickets.is_empty() && self.upgrades.is_empty() {
            return Vec::new();
        }
        let finished = if wait {
            loader.wait_all()
        } else {
            loader.poll()
        };
        let mut out = Vec::new();
        for (ticket, outcome) in finished {
            if let Some(hash) = self.upgrades.remove(&ticket) {
                self.upgrade_done(hash, outcome);
                continue;
            }
            let Some(key) = self.tickets.remove(&ticket) else {
                continue;
            };
            let larger = outcome.as_ref().is_ok_and(|f| f.larger);
            let loaded = outcome.and_then(|f| {
                let hash = *f.hash.as_bytes();
                let loaded = match key.1 {
                    Want::Mesh => f.mesh.map(|m| Loaded::Mesh(MeshHash(hash), m)),
                    Want::Image | Want::Texture(_) => {
                        f.image.map(|i| Loaded::Image(ImageHash(hash), i))
                    }
                    Want::Bytes | Want::Audio | Want::Model => None,
                };
                loaded.ok_or_else(|| Failure::new(FailureKind::Invalid, "not a mesh or an image"))
            });
            if let (Want::Texture(0), Ok(Loaded::Image(hash, image)), true) =
                (key.1, &loaded, larger)
            {
                self.streamed.entry(*hash).or_insert_with(|| {
                    Streamed::new(key.0.clone(), image.width.max(image.height), Instant::now())
                });
            }
            let state = match &loaded {
                Ok(Loaded::Mesh(MeshHash(h), _) | Loaded::Image(ImageHash(h), _)) => {
                    FileState::Loaded(*h)
                }
                Err(failure) => {
                    self.failures.insert(key.clone(), failure.clone());
                    FileState::Failed
                }
            };
            self.states.insert(key.clone(), state);
            out.push((key, loaded));
        }
        out
    }
}

/// The side of a level, as `Want::Texture`'s exponent.
fn side_of(level: u8) -> u32 {
    1u32.checked_shl(u32::from(level)).unwrap_or(u32::MAX)
}

/// The level a longer side of `side` is at (pack 0's 32 is 5).
fn level_of(side: u32) -> u8 {
    (u32::BITS - side.max(1).leading_zeros() - 1) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grows `s` to `target` stepping every `STEP`, each fetch arriving
    /// at once as the packs of a 4096 texture (largest 32, 256, 512, 1024,
    /// 2048, 4096); the levels fetched, and the time it took.
    fn grow(s: &mut Streamed, target: u8, start: Instant) -> (Vec<u8>, Duration) {
        s.target = target;
        let mut now = start;
        let mut fetched = Vec::new();
        while s.stepping() {
            if let Some(level) = s.step(now, false) {
                fetched.push(level);
                s.loading = false;
                s.side = [32, 256, 256, 256, 512, 1024, 2048, 4096][usize::from(level) - 5];
            }
            now += Duration::from_millis(50);
        }
        (fetched, now - start)
    }

    /// Studio 0.748 grew a texture a level every 0.35 s, fetching each
    /// pack as its first level was wanted.
    #[test]
    fn grows_a_level_at_a_time() {
        let start = Instant::now();
        let mut s = Streamed::new(
            ContentRef::Asset {
                id: 1,
                version: None,
            },
            32,
            start,
        );
        assert_eq!(s.level, 5);
        let (fetched, took) = grow(&mut s, 10, start);
        // 64 brings pack 1 (to 256), so 128 and 256 need nothing more.
        assert_eq!(fetched, [6, 9, 10]);
        assert_eq!(s.level, 10);
        // Five levels, the first at once.
        assert!(took >= STEP * 4 && took < STEP * 5, "{took:?}");
    }

    #[test]
    fn a_screenshot_goes_straight_to_its_size() {
        let start = Instant::now();
        let mut s = Streamed::new(
            ContentRef::Asset {
                id: 1,
                version: None,
            },
            32,
            start,
        );
        s.target = 12;
        assert_eq!(s.step(start, true), Some(12));
        assert!(s.stepping(), "until it arrives");
    }

    fn files() -> Files {
        Files::new(Arc::new(Resolver::new(rook_content::Config::empty())))
    }

    /// Larger levels that failed for now (a network failure, a pack being
    /// made) are asked for again later; ones that can't come leave it
    /// whole.
    #[test]
    fn a_failed_step_tries_again_later() {
        let mut files = files();
        let hash = ImageHash([1; 32]);
        files.stream_for_test(hash, 32, false);
        let s = files.streamed.get_mut(&hash).unwrap();
        (s.level, s.target, s.loading) = (6, 8, true);
        files.stepping.insert(hash);
        let network = Failure::new(FailureKind::Network, "timed out");
        files.upgrade_done(hash, Err(network));
        let s = &files.streamed[&hash];
        assert_eq!((s.level, s.whole, s.loading), (5, false, false));
        assert!(s.next > Instant::now() + STEP, "not before the retry");
        assert!(files.stepping.contains(&hash));

        files.streamed.get_mut(&hash).unwrap().loading = true;
        let refused = Failure::new(FailureKind::Forbidden, "no");
        files.upgrade_done(hash, Err(refused));
        assert!(files.streamed[&hash].whole);
        assert!(!files.stepping.contains(&hash));
    }

    /// Only what is in view grows: a texture growing when it leaves view
    /// stops at the level it reached.
    #[test]
    fn leaving_view_stops_growing() {
        let mut files = files();
        let hash = ImageHash([1; 32]);
        files.stream_for_test(hash, 32, false);
        files.want(&FxHashMap::from_iter([(hash, (12, 1.0))]));
        assert!(files.stepping.contains(&hash));
        files.streamed.get_mut(&hash).unwrap().level = 7;
        files.want(&FxHashMap::default());
        assert_eq!(files.streamed[&hash].target, 7);
        // Nothing shrinks: wanted smaller, it keeps its level.
        files.want(&FxHashMap::from_iter([(hash, (6, 1.0))]));
        assert_eq!(files.streamed[&hash].target, 7);
    }

    #[test]
    fn nothing_steps_past_whole() {
        let start = Instant::now();
        let mut s = Streamed::new(
            ContentRef::Asset {
                id: 1,
                version: None,
            },
            32,
            start,
        );
        s.target = 8;
        s.whole = true;
        assert_eq!(s.step(start, false), None);
        assert!(!s.stepping());
    }
}
