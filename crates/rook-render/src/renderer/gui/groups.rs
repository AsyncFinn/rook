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
    key: u64,
    size: [u32; 2],
    view: wgpu::TextureView,
    target: Target,
}

#[derive(Debug, Default)]
pub(super) struct Groups {
    drawn: Vec<Drawn>,
}

impl Groups {
    /// Draw `held` from now on, `packed` being each one's quads as the
    /// shader reads them; textures of groups drawn before are kept. Their
    /// views, by group key.
    pub(super) fn set(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        held: &[Arc<GuiGroup>],
        packed: &[Packed],
        target: impl Fn(&wgpu::Device) -> Target,
    ) -> HashMap<u64, wgpu::TextureView> {
        let mut old = std::mem::take(&mut self.drawn);
        for (group, quads) in held.iter().zip(packed) {
            let size = group.size.map(|v| v.max(1));
            let kept = old
                .iter()
                .position(|d| d.key == group.key && d.size == size);
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
                        key: group.key,
                        size,
                        view: texture.create_view(&Default::default()),
                        target: target(device),
                    }
                }
            };
            drawn.target.set(device, queue, quads);
            self.drawn.push(drawn);
        }
        (self.drawn.iter())
            .map(|d| (d.key, d.view.clone()))
            .collect()
    }

    /// Each group's quads, its texture and its size, inner groups first.
    pub(super) fn drawn(&self) -> impl Iterator<Item = (&Target, &wgpu::TextureView, (u32, u32))> {
        (self.drawn.iter()).map(|d| (&d.target, &d.view, (d.size[0], d.size[1])))
    }
}
