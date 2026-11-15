//! Recording the GUI's draws: groups into their textures, world canvases
//! into the world, and the screen's quads into the GUI layer.

use super::*;

impl GuiPass {
    /// Lay some of the world GUI's canvases in the world.
    pub(crate) fn record_world(&self, pass: &mut wgpu::RenderPass<'_>, runs: &[WorldDraw]) {
        self.world.record(pass, runs);
    }

    /// Draw the groups into their textures, inner ones first, before
    /// anything that shows them.
    pub(crate) fn record_groups(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder) {
        for (target, view, size) in self.groups.drawn() {
            self.draw(queue, encoder, target, view, size, true);
        }
    }

    /// Draw the GUI layer into `target`, if there's any GUI: the untoned
    /// world canvases, then the screen's, then the cursor. Whether there
    /// was.
    pub(crate) fn record(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
    ) -> bool {
        let under = self.draw(queue, encoder, &self.overlay, target, size, true);
        let over = self.draw(queue, encoder, &self.screen, target, size, !under);
        let cursor = self.draw(queue, encoder, &self.cursor, target, size, !under && !over);
        under || over || cursor
    }

    /// Draw `quads` into `view`, cleared first when `clear`. Whether there
    /// were any.
    pub(super) fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        quads: &Target,
        view: &wgpu::TextureView,
        (width, height): (u32, u32),
        clear: bool,
    ) -> bool {
        let Some((buffer, count)) = &quads.quads else {
            return false;
        };
        let size = [width as f32, height as f32, 0.0, 0.0];
        queue.write_buffer(&quads.size, 0, bytemuck::bytes_of(&size));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("rook gui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: match clear {
                        true => wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        false => wgpu::LoadOp::Load,
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &quads.group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        // Quads whose image isn't up yet draw nothing.
        for run in quads.runs.iter().filter(|r| r.quads.end <= *count) {
            if let Some(group) = self.images.group(run.image) {
                pass.set_bind_group(1, group, &[]);
                pass.draw(0..6, run.quads.clone());
            }
        }
        true
    }
}
