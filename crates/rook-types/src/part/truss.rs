//! TrussPart: how Studio sizes a truss and what it draws, the one
//! definition scripts, the scene builder and the renderer follow. Observed
//! in Studio 0.742 on 2026-10-13 and measured from Studio's OBJ export of a
//! 2 × 2 × 2 and a 2 × 4 × 2 truss (docs/behavior/meshes/trusses.md).
//!
//! A truss is drawn in its own frame, built along its Y axis: a square
//! frame of four corner rails, 2 studs across, cut into 2-stud segments.
//! [`TrussLook`] turns that frame onto the part's long axis and says where
//! along it the drawing starts; [`TrussShape`] says how many segments there
//! are and which are braced. The bars themselves are built by the renderer.

use glam::{Mat3, Vec3};

use crate::NormalId;

/// The largest length a truss keeps (observed: setting 3000 or 600 on any
/// axis read back 512).
pub(crate) const TRUSS_MAX_LENGTH: f32 = 512.0;

/// The smallest any axis of a part can be (observed: a truss given -4
/// read back 0.001).
const MIN_SIZE: f32 = 0.001;

/// What a truss's Size becomes when a script sets it (observed in Studio
/// 0.742, 2026-10-13, from what `Size` read back; conformance case
/// `content/truss`). Each axis is first kept between 0.001 and 512. A truss
/// is 2 studs across, with at most one axis of another length: when two
/// axes are already exactly 2 the third keeps its length, shorter than 2
/// or not ((2, 1, 2) and (2, 0.5, 2) stay as they are). Otherwise the
/// longest axis is the truss's length, X before Y before Z on a tie, at
/// least 2, and the other two become 2: (3, 3, 3) becomes (3, 2, 2),
/// (6, 2, 10) becomes (2, 2, 10), (1, 1, 1) and (1, 0.5, 2) become
/// (2, 2, 2). The Size set before doesn't matter.
pub fn truss_size(size: Vec3) -> Vec3 {
    let s = size.clamp(Vec3::splat(MIN_SIZE), Vec3::splat(TRUSS_MAX_LENGTH));
    let twos = s.to_array().iter().filter(|&&v| v == 2.0).count();
    if twos >= 2 {
        return s;
    }
    let long = long_axis_of(s);
    let mut out = Vec3::splat(2.0);
    out[long] = s[long].max(2.0);
    out
}

/// The longest axis, X before Y before Z on a tie.
fn long_axis_of(s: Vec3) -> usize {
    let mut long = 0;
    for axis in 1..3 {
        if s[axis] > s[long] {
            long = axis;
        }
    }
    long
}

/// `Enum.Style`: which segments of a truss are braced. A truss of one
/// segment is never braced, whatever its Style (observed for all three on
/// 2 × 2 × 2 and 2 × 3 × 2 trusses; the export of a 2 × 2 × 2 truss has no
/// braces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrussStyle {
    /// Every segment braced on all four sides, the braces leaning one way
    /// and then the other from segment to segment (the default).
    AlternatingSupports,
    /// Only the truss's two X sides braced, the lower half of the segments
    /// leaning one way and the upper half the other, like a bridge.
    BridgeStyleSupports,
    /// No braces at all.
    NoSupports,
}

impl TrussStyle {
    /// From `Enum.Style`'s value: AlternatingSupports 0, BridgeStyleSupports
    /// 1, NoSupports 2.
    pub fn from_u32(value: u32) -> Self {
        match value {
            1 => Self::BridgeStyleSupports,
            2 => Self::NoSupports,
            _ => Self::AlternatingSupports,
        }
    }
}

/// What a truss draws in its own frame: `segments` 2-stud segments along
/// its Y axis, centered on the origin, braced by `style`. Two trusses that
/// draw the same have equal shapes (a one-segment truss is
/// [`TrussStyle::NoSupports`] whatever its Style), so they share one mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrussShape {
    pub style: TrussStyle,
    pub segments: u16,
}

/// The direction a brace leans: along its face's [`across`](TrussShape::across)
/// axis, it rises from -0.875 at the segment's bottom to +0.875 at its top
/// (`Up`), or the other way (`Down`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lean {
    Up,
    Down,
}

impl Lean {
    pub fn sign(self) -> f32 {
        match self {
            Self::Up => 1.0,
            Self::Down => -1.0,
        }
    }

    fn flip(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
        }
    }
}

