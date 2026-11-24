//! The canvases' quads: one shared buffer, each canvas's run beside its
//! id, kept until that canvas changes and given back when it goes.

use super::*;

impl WorldGui {
    /// For each of `canvases`, the canvas drawn now it was given as (the
    /// same `Arc`), if any.
    pub(crate) fn kept(&self, canvases: &[Arc<WorldCanvas>]) -> Vec<Option<usize>> {
        let at: FxHashMap<*const WorldCanvas, usize> = (self.canvases.iter().enumerate())
            .map(|(i, c)| (Arc::as_ptr(&c.source), i))
            .collect();
        (canvases.iter())
            .map(|c| at.get(&Arc::as_ptr(c)).copied())
            .collect()
    }

    /// The groups the `kept` canvases hold.
    pub(crate) fn held<'a>(&'a self, kept: &'a [Option<usize>]) -> impl Iterator<Item = &'a Held> {
        kept.iter().flatten().map(|&i| &self.canvases[i].held)
    }

    /// The groups the canvases hold, but for those `replaced` and those at
    /// `len` or past it.
    pub(crate) fn held_but<'a>(
        &'a self,
        replaced: &'a rustc_hash::FxHashSet<usize>,
        len: usize,
    ) -> impl Iterator<Item = &'a Held> {
        (self.canvases.iter().enumerate().take(len))
            .filter(|(i, _)| !replaced.contains(i))
            .map(|(_, c)| &c.held)
    }

    /// The canvases as last given.
    pub(crate) fn sources(&self) -> Vec<Arc<WorldCanvas>> {
        self.placed.clone()
    }

    /// Drop every canvas, so each is given again.
    pub(crate) fn forget(&mut self) {
        self.canvases.clear();
        if let Some(quads) = &mut self.quads {
            quads.ranges = RangeAlloc::default();
        }
        self.ids = RangeAlloc::default();
    }

    /// Draw `canvases` from now on: those [`WorldGui::kept`] found as they
    /// are, the rest from `fresh`, in order, each one's quads as the shader
    /// reads them and the groups it holds.
    pub(crate) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvases: &[Arc<WorldCanvas>],
        kept: Vec<Option<usize>>,
        fresh: Vec<(Packed, Held)>,
    ) {
        let mut old: Vec<Option<Canvas>> = self.canvases.drain(..).map(Some).collect();
        let kept: Vec<Option<Canvas>> = (kept.into_iter())
            .map(|k| k.and_then(|i| old[i].take()))
            .collect();
        // What's dropped gives its room back first, for what's new.
        for canvas in old.into_iter().flatten() {
            self.free(&canvas);
        }
        // A canvas given before stays where it was; a new one has no
        // place until it's given one.
        let was: FxHashMap<*const WorldCanvas, usize> = (self.placed.iter().enumerate())
            .map(|(i, c)| (Arc::as_ptr(c), i))
            .collect();
        let mut places = std::mem::take(&mut self.places);
        self.places = (canvases.iter())
            .map(|c| places.get_mut(*was.get(&Arc::as_ptr(c))?)?.take())
            .collect();
        self.placed = canvases.to_vec();
        let mut fresh = fresh.into_iter();
        for (c, kept) in canvases.iter().zip(kept) {
            if let Some(canvas) = kept {
                self.canvases.push(canvas);
                continue;
            }
            let (quads, held) = fresh.next().expect("one per canvas not kept");
            let canvas = self.make(device, queue, c, quads, held);
            self.canvases.push(canvas);
        }
    }

    /// Draw the canvases at `changed`'s indices (in order) from `fresh`
    /// from now on, with the list `len` long: those past it are dropped,
    /// and the rest stay as they are, where they are. A canvas at a new
    /// index has no place until it's given one.
    pub(crate) fn replace(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        len: usize,
        changed: &[(usize, Arc<WorldCanvas>)],
        fresh: Vec<(Packed, Held)>,
    ) {
        for canvas in self.canvases.split_off(len.min(self.canvases.len())) {
            self.free(&canvas);
        }
        self.places.truncate(len);
        self.placed.truncate(len);
        for ((i, c), (quads, held)) in changed.iter().zip(fresh) {
            // What's replaced gives its room back first, for what's new.
            if let Some(old) = self.canvases.get(*i) {
                let (quads, id) = (old.quads.clone(), old.id);
                self.give_back(quads, id);
            }
            let canvas = self.make(device, queue, c, quads, held);
            if *i < self.canvases.len() {
                self.canvases[*i] = canvas;
                self.placed[*i] = c.clone();
            } else {
                assert_eq!(*i, self.canvases.len(), "new canvases come in order");
                self.canvases.push(canvas);
                self.placed.push(c.clone());
                self.places.push(None);
            }
        }
    }

    /// Canvas `c` with its quads, as the shader reads them, put in the
    /// shared buffer.
    pub(super) fn make(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        c: &Arc<WorldCanvas>,
        quads: Packed,
        held: Held,
    ) -> Canvas {
        let gpu: Vec<GpuQuad> = match c.untoned {
            true => Vec::new(),
            false => quads.iter().map(|(q, _)| *q).collect(),
        };
        let (at, id) = self.put(device, queue, &gpu).unwrap_or((0..0, 0));
        Canvas {
            source: c.clone(),
            held,
            quads: at,
            id,
            runs: runs(quads.iter().map(|(_, image)| *image)),
            extent: extent(&gpu),
            size: c.size,
            light_influence: c.light_influence.clamp(0.0, 1.0),
            brightness: c.brightness,
            on_top: c.always_on_top,
            max_distance: c.max_distance,
            overlay: c.untoned.then_some(quads),
        }
    }

    /// Put a canvas's quads in the shared buffer, each beside a new id
    /// for the canvas: where they are, and the id; `None` when there are
    /// none, or no room.
    pub(super) fn put(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quads: &[GpuQuad],
    ) -> Option<(Range<u32>, u32)> {
        let len = u32::try_from(quads.len()).ok().filter(|&n| n > 0)?;
        let shared = (self.quads).get_or_insert_with(|| Quads::new(device, FIRST_QUADS));
        let at = shared.alloc(device, queue, len)?;
        let id = self
            .ids
            .alloc(1, u32::MAX)
            .expect("fewer canvases than ids");
        let ids = vec![id; quads.len()];
        queue.write_buffer(
            &shared.quads,
            u64::from(at) * QUAD,
            bytemuck::cast_slice(quads),
        );
        queue.write_buffer(&shared.ids, u64::from(at) * ID, bytemuck::cast_slice(&ids));
        Some((at..at + len, id))
    }

    /// Give a dropped canvas's quads and id back.
    pub(super) fn free(&mut self, canvas: &Canvas) {
        self.give_back(canvas.quads.clone(), canvas.id);
    }

    /// Give a canvas's `quads` and `id` back.
    pub(super) fn give_back(&mut self, quads: Range<u32>, id: u32) {
        if quads.is_empty() {
            return;
        }
        if let Some(shared) = &mut self.quads {
            shared.ranges.free(quads);
        }
        self.ids.free(id..id + 1);
    }

    /// Drop the frames' bind groups of `hash`'s image, which was replaced.
    pub(crate) fn forget_file(&mut self, hash: ImageHash) {
        if let Some(frames) = &mut self.frames {
            (frames.groups).retain(|key, _| !is_file(*key, hash));
        }
    }

    /// Drop the bind groups of groups' textures, which were drawn anew.
    pub(crate) fn forget_groups(&mut self) {
        if let Some(frames) = &mut self.frames {
            (frames.groups).retain(|key, _| !matches!(key, Some(ImageKey::Group(_))));
        }
    }
}

