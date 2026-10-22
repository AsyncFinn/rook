//! Groups' textures (CanvasGroups): each group's quads drawn into a
//! texture of its own every frame, inner groups first, which the quads
//! showing the group then draw through like an image.

use std::sync::Arc;

use rook_types::{GuiGroup, GuiQuad, Paint};

use super::super::*;
use super::{GUI_FORMAT, Packed, Target};

/// The groups `lists` hold, nested ones before the groups holding them,
/// each once.
pub(super) fn held(lists: &[&[GuiQuad]]) -> Vec<Arc<GuiGroup>> {
    fn walk(quads: &[GuiQuad], out: &mut Vec<Arc<GuiGroup>>) {
        for q in quads {
            if let Paint::Group(group) = &q.paint
                && !out.iter().any(|g| g.key == group.key)
            {
                walk(&group.quads, out);
                out.push(group.clone());
            }
        }
    }
    let mut out = Vec::new();
    for quads in lists {
        walk(quads, &mut out);
    }
    out
}

#[derive(Debug)]
struct Drawn {
    /// The group as last given, whose quads `target` holds.
    source: Arc<GuiGroup>,
    size: [u32; 2],
    view: wgpu::TextureView,
    target: Target,
}

#[derive(Debug, Default)]
pub(super) struct Groups {
    drawn: Vec<Drawn>,
}

impl Groups {
    /// Whether `group` is drawn as given (the same `Arc`).
    pub(super) fn has(&self, group: &Arc<GuiGroup>) -> bool {
        self.drawn.iter().any(|d| Arc::ptr_eq(&d.source, group))
    }

    /// Drop every group's quads, so each is given again.
    pub(super) fn forget(&mut self) {
        self.drawn.clear();
    }

    /// Draw `shown` from now on, inner groups first; `fresh` holds the
    /// quads, as the shader reads them, of those not drawn as given. A
    /// group's texture is kept while its key and size are. The textures'
    /// views by group key, when which are drawn changed.
    pub(super) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        shown: &[Arc<GuiGroup>],
        fresh: &[(Arc<GuiGroup>, Packed)],
        target: impl Fn(&wgpu::Device) -> Target,
    ) -> Option<HashMap<u64, wgpu::TextureView>> {
        let mut old = std::mem::take(&mut self.drawn);
        let changed = !fresh.is_empty() || old.len() != shown.len();
        for group in shown {
            if let Some(i) = old.iter().position(|d| Arc::ptr_eq(&d.source, group)) {
                self.drawn.push(old.swap_remove(i));
                continue;
            }
            let Some((_, quads)) = fresh.iter().find(|(g, _)| Arc::ptr_eq(g, group)) else {
                continue;
            };
            let size = group.size.map(|v| v.max(1));
            let kept = (old.iter()).position(|d| d.source.key == group.key && d.size == size);
            let mut drawn = match kept {
                Some(i) => old.swap_remove(i),
                None => {
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("rook gui group"),
                        size: wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: GUI_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    Drawn {
                        source: group.clone(),
                        size,
                        view: texture.create_view(&Default::default()),
                        target: target(device),
                    }
                }
            };
            drawn.source = group.clone();
            drawn.target.set(device, queue, quads);
            self.drawn.push(drawn);
        }
        changed.then(|| {
            (self.drawn.iter())
                .map(|d| (d.source.key, d.view.clone()))
                .collect()
        })
    }

    /// Each group's quads, its texture and its size, inner groups first.
    pub(super) fn drawn(&self) -> impl Iterator<Item = (&Target, &wgpu::TextureView, (u32, u32))> {
        (self.drawn.iter()).map(|d| (&d.target, &d.view, (d.size[0], d.size[1])))
    }
}