/// Measured from the export (Confirmed): bars are 0.25 studs square. The
/// four rails run the truss's whole length at its corners. Each segment has
/// a rung across each of the four sides at both ends, between the rails;
/// where two segments meet their rungs are one bar 0.5 studs thick. A
/// brace is 0.25 studs wide and 0.125 deep, centered in the plane of the
/// rails' centers (0.875 from the axis), running between the rails' center
/// lines from one corner of the segment 0.875 below its middle to the
/// opposite corner 0.875 above it, its ends cut square and buried in the
/// rails.
impl TrussShape {
    /// The truss's width, and the length of a segment.
    pub const SEGMENT: f32 = 2.0;
    /// The thickness of the rails and rungs.
    pub const BAR: f32 = 0.25;
    /// A brace's width and depth.
    pub const BRACE_WIDTH: f32 = 0.25;
    pub const BRACE_DEPTH: f32 = 0.125;

    /// The four sides of the truss, which carry its rungs and braces.
    pub const SIDES: [NormalId; 4] = [
        NormalId::Front,
        NormalId::Right,
        NormalId::Back,
        NormalId::Left,
    ];

    pub fn new(style: TrussStyle, segments: u16) -> Self {
        let segments = segments.max(1);
        let style = if segments == 1 {
            TrussStyle::NoSupports
        } else {
            style
        };
        Self { style, segments }
    }

    /// The drawn length, in studs.
    pub fn length(self) -> f32 {
        f32::from(self.segments) * Self::SEGMENT
    }

    /// The axis a brace on `side` leans along: X on the Front and Back
    /// sides, Z on the Right and Left.
    pub fn across(side: NormalId) -> Vec3 {
        if side.normal().z != 0.0 {
            Vec3::X
        } else {
            Vec3::Z
        }
    }

    /// The brace on `side` of segment `segment` (0 is the bottom, at the
    /// frame's -Y end), if there is one, and which way it leans. Observed in
    /// Studio 0.742 with a dark block inside each truss so only its near
    /// side showed, and measured from the 2 × 4 × 2 export:
    ///
    /// - AlternatingSupports: segment 0 leans Up on the Front and Right
    ///   sides and Down on the Back and Left (so the braces turn the same
    ///   way round the truss), the next segment the other way, and so on up
    ///   from the bottom (checked on 2, 3, 4 and 5 segments).
    /// - BridgeStyleSupports: only the Right and Left sides, both leaning
    ///   Down in the lower floor(n / 2) segments and Up in the rest, so the
    ///   two sides look the same seen through the truss (checked on 2, 3, 4
    ///   and 5 segments, from both sides).
    pub fn brace(self, segment: u16, side: NormalId) -> Option<Lean> {
        match self.style {
            TrussStyle::NoSupports => None,
            TrussStyle::AlternatingSupports => {
                let first = match side {
                    NormalId::Front | NormalId::Right => Lean::Up,
                    _ => Lean::Down,
                };
                Some(if segment.is_multiple_of(2) {
                    first
                } else {
                    first.flip()
                })
            }
            TrussStyle::BridgeStyleSupports => {
                if side.normal().x == 0.0 {
                    return None;
                }
                Some(if segment < self.segments / 2 {
                    Lean::Down
                } else {
                    Lean::Up
                })
            }
        }
    }
}

/// How a truss part draws: its [`TrussShape`], the turn from the shape's
/// frame to the part's axes, and where the shape's center is in the part's
/// axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrussLook {
    pub shape: TrussShape,
    /// Columns are the part-axis directions of the frame's X, Y and Z.
    pub turn: Mat3,
    pub offset: Vec3,
}

