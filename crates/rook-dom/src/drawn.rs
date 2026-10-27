//! Where things are drawn this frame, which can differ from where scripts
//! see them: a moving part between its last two physics steps, and the
//! camera following it. The engine sets these once a frame; everything
//! that draws a part or looks through the camera reads
//! [`Dom::drawn_cframe`], and scripts read `CFrame`.

use crate::{Dom, InstanceId};
use rook_types::CFrame;

impl Dom {
    /// Where `id` (a part or a camera) is drawn: where the engine put it
    /// for this frame, else its `CFrame`.
    pub fn drawn_cframe(&self, id: InstanceId) -> Option<CFrame> {
        match self.drawn.get(&id) {
            Some(&cf) => Some(cf),
            None => self.cframe_property(id, "CFrame"),
        }
    }

    /// What is drawn away from its `CFrame` this frame.
    pub fn drawn_ids(&self) -> impl Iterator<Item = InstanceId> + '_ {
        self.drawn.keys().copied()
    }

    /// This frame's drawn poses, where they differ from `CFrame`. Returns
    /// what was drawn away from its `CFrame` last frame.
    pub fn set_drawn(
        &mut self,
        drawn: impl IntoIterator<Item = (InstanceId, CFrame)>,
    ) -> Vec<InstanceId> {
        let before = self.drawn.keys().copied().collect();
        self.drawn.clear();
        self.drawn.extend(drawn);
        before
    }
}
