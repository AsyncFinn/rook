//! What the renderer tells about itself: GPU memory, draw counts, how long
//! frames took, the GPU and the light voxels as kept.

use super::*;

impl Renderer {
    /// The GPU memory the scene's images hold.
    pub fn image_bytes(&self) -> usize {
        self.images.uploaded().map(|i| i.bytes).sum()
    }

    /// The light voxels' and sun voxels' bytes, as kept.
    #[cfg(test)]
    pub(crate) fn light_voxels(&self) -> [Vec<u8>; 2] {
        self.lights.read_voxels(&self.device, &self.queue)
    }

    /// Where each scene entry is drawn.
    #[cfg(test)]
    pub(crate) fn placements(&self) -> Vec<(Space, Placement)> {
        self.placements.clone()
    }

    /// How many draw calls the last frame took.
    pub fn draw_counts(&self) -> DrawCounts {
        self.draw_counts
    }

    /// How long the CPU took to record and submit the last frame, file
    /// uploads included (not the GPU's time: [`Renderer::gpu_times`]).
    pub fn record_time(&self) -> std::time::Duration {
        self.record_time
    }

    /// Where [`Renderer::record_time`] went.
    pub fn stage_times(&self) -> StageTimes {
        self.stage_times
    }

    /// The GPU, its backend and driver.
    pub fn gpu(&self) -> &str {
        &self.gpu
    }

    pub fn color_format(&self) -> wgpu::TextureFormat {
        self.color_format
    }
}
