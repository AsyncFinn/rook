//! What the renderer keeps on the GPU from the scene's files: one cache
//! scheme for mesh files and images. Each is uploaded once, by content
//! hash, the first time something draws it, and kept for the renderer's
//! life. Uploads wait in a queue and go up at the start of a frame within
//! a byte budget ([`FRAME_UPLOAD_BYTES`]), so a place that needs hundreds
//! of images at once spreads them over frames instead of stalling one.
//! What waits for an upload draws nothing until it's done.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

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
}

/// A file that can live on the GPU.
pub(crate) trait Upload {
    type Gpu;
    /// About how many bytes uploading it moves.
    fn upload_size(&self) -> usize;
    fn upload(&self, cx: &UploadContext<'_>) -> Self::Gpu;
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
            ready: HashMap::new(),
            queue: VecDeque::new(),
            queued: HashSet::new(),
        }
    }

    pub fn get(&self, key: &K) -> Option<&T::Gpu> {
        self.ready.get(key)
    }

    pub fn contains(&self, key: &K) -> bool {
        self.ready.contains_key(key)
    }

    /// Put an already uploaded object in the cache (built-in meshes).
    pub fn insert(&mut self, key: K, gpu: T::Gpu) {
        self.ready.insert(key, gpu);
    }

    /// Queue `file` for upload under `key`, unless it's there or queued.
    pub fn request(&mut self, key: K, file: &Arc<T>) {
        if !self.ready.contains_key(&key) && self.queued.insert(key) {
            self.queue.push_back((key, file.clone()));
        }
    }

    /// Forget `key`'s GPU copy, or its queued upload.
    pub fn remove(&mut self, key: &K) {
        self.ready.remove(key);
        if self.queued.remove(key) {
            self.queue.retain(|(k, _)| k != key);
        }
    }

    pub fn has_queued(&self) -> bool {
        !self.queue.is_empty()
    }

    pub fn is_queued(&self, key: &K) -> bool {
        self.queued.contains(key)
    }

    /// Upload queued files while `budget` (bytes) lasts; with
    /// `first_free`, the first one goes whatever its size. Returns whether
    /// anything was uploaded.
    pub fn upload(
        &mut self,
        cx: &UploadContext<'_>,
        budget: &mut usize,
        first_free: &mut bool,
    ) -> bool {
        let mut any = false;
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
            any = true;
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Blob(usize);

    impl Upload for Blob {
        type Gpu = usize;
        fn upload_size(&self) -> usize {
            self.0
        }
        fn upload(&self, _: &UploadContext<'_>) -> usize {
            self.0
        }
    }

    /// Uploads stop at the budget, but the first always goes.
    #[test]
    fn uploads_spread_over_frames() {
        let Ok(r) = crate::Renderer::headless() else {
            return;
        };
        let cx = r.upload_context();
        let mut cache: GpuCache<u32, Blob> = GpuCache::new();
        let big = Arc::new(Blob(FRAME_UPLOAD_BYTES));
        for key in 0..3 {
            cache.request(key, &big);
        }
        cache.request(0, &big);
        let frame = |cache: &mut GpuCache<u32, Blob>| {
            let mut budget = FRAME_UPLOAD_BYTES / 2;
            let mut first = true;
            cache.upload(&cx, &mut budget, &mut first)
        };
        assert!(frame(&mut cache));
        assert!(cache.contains(&0) && !cache.contains(&1));
        assert!(frame(&mut cache) && frame(&mut cache));
        assert!(!cache.has_queued() && cache.contains(&2));
        assert!(!frame(&mut cache));
    }
}
