//! The per-frame budgets of the stages whose work comes in bursts
//! (docs/design/architecture.md#per-frame-budgets), checked once content
//! has loaded: a stage over its budget is flagged with how often and by
//! how much, as engines' frame-budget checks flag a stage in CI (Unreal's
//! `stat unit` budgets, Unity's Frame Timing checks).

use std::time::Duration;

use crate::stats::Times;

/// A stage and what one frame of it may cost.
pub(super) struct Budget {
    pub(super) stage: &'static str,
    pub(super) per_frame: Duration,
}

const fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// Each budget is its queue's slice plus the stage's steady work on the
/// heaviest place benched (a 2018-era place with 85k parts): arrivals 2 ms
/// beside the scene's own following of moved parts, uploads 2 ms plus the
/// one upload that always goes.
pub(super) const SCENE_UPDATE: Budget = Budget {
    stage: "scene update",
    per_frame: ms(6),
};
pub(super) const HAND_OVER: Budget = Budget {
    stage: "hand-over",
    per_frame: ms(6),
};
pub(super) const UPLOADS: Budget = Budget {
    stage: "CPU recording, uploads",
    per_frame: ms(6),
};

impl Budget {
    /// A line saying whether `times` kept to the budget.
    pub(super) fn check(&self, times: &Times) -> String {
        let over: Vec<Duration> = times
            .samples()
            .iter()
            .copied()
            .filter(|t| *t > self.per_frame)
            .collect();
        let budget = self.per_frame.as_millis();
        match over.iter().max() {
            None => format!("budget kept: {} within {budget} ms", self.stage),
            Some(longest) => format!(
                "FLAG budget: {} over {budget} ms on {} of {} frames after load (longest {:.2} ms)",
                self.stage,
                over.len(),
                times.len(),
                crate::stats::ms(*longest)
            ),
        }
    }
}
