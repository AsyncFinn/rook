//! Sony pads' HID input reports, DualShock 4 and DualSense, over USB and
//! Bluetooth, as their published layouts give them. Pure, so it's tested
//! on every platform; only Windows reads pads with it.

use rook_types::GamepadKey;

use super::PadState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Model {
    DualShock4,
    DualSense,
}

impl Model {
    pub(super) fn of(product: u16) -> Option<Self> {
        match product {
            0x05c4 | 0x09cc | 0x0ba0 => Some(Self::DualShock4),
            0x0ce6 | 0x0df2 => Some(Self::DualSense),
            _ => None,
        }
    }

    /// The feature report (id, length) whose read makes a pad on
    /// Bluetooth send full reports, not the short ones it starts with:
    /// its calibration report.
    pub(super) fn full_reports_feature(self) -> (u8, usize) {
        match self {
            Self::DualShock4 => (0x02, 37),
            Self::DualSense => (0x05, 41),
        }
    }
}

/// Where the fields sit after the report's header.
#[derive(Clone, Copy)]
enum Layout {
    /// Sticks, buttons, then triggers: the DualShock 4's reports, and
    /// either pad's short Bluetooth report.
    Ds4,
    /// Sticks, triggers, a counter, then buttons.
    DualSense,
}

/// A report's state, or None for a report that carries none.
pub(super) fn parse(model: Model, bluetooth: bool, report: &[u8]) -> Option<PadState> {
    let (layout, base) = match (report.first()?, model, bluetooth) {
        (0x01, Model::DualShock4, _) | (0x01, Model::DualSense, true) => (Layout::Ds4, 1),
        (0x01, Model::DualSense, false) => (Layout::DualSense, 1),
        (0x11, Model::DualShock4, true) => (Layout::Ds4, 3),
        (0x31, Model::DualSense, true) => (Layout::DualSense, 2),
        _ => return None,
    };
    let data = report.get(base..base + 10)?;
    let (buttons, triggers) = match layout {
        Layout::Ds4 => (&data[4..7], [data[7], data[8]]),
        Layout::DualSense => (&data[7..10], [data[4], data[5]]),
    };
    let mut state = PadState {
        buttons: 0,
        sticks: [stick(data[0], data[1]), stick(data[2], data[3])],
        triggers: triggers.map(|b| f32::from(b) / 255.0),
    };
    let hat = buttons[0] & 0x0f;
    for (key, on) in [
        (GamepadKey::DPadUp, matches!(hat, 7 | 0 | 1)),
        (GamepadKey::DPadRight, matches!(hat, 1..=3)),
        (GamepadKey::DPadDown, matches!(hat, 3..=5)),
        (GamepadKey::DPadLeft, matches!(hat, 5..=7)),
        (GamepadKey::ButtonX, buttons[0] & 0x10 != 0),
        (GamepadKey::ButtonA, buttons[0] & 0x20 != 0),
        (GamepadKey::ButtonB, buttons[0] & 0x40 != 0),
        (GamepadKey::ButtonY, buttons[0] & 0x80 != 0),
        (GamepadKey::ButtonL1, buttons[1] & 0x01 != 0),
        (GamepadKey::ButtonR1, buttons[1] & 0x02 != 0),
        (GamepadKey::ButtonSelect, buttons[1] & 0x10 != 0),
        (GamepadKey::ButtonStart, buttons[1] & 0x20 != 0),
        (GamepadKey::ButtonL3, buttons[1] & 0x40 != 0),
        (GamepadKey::ButtonR3, buttons[1] & 0x80 != 0),
    ] {
        state.set(key, on);
    }
    // The PS button fires nothing in Studio (measured); the touchpad's
    // click is left out with it (Inference: no KeyCode names it).
    Some(state)
}

/// A stick's bytes as Studio scales them: steps of 1/127.5 around 127.5,
/// so it's never exactly 0 (measured), with up positive where the byte
/// grows downward.
fn stick(x: u8, y: u8) -> (f32, f32) {
    let axis = |b: u8| (f32::from(b) - 127.5) / 127.5;
    (axis(x), -axis(y))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One report with the left stick full right and up, the right stick
    /// at rest, L2 half pulled, Cross, R1 and Options down, and the D-pad
    /// down-left, its fields at `layout`'s offsets after `base` bytes.
    fn report(id: u8, base: usize, layout: Layout) -> Vec<u8> {
        let mut r = vec![0u8; base + 10];
        r[0] = id;
        let d = &mut r[base..];
        d[..4].copy_from_slice(&[255, 0, 128, 128]);
        let (buttons, triggers) = match layout {
            Layout::Ds4 => (4, 7),
            Layout::DualSense => (7, 4),
        };
        d[buttons] = 0x20 | 5;
        d[buttons + 1] = 0x02 | 0x20;
        d[triggers] = 128;
        r
    }

    fn check(state: PadState) {
        assert_eq!(state.sticks[0], (1.0, 1.0));
        assert_eq!(state.sticks[1], (0.5 / 127.5, -0.5 / 127.5));
        assert_eq!(state.triggers, [128.0 / 255.0, 0.0]);
        let down: Vec<_> = GamepadKey::ITEMS
            .iter()
            .map(|&(_, k)| k)
            .filter(|&k| state.is_down(k))
            .collect();
        use GamepadKey::*;
        let mut expected = vec![ButtonStart, ButtonA, ButtonR1, DPadDown, DPadLeft];
        expected.sort_by_key(|k| k.index());
        assert_eq!(down, expected);
    }

    #[test]
    fn every_report_reads_the_same_state() {
        let cases = [
            (Model::DualShock4, false, report(0x01, 1, Layout::Ds4)),
            (Model::DualShock4, true, report(0x11, 3, Layout::Ds4)),
            (Model::DualShock4, true, report(0x01, 1, Layout::Ds4)),
            (Model::DualSense, false, report(0x01, 1, Layout::DualSense)),
            (Model::DualSense, true, report(0x31, 2, Layout::DualSense)),
            (Model::DualSense, true, report(0x01, 1, Layout::Ds4)),
        ];
        for (model, bluetooth, r) in cases {
            check(parse(model, bluetooth, &r).unwrap());
        }
    }

    #[test]
    fn other_reports_carry_no_state() {
        assert!(parse(Model::DualSense, false, &[0x31; 80]).is_none());
        assert!(parse(Model::DualShock4, false, &[0x01, 0, 0]).is_none());
        assert!(parse(Model::DualSense, true, &[]).is_none());
    }
}