impl Quads {
    pub(super) fn new(device: &wgpu::Device, capacity: u32) -> Self {
        let buffer = |label, size| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::VERTEX
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        Self {
            quads: buffer("rook world gui quads", u64::from(capacity) * QUAD),
            ids: buffer("rook world gui quad canvases", u64::from(capacity) * ID),
            capacity,
            ranges: RangeAlloc::default(),
        }
    }

    /// Room for `len` quads, the buffers grown (keeping what they hold)
    /// if they must be; none past the device's biggest buffer.
    pub(super) fn alloc(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        len: u32,
    ) -> Option<u32> {
        if let Some(at) = self.ranges.alloc(len, self.capacity) {
            return Some(at);
        }
        let max = (device.limits().max_buffer_size / QUAD).min(u64::from(u32::MAX)) as u32;
        let need = (self.ranges.top().checked_add(len)).filter(|&n| n <= max)?;
        let grown = Self::new(device, self.capacity.saturating_mul(2).clamp(need, max));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("rook world gui quads growth"),
        });
        let used = u64::from(self.ranges.top());
        encoder.copy_buffer_to_buffer(&self.quads, 0, &grown.quads, 0, used * QUAD);
        encoder.copy_buffer_to_buffer(&self.ids, 0, &grown.ids, 0, used * ID);
        // Writes queued to the old buffers land before this copy.
        queue.submit([encoder.finish()]);
        (self.quads, self.ids, self.capacity) = (grown.quads, grown.ids, grown.capacity);
        self.ranges.alloc(len, self.capacity)
    }
}
