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

    /// Whether `id` is drawn away from its `CFrame` this frame.
    pub fn is_drawn(&self, id: InstanceId) -> bool {
        self.drawn.contains_key(&id)
    }

    /// What is drawn away from its `CFrame` this frame.
    pub fn drawn_ids(&self) -> impl Iterator<Item = InstanceId> + '_ {
        self.drawn.keys().copied()
    }

    /// This frame's drawn poses, where they differ from `CFrame`. Returns
    /// what is drawn somewhere new: those drawn at another pose than last
    /// frame, then those drawn away from their `CFrame` last frame and back
    /// at it now. A resting part an awake assembly keeps redrawing at the
    /// same pose isn't among them, so nothing redraws it.
    pub fn set_drawn(
        &mut self,
        drawn: impl IntoIterator<Item = (InstanceId, CFrame)>,
    ) -> Vec<InstanceId> {
        let before = std::mem::take(&mut self.drawn);
        let mut moved = Vec::new();
        for (id, cf) in drawn {
            if before.get(&id) != Some(&cf) {
                moved.push(id);
            }
            self.drawn.insert(id, cf);
        }
        let mut back: Vec<InstanceId> = (before.into_keys())
            .filter(|id| !self.drawn.contains_key(id))
            .collect();
        back.sort_unstable();
        moved.extend(back);
        moved
    }
}
