//! The images each draw samples (group 2): its image (a mesh texture, a
//! decal or a skybox face) and its material's color, normal, roughness
//! and metalness maps.
//!
//! Where the adapter has binding arrays, every image sits in one table
//! that all draws share, and each instance names its images' slots, so
//! draws with different images go in one multi-draw. Elsewhere each image
//! set gets its own bind group, made once all its images are on the GPU,
//! and bound before the draws that use it.

use super::*;

/// A draw's images, in binding order: image, color, normal, roughness,
/// metalness.
pub(crate) type ImageSet = [Option<ImageHash>; 5];

/// The table's first slots: what stands in for missing images.
const WHITE: u32 = 0;
const FLAT_NORMAL: u32 = 1;
/// Each image of a set without its own: a flat normal for the normal map,
/// white for the rest.
const DEFAULT_SLOTS: [u32; 5] = [WHITE, WHITE, FLAT_NORMAL, WHITE, WHITE];
/// The table's first size, in slots (doubled as places need more).
const FIRST_SLOTS: u32 = 1024;

/// What stands in for a missing image: white for the image and color
/// (sRGB) and for roughness and metalness (linear, so the part's own
/// values count in full), and a flat normal.
#[derive(Debug)]
pub(crate) struct Defaults {
    pub(crate) white: GpuImage,
    pub(crate) flat_normal: GpuImage,
}

impl Defaults {
    pub(crate) fn new(cx: &UploadContext<'_>) -> Self {
        Self {
            white: GpuImage::solid(cx, "rook white", [255; 4]),
            flat_normal: GpuImage::solid(cx, "rook flat normal", [128, 128, 255, 255]),
        }
    }
}

/// How draws get their images.
#[derive(Debug)]
pub(crate) enum ImageBinding {
    Table(ImageTable),
    /// A bind group per image set.
    Sets(HashMap<ImageSet, wgpu::BindGroup>),
}

/// Every image on the GPU in two binding arrays (sRGB and linear views)
/// with the same slots.
#[derive(Debug)]
pub(crate) struct ImageTable {
    slots: HashMap<ImageHash, u32>,
    /// The images in slot order, after the two defaults.
    order: Vec<ImageHash>,
    /// The slots the layout has room for.
    pub(crate) capacity: u32,
    /// Whether the bind group may hold fewer images than the layout.
    partial: bool,
    /// The bind group of every slot, made again when images arrive.
    group: Option<wgpu::BindGroup>,
}

impl ImageBinding {
    /// A table if the device has binding arrays with room for one, else
    /// bind groups per set.
    pub(crate) fn for_device(device: &wgpu::Device) -> Self {
        let needs = wgpu::Features::TEXTURE_BINDING_ARRAY
            | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING;
        if !device.features().contains(needs) || table_room(device) < FIRST_SLOTS {
            return Self::Sets(HashMap::default());
        }
        Self::Table(ImageTable {
            slots: HashMap::default(),
            order: Vec::new(),
            capacity: FIRST_SLOTS,
            partial: device
                .features()
                .contains(wgpu::Features::PARTIALLY_BOUND_BINDING_ARRAY),
            group: None,
        })
    }

    /// The table's size in slots, if there's a table.
    pub(crate) fn table_slots(&self) -> Option<u32> {
        match self {
            Self::Table(t) => Some(t.capacity),
            Self::Sets(_) => None,
        }
    }
}

/// The most slots a table may have on this device: both arrays, and the
/// main pass's three other textures, count against its limits.
fn table_room(device: &wgpu::Device) -> u32 {
    let l = device.limits();
    (l.max_binding_array_elements_per_shader_stage / 2)
        .min(l.max_sampled_textures_per_shader_stage.saturating_sub(3) / 2)
}

/// The layout of group 2: the table's two arrays, or one set's images.
pub(crate) fn image_layout(device: &wgpu::Device, table: Option<u32>) -> wgpu::BindGroupLayout {
    let entry = |binding, count| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count,
    };
    let entries: Vec<_> = match table {
        Some(slots) => (0..2)
            .map(|i| entry(i, std::num::NonZeroU32::new(slots)))
            .collect(),
        None => (0..5).map(|i| entry(i, None)).collect(),
    };
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("rook images"),
        entries: &entries,
    })
}

/// The image set an entry draws with.
pub(crate) fn image_set(part: &PartInstance) -> ImageSet {
    let image = match part.layer {
        Layer::Base => part.texture,
        Layer::Image { image, .. } => Some(image),
    };
    let m = part.maps;
    [image, m.color, m.normal, m.roughness, m.metalness]
}

