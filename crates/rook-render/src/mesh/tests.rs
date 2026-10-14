use super::*;

/// Every face's normal points away from the centre.
#[test]
fn normals_point_outward() {
    for shape in DrawShape::ALL {
        let m = build(shape);
        for tri in m.indices.chunks(3) {
            let [a, b, c] =
                [tri[0], tri[1], tri[2]].map(|i| Vec3::from(m.vertices[i as usize].position));
            let face_normal = (b - a).cross(c - a);
            let centroid = (a + b + c) / 3.0;
            assert!(
                face_normal.dot(centroid) > -1e-6,
                "{shape:?}: inward-facing triangle"
            );
        }
    }
}

/// The user's own head mesh, when an install is found, faces outward
/// like Rook's (same winding) and is centred like it.
#[test]
fn installed_head_matches_rooks_frame() {
    let Some(install) = rook_content::install::newest() else {
        eprintln!("skipped: no Roblox install found");
        return;
    };
    let Ok(bytes) = std::fs::read(install.content[0].join("avatar/heads/head.mesh")) else {
        eprintln!("skipped: no head.mesh in the install");
        return;
    };
    let m = from_file(&rook_content::mesh::Mesh::decode(&bytes).unwrap());
    let outward = m
        .indices
        .chunks(3)
        .filter(|tri| {
            let [a, b, c] =
                [tri[0], tri[1], tri[2]].map(|i| Vec3::from(m.vertices[i as usize].position));
            (b - a).cross(c - a).dot((a + b + c) / 3.0) > -1e-6
        })
        .count();
    assert_eq!(outward, m.indices.len() / 3);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &m.vertices {
        lo = lo.min(Vec3::from(v.position));
        hi = hi.max(Vec3::from(v.position));
    }
    assert!((lo + hi).abs().max_element() < 1e-3, "{lo} {hi}");
}

/// The round shapes keep each vertex's grid point on the box face it
/// belongs to, with the measured vertex and triangle counts.
#[test]
fn round_shapes_map_from_their_box_faces() {
    for (shape, triangles) in [(PartShape::Ball, 432), (PartShape::Cylinder, 96)] {
        let m = build(DrawShape::Part(shape));
        let side_triangles = m.indices.len() / 3;
        if shape == PartShape::Ball {
            assert_eq!((m.vertices.len(), side_triangles), (294, triangles));
        } else {
            // 24 side quads; the flat ends as 22-triangle fans (the
            // export's are 24-triangle fans around a centre).
            assert_eq!(side_triangles, 48 + 2 * 22);
        }
        for v in &m.vertices {
            let face = NormalId::ALL[v.face as usize];
            let b = Vec3::from(v.box_point);
            assert!((b.dot(face.normal()) - 0.5).abs() < 1e-5, "{shape:?} {b}");
            assert!(b.abs().max_element() <= 0.5 + 1e-5);
            assert!(
                (Vec3::from(v.position).length() - 0.5).abs() < 1e-5
                    || shape == PartShape::Cylinder
            );
        }
    }
    // The cylinder's side corners: atan(1/3) from the axis of its face.
    let m = build(DrawShape::Part(PartShape::Cylinder));
    let angle = (1.0f32 / 3.0).atan();
    assert!(m.vertices.iter().any(|v| {
        let p = Vec3::from(v.position);
        (p.z.atan2(p.y) - angle).abs() < 1e-5 && p.x > 0.0
    }));
}

/// The head's UVs: the front quarter projected straight on, the rest
/// at (0, 0).
#[test]
fn head_uvs_project_the_front_quarter() {
    let m = build(DrawShape::Head);
    let mut front = 0;
    for v in &m.vertices {
        if v.uv == [0.0; 2] {
            continue;
        }
        front += 1;
        let [x, y, z] = v.position;
        // In the front quarter, or a cap's centre.
        assert!(x.abs() <= -z + 1e-4, "{:?}", v.position);
        let span = 2.0 * HEAD_RADIUS;
        assert!((v.uv[0] - (0.5 - x / span)).abs() < 1e-6);
        assert!((v.uv[1] - (0.5 - y / span)).abs() < 1e-6);
    }
    assert!(front > 0);
    // The middle of the front maps to the middle of the image.
    assert!(m.vertices.iter().any(|v| v.uv[0] > 0.45 && v.uv[0] < 0.55));
}

