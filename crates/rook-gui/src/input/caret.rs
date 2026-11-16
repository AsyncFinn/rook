//! The focused TextBox's caret blinking. Measured at 60 Hz: shown for
//! 0.3 s, hidden for 0.3 s, each turn ending on the first frame at or past
//! 0.3 s (18 or 19 frames, a 0.62 s blink); moving the caret or typing
//! doesn't restart it.

use crate::Gui;

/// How long the caret shows, and how long it hides.
const TURN: f64 = 0.3;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Blink {
    pub(crate) shown: bool,
    /// Seconds since the caret last showed or hid.
    since: f64,
}

/// Inference: a box shows its caret as it gains focus.
impl Default for Blink {
    fn default() -> Self {
        Self {
            shown: true,
            since: 0.0,
        }
    }
}

impl Gui {
    /// Blink the focused TextBox's caret on a frame `dt` seconds long,
    /// before GUI is laid out; a turn draws that box alone again.
    pub fn blink_caret(&mut self, dt: f64) {
        let Some(id) = self.focus else {
            return;
        };
        self.blink.since += dt;
        if self.blink.since >= TURN {
            self.blink = Blink {
                shown: !self.blink.shown,
                since: 0.0,
            };
            self.restyled.insert(id);
        }
    }
}