impl TrussLook {
    /// What a truss of `size` and `style` draws: segments along its long axis
    /// from its low end, as observed in Studio (`docs/behavior/meshes/trusses.md`). A Size Studio wouldn't keep (only from a file) is first snapped
    /// as [`truss_size`] snaps a script's (Inference: files in the archive only
    /// hold sizes Studio keeps).
    pub fn of(size: Vec3, style: TrussStyle) -> Self {
        let s = truss_size(size);
        let long = (0..3).find(|&axis| s[axis] != 2.0).unwrap_or(1);
        let length = s[long];
        // A small allowance for files that save 8.000067 or 3.9999999.
        let segments = ((length / TrussShape::SEGMENT) + 1e-4).floor() as u16;
        let shape = TrussShape::new(style, segments);
        let turn = match long {
            0 => Mat3::from_cols(Vec3::NEG_Z, Vec3::X, Vec3::NEG_Y),
            2 => Mat3::from_cols(Vec3::X, Vec3::Z, Vec3::NEG_Y),
            _ => Mat3::IDENTITY,
        };
        let start = -length.max(TrussShape::SEGMENT) / 2.0;
        let offset = turn.y_axis * (start + shape.length() / 2.0);
        Self {
            shape,
            turn,
            offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Studio read back after each Size was set.
    #[test]
    fn sizes_snap_as_in_studio() {
        for (set, got) in [
            ((2.0, 5.0, 2.0), (2.0, 5.0, 2.0)),
            ((3.0, 3.0, 3.0), (3.0, 2.0, 2.0)),
            ((2.0, 1.0, 2.0), (2.0, 1.0, 2.0)),
            ((4.0, 4.0, 4.0), (4.0, 2.0, 2.0)),
            ((6.0, 2.0, 10.0), (2.0, 2.0, 10.0)),
            ((2.9, 7.0, 2.0), (2.0, 7.0, 2.0)),
            ((3.1, 2.0, 2.0), (3.1, 2.0, 2.0)),
            ((1.0, 3.0, 2.0), (2.0, 3.0, 2.0)),
            ((1.0, 1.0, 2.0), (2.0, 2.0, 2.0)),
            ((1.0, 1.0, 1.0), (2.0, 2.0, 2.0)),
            ((5.0, 1.0, 1.0), (5.0, 2.0, 2.0)),
            ((1.0, 3.0, 3.0), (2.0, 3.0, 2.0)),
            ((0.0, 0.0, 0.0), (2.0, 2.0, 2.0)),
            ((2.0, 2.0, 0.2), (2.0, 2.0, 0.2)),
            ((-4.0, 2.0, 2.0), (0.001, 2.0, 2.0)),
            ((2.0, 3000.0, 2.0), (2.0, 512.0, 2.0)),
            ((600.0, 600.0, 2.0), (512.0, 2.0, 2.0)),
            ((1.0, 1.5, 1.0), (2.0, 2.0, 2.0)),
            ((2.0, 1.0, 1.9), (2.0, 2.0, 2.0)),
            ((1.0, 2.0, 2.0), (1.0, 2.0, 2.0)),
            ((1.0, 2.0, 3.0), (2.0, 2.0, 3.0)),
        ] {
            assert_eq!(truss_size(set.into()), got.into(), "{set:?}");
        }
    }

    #[test]
    fn segments_start_at_the_low_end() {
        let look = |y: f32| TrussLook::of(Vec3::new(2.0, y, 2.0), TrussStyle::AlternatingSupports);
        assert_eq!(look(1.0).shape.segments, 1);
        assert_eq!(look(1.0).offset, Vec3::ZERO);
        assert_eq!(look(3.0).shape.segments, 1);
        assert_eq!(look(3.0).offset, Vec3::new(0.0, -0.5, 0.0));
        assert_eq!(look(5.0).shape.segments, 2);
        assert_eq!(look(5.0).offset, Vec3::new(0.0, -0.5, 0.0));
        assert_eq!(look(10.0).shape.segments, 5);
        assert_eq!(look(10.0).offset, Vec3::ZERO);
        let x = TrussLook::of(Vec3::new(5.0, 2.0, 2.0), TrussStyle::NoSupports);
        assert_eq!(x.offset, Vec3::new(-0.5, 0.0, 0.0));
        let z = TrussLook::of(Vec3::new(2.0, 2.0, 8.000067), TrussStyle::NoSupports);
        assert_eq!(z.shape.segments, 4);
        // The turns are rotations.
        for look in [x, z] {
            assert!((look.turn.determinant() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn one_segment_is_never_braced() {
        for style in [
            TrussStyle::AlternatingSupports,
            TrussStyle::BridgeStyleSupports,
        ] {
            let one = TrussShape::new(style, 1);
            assert_eq!(one, TrussShape::new(TrussStyle::NoSupports, 1));
            assert!(TrussShape::SIDES.iter().all(|&s| one.brace(0, s).is_none()));
        }
    }

    #[test]
    fn braces_follow_studio() {
        let alt = TrussShape::new(TrussStyle::AlternatingSupports, 3);
        assert_eq!(alt.brace(0, NormalId::Front), Some(Lean::Up));
        assert_eq!(alt.brace(0, NormalId::Left), Some(Lean::Down));
        assert_eq!(alt.brace(1, NormalId::Front), Some(Lean::Down));
        assert_eq!(alt.brace(2, NormalId::Front), Some(Lean::Up));
        let bridge = TrussShape::new(TrussStyle::BridgeStyleSupports, 5);
        assert_eq!(bridge.brace(0, NormalId::Front), None);
        let leans: Vec<_> = (0..5)
            .map(|i| bridge.brace(i, NormalId::Right).unwrap())
            .collect();
        assert_eq!(
            leans,
            [Lean::Down, Lean::Down, Lean::Up, Lean::Up, Lean::Up]
        );
        assert_eq!(bridge.brace(1, NormalId::Left), Some(Lean::Down));
    }
}
