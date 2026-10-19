//! What the renderer keeps on the GPU from the scene's files: one cache
//! scheme for mesh files and images. Each is uploaded once, by content
//! hash, the first time something draws it, and kept for the renderer's
//! life. Uploads wait in a queue and go up at the start of a frame within
//! a byte budget ([`FRAME_UPLOAD_BYTES`]), so a place that needs hundreds
//! of images at once spreads them over frames instead of stalling one.
//! What waits for an upload draws nothing until it's done.

use std::collections::VecDeque;
use std::hash::Hash;
use std::sync::Arc;

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};

/// Bytes uploaded per frame at most (the first upload of a frame always
/// goes, however large).
pub(crate) const FRAME_UPLOAD_BYTES: usize = 24 << 20;

/// What uploads need from the renderer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UploadContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    /// The layout of an image's bind group.
    pub image_layout: &'a wgpu::BindGroupLayout,
    pub sampler: &'a wgpu::Sampler,
    /// Whether a texture can be viewed in a second format (sRGB and
    /// linear); OpenGL can't.
    pub view_formats: bool,
    /// Whether the device samples BC blocks (most desktop GPUs; not
    /// phones or browsers).
    pub block_compression: bool,
}

/// A file that can live on the GPU.
pub(crate) trait Upload {
    type Gpu;
    /// What uploading it needs.
    type Context<'a>;
    /// About how many bytes uploading it moves.
    fn upload_size(&self) -> usize;
    fn upload(&self, cx: &mut Self::Context<'_>) -> Self::Gpu;
}

/// GPU copies of files, by key.
#[derive(Debug)]
pub(crate) struct GpuCache<K, T: Upload> {
    ready: HashMap<K, T::Gpu>,
    queue: VecDeque<(K, Arc<T>)>,
    queued: HashSet<K>,
}

impl<K: Copy + Eq + Hash, T: Upload> GpuCache<K, T> {
    pub fn new() -> Self {
        Self {
            ready: HashMap::default(),
            queue: VecDeque::new(),
            queued: HashSet::default(),
        }
    }

    pub fn get(&self, key: &K) -> Option<&T::Gpu> {
        self.ready.get(key)
    }

    /// Every uploaded object.
    pub fn uploaded(&self) -> impl Iterator<Item = &T::Gpu> {
        self.ready.values()
    }

    pub fn contains(&self, key: &K) -> bool {
        self.ready.contains_key(key)
    }

    /// Put an already uploaded object in the cache (built-in meshes),
    /// returning the one it replaces.
    pub fn insert(&mut self, key: K, gpu: T::Gpu) -> Option<T::Gpu> {
        self.ready.insert(key, gpu)
    }

    /// Queue `file` for upload under `key`, unless it's there or queued.
    pub fn request(&mut self, key: K, file: &Arc<T>) {
        if !self.ready.contains_key(&key) && self.queued.insert(key) {
            self.queue.push_back((key, file.clone()));
        }
    }

    /// Forget `key`'s queued upload, or take its GPU copy out.
    pub fn remove(&mut self, key: &K) -> Option<T::Gpu> {
        if self.queued.remove(key) {
            self.queue.retain(|(k, _)| k != key);
        }
        self.ready.remove(key)
    }

    pub fn has_queued(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn is_queued(&self, key: &K) -> bool {
        self.queued.contains(key)
    }

    /// Upload queued files while `budget` (bytes) lasts; with
    /// `first_free`, the first one goes whatever its size. Returns the keys
    /// uploaded.
    pub fn upload(
        &mut self,
        cx: &mut T::Context<'_>,
        budget: &mut usize,
        first_free: &mut bool,
    ) -> Vec<K> {
        let mut uploaded = Vec::new();
        while let Some((_, file)) = self.queue.front() {
            let size = file.upload_size();
            if size > *budget && !*first_free {
                break;
            }
            *first_free = false;
            *budget = budget.saturating_sub(size);
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
            let mut budget = FRAME_UPLOAD_BYTES / 2;
            let mut first = true;
            !cache.upload(&mut (), &mut budget, &mut first).is_empty()
        };
        assert!(frame(&mut cache));
        assert!(cache.contains(&0) && !cache.contains(&1));
        assert!(frame(&mut cache) && frame(&mut cache));
        assert!(!cache.has_queued() && cache.contains(&2));
        assert!(!frame(&mut cache));
    }
}
