use spade::{ConstrainedDelaunayTriangulation, Point2, PositionInTriangulation, Triangulation};

use super::{Line, Ring, fill, trace};

/// Whether `ring` holds the point at `(u, v)` of a 128-stud side solid,
/// its outline running through `points` in order.
fn solid_at(points: &[[f64; 2]], ring: Ring, probes: &[[f64; 2]]) -> Vec<bool> {
    let mut cdt: ConstrainedDelaunayTriangulation<Point2<f64>> =
        ConstrainedDelaunayTriangulation::new();
    for p in [[0.0, 0.0], [128.0, 0.0], [0.0, 128.0], [128.0, 128.0]] {
        cdt.insert(Point2::new(p[0], p[1])).unwrap();
    }
    let handles: Vec<_> = points
        .iter()
        .map(|p| cdt.insert(Point2::new(p[0], p[1])).unwrap())
        .collect();
    let lines: Vec<Line> = handles
        .windows(2)
        .map(|w| (w[0], w[1], ring, None))
        .collect();
    for &(a, b, _, _) in &lines {
        cdt.add_constraint_and_split(a, b, |p| p);
    }
    let (along, _) = trace(&cdt, &lines);
    let state = fill(&cdt, &along, ring).expect("the outline crosses the side");
    probes
        .iter()
        .map(|p| match cdt.locate(Point2::new(p[0], p[1])) {
            PositionInTriangulation::OnFace(f) => state[f.index()].expect("every face counted"),
            other => panic!("probe {p:?} at {other:?}"),
        })
        .collect()
}

/// A folded surface cuts the side in a figure eight: a hill whose top
/// twists into a small loop running backwards. The hill stays solid and
/// the air above it stays air; the loop doesn't spread its wrong side
/// across the side (it once filled the side up to its top corners).
#[test]
fn an_outline_crossing_itself_keeps_to_its_faces() {
    // The coarser ring's solid lies left of its outline: up from the
    // bottom right, across the top, back down to the bottom left, with
    // the top crossing itself at (60, 55).
    let hill = [
        [110.0, 0.0],
        [64.0, 50.0],
        [56.0, 60.0],
        [64.0, 60.0],
        [56.0, 50.0],
        [20.0, 0.0],
    ];
    let probes = [[60.0, 20.0], [4.0, 124.0], [124.0, 124.0], [60.0, 100.0]];
    assert_eq!(
        solid_at(&hill, Ring::Coarser, &probes),
        [true, false, false, false]
    );
}