impl Renderer {
    /// An entry's instance, naming its images' slots in the table.
    pub(crate) fn instance(&self, part: &PartInstance) -> Instance {
        let s = self.image_slots(&image_set(part));
        let images = [s[0] | s[1] << 16, s[2] | s[3] << 16, s[4]];
        Instance::new(part, self.bounds(part.shape), images)
    }

    /// The table slot of each image of `set`: the default's for one the
    /// table doesn't hold (none without a table).
    pub(crate) fn image_slots(&self, set: &ImageSet) -> [u32; 5] {
        let mut slots = DEFAULT_SLOTS;
        if let ImageBinding::Table(table) = &self.image_binding {
            for (slot, hash) in slots.iter_mut().zip(set) {
                // Past the most slots the device allows: the default.
                let found = hash.and_then(|h| table.slots.get(&h));
                if let Some(&s) = found.filter(|&&s| s < table.capacity) {
                    *slot = s;
                }
            }
        }
        slots
    }

    /// Give newly uploaded images their slots, making the table bigger
    /// (and the pipelines again) when it's full.
    pub(crate) fn add_images(&mut self, hashes: &[ImageHash]) {
        let ImageBinding::Table(table) = &mut self.image_binding else {
            return;
        };
        if hashes.is_empty() {
            return;
        }
        for &hash in hashes {
            if !table.slots.contains_key(&hash) {
                table.slots.insert(hash, table.order.len() as u32 + 2);
                table.order.push(hash);
            }
        }
        table.group = None;
        let needed = table.order.len() as u32 + 2;
        if needed > table.capacity {
            let room = table_room(&self.device);
            table.capacity = (table.capacity * 2)
                .max(needed.next_power_of_two())
                .min(room);
            let capacity = table.capacity;
            self.image_layout = image_layout(&self.device, Some(capacity));
            let layouts = BindLayouts {
                globals: &self.globals_layout,
                main: &self.main_layout,
                image: &self.image_layout,
                lights: &self.lights.layout,
            };
            self.pipelines = Pipelines::new(&self.device, self.color_format, layouts, true);
            self.gui.rebuild_world(&self.device, layouts);
        }
    }

    /// Make the bind group of `set`, whose images must all be on the GPU
    /// (with a table, nothing to make).
    pub(crate) fn ensure_group(&mut self, set: ImageSet) {
        let ImageBinding::Sets(groups) = &self.image_binding else {
            return;
        };
        if groups.contains_key(&set) {
            return;
        }
        let d = &self.defaults;
        let fallback = [&d.white, &d.white, &d.flat_normal, &d.white, &d.white];
        let mut images = fallback;
        for (slot, hash) in set.iter().enumerate() {
            if let Some(gpu) = hash.and_then(|h| self.images.get(&h)) {
                images[slot] = gpu;
            }
        }
        let group = images_bind_group(&self.upload_context(), images);
        if let ImageBinding::Sets(groups) = &mut self.image_binding {
            groups.insert(set, group);
        }
    }

    /// The bind group of a draw's images: the table's, or the set's (the
    /// defaults' when it hasn't been made).
    pub(crate) fn group(&self, set: &ImageSet) -> &wgpu::BindGroup {
        match &self.image_binding {
            ImageBinding::Table(table) => table.group.as_ref().expect("made before drawing"),
            ImageBinding::Sets(groups) => groups.get(set).unwrap_or_else(|| &groups[&[None; 5]]),
        }
    }

    /// Make the table's bind group if images arrived since it was made.
    pub(crate) fn update_table(&mut self) {
        let ImageBinding::Table(table) = &self.image_binding else {
            return;
        };
        if table.group.is_some() {
            return;
        }
        let d = &self.defaults;
        let mut images: Vec<&GpuImage> = vec![&d.white, &d.flat_normal];
        images.extend(
            table
                .order
                .iter()
                .map(|h| self.images.get(h).unwrap_or(&d.white)),
        );
        images.truncate(table.capacity as usize);
        if !table.partial {
            images.resize(table.capacity as usize, &d.white);
        }
        let srgb: Vec<&wgpu::TextureView> = images.iter().map(|i| &i.srgb).collect();
        let linear: Vec<&wgpu::TextureView> = images.iter().map(|i| &i.linear).collect();
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rook image table"),
            layout: &self.image_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureViewArray(&srgb),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureViewArray(&linear),
                },
            ],
        });
        if let ImageBinding::Table(table) = &mut self.image_binding {
            table.group = Some(group);
        }
    }
}