/// The block's front face shows the image upright, unmirrored from the
/// front: its top-left at +X, +Y.
#[test]
fn block_uvs_follow_studios_faces() {
    let m = build(DrawShape::Part(PartShape::Block));
    let front = m
        .vertices
        .iter()
        .find(|v| v.normal == [0.0, 0.0, -1.0] && v.position == [0.5, 0.5, -0.5])
        .unwrap();
    assert_eq!(front.uv, [0.0, 0.0]);
}

/// A truss's faces each face the way their normal says, and its bars sit
/// where the export of a 2 × 4 × 2 truss has them: rails out to the
/// 2-stud box over the whole length, and each brace's outline reaching
/// 0.9634 studs from its segment's middle both across and along.
#[test]
fn truss_matches_the_export() {
    use rook_types::TrussStyle;
    let shape = TrussShape::new(TrussStyle::AlternatingSupports, 2);
    let m = build(DrawShape::Truss(shape));
    for tri in m.indices.chunks(3) {
        let v = [tri[0], tri[1], tri[2]].map(|i| m.vertices[i as usize]);
        let [a, b, c] = v.map(|v| Vec3::from(v.position));
        let n = (b - a).cross(c - a).normalize();
        assert!(n.dot(Vec3::from(v[0].normal)) > 0.999);
    }
    let (lo, hi) = m.bounds();
    assert_eq!(
        (lo, hi),
        (Vec3::new(-1.0, -2.0, -1.0), Vec3::new(1.0, 2.0, 1.0))
    );
    // The braces are the only faces with diagonal normals: on the
    // Front side of the lower segment they span x and y within
    // ±0.9634 of (0, -1).
    let brace: Vec<Vec3> = m
        .vertices
        .iter()
        .filter(|v| v.normal[0].abs() > 0.1 && v.normal[1].abs() > 0.1)
        .map(|v| Vec3::from(v.position))
        .filter(|p| p.y < 0.0 && p.z < 0.0)
        .collect();
    let reach = brace
        .iter()
        .map(|p| p.x.abs().max((p.y + 1.0).abs()))
        .fold(0.0, f32::max);
    assert!((reach - 0.9634).abs() < 1e-4, "{reach}");
    assert!(brace.iter().all(|p| (-0.9375..=-0.8125).contains(&p.z)));
    // The joint's rung is 0.5 thick: a top face at y = 0.25.
    assert!(m.vertices.iter().any(|v| v.normal == [0.0, 1.0, 0.0]
        && (v.position[1] - 0.25).abs() < 1e-6
        && v.position[2] == -1.0));
    // One segment: no braces.
    let one = build(DrawShape::Truss(TrussShape::new(
        TrussStyle::AlternatingSupports,
        1,
    )));
    assert!(
        one.vertices
            .iter()
            .all(|v| v.normal.iter().filter(|c| **c != 0.0).count() == 1)
    );
}

/// The head's measured outline: 1.2024 studs tall, and as wide as its
/// 36 segments allow (R cos 5 degrees seen from the front or side).
#[test]
fn head_matches_the_measured_bounds() {
    let m = build(DrawShape::Head);
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &m.vertices {
        lo = lo.min(Vec3::from(v.position));
        hi = hi.max(Vec3::from(v.position));
    }
    let wide = HEAD_RADIUS * 5.0f32.to_radians().cos();
    assert!(
        (hi - Vec3::new(wide, HEAD_RADIUS, wide))
            .abs()
            .max_element()
            < 1e-5
    );
    assert!((lo + hi).abs().max_element() < 1e-5);
    for v in &m.vertices {
        assert!((Vec3::from(v.normal).length() - 1.0).abs() < 1e-5);
    }
}
