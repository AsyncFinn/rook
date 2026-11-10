//! What the renderer keeps on the GPU from the scene's files: one cache
//! scheme for mesh files and images. Each is uploaded once, by content
//! hash, the first time something draws it, and kept for the renderer's
//! life. Uploads wait in a queue and go up at the start of a frame within
//! a budget of bytes and of time ([`UploadBudget`]), so a place that needs
//! hundreds of images at once spreads them over frames instead of stalling
//! one. What waits for an upload draws nothing until it's done.

use std::collections::VecDeque;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustc_hash::{FxHashMap, FxHashSet};

/// Bytes uploaded per frame at most (the first upload of a frame always
/// goes, however large).
pub(crate) const FRAME_UPLOAD_BYTES: usize = 24 << 20;
/// Time a frame spends uploading at most. Bytes alone don't bound it:
/// making a texture and staging its levels costs far more than copying
/// them, and 24 MB of images took 47-70 ms in one frame on a 2018-era
/// place. A time slice beside a byte budget is Unity's pair
/// (`asyncUploadTimeSlice`, 2 ms by default, and `asyncUploadBufferSize`).
pub(crate) const FRAME_UPLOAD_TIME: Duration = Duration::from_millis(2);

/// What a frame may still upload: bytes, and time until a deadline (none
/// for a screenshot, which uploads everything). The first upload always
/// goes, so the queue moves whatever its sizes.
#[derive(Debug)]
pub(crate) struct UploadBudget {
    bytes: usize,
    until: Option<Instant>,
    first: bool,
}

impl UploadBudget {
    pub(crate) fn frame() -> Self {
        Self {
            bytes: FRAME_UPLOAD_BYTES,
            until: Some(Instant::now() + FRAME_UPLOAD_TIME),
            first: true,
        }
    }

    pub(crate) fn unlimited() -> Self {
        Self {
            bytes: usize::MAX,
            until: None,
            first: true,
        }
    }

    /// Take `size` bytes if they fit, or if nothing went up yet.
    fn take(&mut self, size: usize) -> bool {
        let fits = size <= self.bytes && self.until.is_none_or(|t| Instant::now() < t);
        if !fits && !self.first {
            return false;
        }
        self.first = false;
        self.bytes = self.bytes.saturating_sub(size);
        true
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct UploadContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub image_layout: &'a wgpu::BindGroupLayout,
    pub sampler: &'a wgpu::Sampler,
    /// Whether a texture can be viewed in a second format (sRGB and
    /// linear); OpenGL can't.
    pub view_formats: bool,
    /// Whether the device samples BC blocks (most desktop GPUs).
    pub block_compression: bool,
}

pub(crate) trait Upload {
    type Gpu;
    type Context<'a>;
    /// About how many bytes uploading it moves.
    fn upload_size(&self) -> usize;
    fn upload(&self, cx: &mut Self::Context<'_>) -> Self::Gpu;
}

#[derive(Debug)]
pub(crate) struct GpuCache<K, T: Upload> {
    ready: FxHashMap<K, T::Gpu>,
    queue: VecDeque<(K, Arc<T>)>,
    queued: FxHashSet<K>,
}

impl<K: Copy + Eq + Hash, T: Upload> GpuCache<K, T> {
    pub(crate) fn new() -> Self {
        Self {
            ready: FxHashMap::default(),
            queue: VecDeque::new(),
            queued: FxHashSet::default(),
        }
    }

    pub(crate) fn get(&self, key: &K) -> Option<&T::Gpu> {
        self.ready.get(key)
    }

    pub(crate) fn uploaded(&self) -> impl Iterator<Item = &T::Gpu> {
        self.ready.values()
    }

    pub(crate) fn contains(&self, key: &K) -> bool {
        self.ready.contains_key(key)
    }

    /// Put an already uploaded object in the cache (built-in meshes),
    /// returning the one it replaces.
    pub(crate) fn insert(&mut self, key: K, gpu: T::Gpu) -> Option<T::Gpu> {
        self.ready.insert(key, gpu)
    }

    /// Queue `file` for upload under `key`, unless it's there or queued.
    pub(crate) fn request(&mut self, key: K, file: &Arc<T>) {
        if !self.ready.contains_key(&key) && self.queued.insert(key) {
            self.queue.push_back((key, file.clone()));
        }
    }

    /// Forget `key`'s queued upload, or take its GPU copy out.
    pub(crate) fn remove(&mut self, key: &K) -> Option<T::Gpu> {
        if self.queued.remove(key) {
            self.queue.retain(|(k, _)| k != key);
        }
        self.ready.remove(key)
    }

    pub(crate) fn has_queued(&self) -> bool {
        !self.queue.is_empty()
    }

    pub(crate) fn is_queued(&self, key: &K) -> bool {
        self.queued.contains(key)
    }

    /// Upload queued files while `budget` lasts. Returns the keys
    /// uploaded.
    pub(crate) fn upload(&mut self, cx: &mut T::Context<'_>, budget: &mut UploadBudget) -> Vec<K> {
        let mut uploaded = Vec::new();
        while let Some((_, file)) = self.queue.front() {
            if !budget.take(file.upload_size()) {
                break;
            }
            let (key, file) = self.queue.pop_front().expect("front");
            self.queued.remove(&key);
            let gpu = file.upload(cx);
            self.ready.insert(key, gpu);
            uploaded.push(key);
        }
        uploaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Blob(usize);

    impl Upload for Blob {
        type Gpu = usize;
        type Context<'a> = ();
        fn upload_size(&self) -> usize {
            self.0
        }
        fn upload(&self, _: &mut ()) -> usize {
            self.0
        }
    }

    /// Uploads stop at the budget, but the first always goes.
    #[test]
    fn uploads_spread_over_frames() {
        let mut cache: GpuCache<u32, Blob> = GpuCache::new();
        let big = Arc::new(Blob(FRAME_UPLOAD_BYTES));
        for key in 0..3 {
            cache.request(key, &big);
        }
        cache.request(0, &big);
        let frame = |cache: &mut GpuCache<u32, Blob>| {
            let mut budget = UploadBudget::frame();
            budget.bytes = FRAME_UPLOAD_BYTES / 2;
            !cache.upload(&mut (), &mut budget).is_empty()
        };
        assert!(frame(&mut cache));
        assert!(cache.contains(&0) && !cache.contains(&1));
        assert!(frame(&mut cache) && frame(&mut cache));
        assert!(!cache.has_queued() && cache.contains(&2));
        assert!(!frame(&mut cache));
    }

    /// A frame whose time is spent uploads only its first file, however
    /// small the rest.
    #[test]
    fn uploads_stop_when_the_frames_time_is_spent() {
        let mut cache: GpuCache<u32, Blob> = GpuCache::new();
        let small = Arc::new(Blob(1));
        for key in 0..3 {
            cache.request(key, &small);
        }
        let mut spent = UploadBudget::frame();
        spent.until = Some(Instant::now());
        assert_eq!(cache.upload(&mut (), &mut spent), [0]);
        assert_eq!(
            cache.upload(&mut (), &mut UploadBudget::unlimited()),
            [1, 2]
        );
    }
}
