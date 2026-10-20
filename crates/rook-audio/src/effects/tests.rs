use super::*;

const EQ: Effect = Effect::Equalizer {
    low: 0.0,
    mid: 0.0,
    high: 0.0,
    range: CROSSOVERS,
};
const DIST: Effect = Effect::Distortion { level: 0.5 };
const PITCH: Effect = Effect::PitchShift { octave: 2.0 };

/// The orders measured in Studio: at Priority 0 Equalizer before
/// Distortion before PitchShift; an explicit priority above or below
/// the base moves an effect; ties go to the later child.
#[test]
fn orders_by_priority() {
    assert_eq!(ordered([(PITCH, 0), (DIST, 0), (EQ, 0)]), [EQ, DIST, PITCH]);
    assert_eq!(ordered([(DIST, 9), (EQ, 0)]), [DIST, EQ]);
    assert_eq!(ordered([(DIST, 8), (EQ, 0)]), [EQ, DIST]);
    assert_eq!(ordered([(DIST, 0), (EQ, 3)]), [DIST, EQ]);
    assert_eq!(ordered([(DIST, 0), (EQ, 4)]), [EQ, DIST]);
    assert_eq!(ordered([(PITCH, 0), (DIST, 2)]), [PITCH, DIST]);
    assert_eq!(ordered([(DIST, -5), (EQ, -10)]), [DIST, EQ]);
}

#[test]
fn keeps_state_across_settings() {
    let mut chain = Chain::new(crate::SAMPLE_RATE);
    chain.sync(&[EQ]);
    let mut frames = vec![[0.5, 0.5]; 64];
    chain.process(&mut frames);
    chain.sync(&[Effect::Equalizer {
        low: -6.0,
        mid: 0.0,
        high: 0.0,
        range: CROSSOVERS,
    }]);
    assert!(matches!(chain.stages[0], Stage::Equalizer(_)));
    chain.sync(&[DIST]);
    assert!(matches!(chain.stages[0], Stage::Distortion(_)));
}
