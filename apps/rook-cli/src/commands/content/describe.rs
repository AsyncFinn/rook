//! What a resolved file holds, from its bytes: a mesh's levels, a union's
//! collision hulls, an image's or a sound's format.

/// What a resolved file looks like, from its first bytes.
pub(super) fn describe(bytes: &[u8]) -> String {
    let starts = |magic: &[u8]| bytes.starts_with(magic);
    if starts(b"version ") {
        let line = bytes.iter().take(16).take_while(|&&b| b != b'\n');
        let line: String = line.map(|&b| b as char).collect();
        let detail = match rook_content::decode::mesh::Mesh::decode(bytes) {
            Ok(m) => mesh_detail(&m),
            Err(e) => e.to_string(),
        };
        return format!("mesh, {}, {detail}", line.trim_end());
    }
    if starts(b"<roblox")
        && let Ok(data) = rook_content::decode::mesh::union::asset_mesh_data(bytes)
    {
        let kind = rook_content::decode::mesh::union::MeshDataKind::of(&data).name();
        let mesh = match rook_content::decode::mesh::Mesh::decode(&data) {
            Ok(m) => mesh_detail(&m),
            Err(e) => e.to_string(),
        };
        return format!(
            "union asset, MeshData {kind}, {mesh}; PhysicsData {}",
            physics_detail(bytes)
        );
    }
    let kind = if starts(b"\x89PNG") {
        "PNG image"
    } else if starts(b"\xFF\xD8\xFF") {
        "JPEG image"
    } else if starts(b"DDS ") {
        "DDS texture"
    } else if let Some(format) = rook_content::decode::audio::AudioFormat::sniff(bytes) {
        return match rook_content::decode::audio::Audio::decode(bytes) {
            Ok(a) => format!(
                "{} sound, {:.6} s, {} Hz, {} channel(s)",
                format.name(),
                a.seconds(),
                a.sample_rate,
                a.channels
            ),
            Err(e) => e.to_string(),
        };
    } else if starts(b"<roblox!") {
        "binary model"
    } else if starts(b"<roblox") {
        "XML model"
    } else {
        "unknown"
    };
    kind.to_owned()
}

/// A union asset's collision geometry: its form, and its hulls' count,
/// points and bounds.
fn physics_detail(bytes: &[u8]) -> String {
    use rook_content::decode::mesh::union::physics;
    use rook_types::SavedCollision;
    let hulls = match physics::decode_asset(bytes) {
        Ok(None) => return "none".into(),
        Ok(Some(SavedCollision::Block)) => return "BLOCK".into(),
        Ok(Some(SavedCollision::Hulls { hulls, .. })) => hulls,
        Err(e) => return e.to_string(),
    };
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in hulls.iter().flat_map(|h| &h.positions) {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    let points: usize = hulls.iter().map(|h| h.positions.len()).sum();
    let size = [0, 1, 2].map(|i| format!("{:.3}", hi[i] - lo[i]));
    format!(
        "{} hulls, {points} points, {} studs",
        hulls.len(),
        size.join(" x ")
    )
}

/// A decoded mesh's triangles (its levels of detail), size and center.
fn mesh_detail(m: &rook_content::decode::mesh::Mesh) -> String {
    let (lo, hi) = m.bounds();
    let size = [0, 1, 2].map(|i| format!("{:.3}", hi[i] - lo[i]));
    let center = [0, 1, 2].map(|i| format!("{:.3}", (hi[i] + lo[i]) / 2.0));
    let mut uv = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for t in &m.uvs {
        uv = [
            uv[0].min(t[0]),
            uv[1].min(t[1]),
            uv[2].max(t[0]),
            uv[3].max(t[1]),
        ];
    }
    let mut levels = m.levels().map(|l| (l.len() / 3).to_string());
    let full = levels.next().unwrap_or_default();
    let lower: Vec<_> = levels.collect();
    let lower = match lower.is_empty() {
        true => String::new(),
        false => format!(
            " (lower levels {}, {} high quality)",
            lower.join(", "),
            m.levels.high_quality
        ),
    };
    format!(
        "{full} triangles{lower}, {} studs, centered at ({}), UVs {:.2}..{:.2} x {:.2}..{:.2}",
        size.join(" x "),
        center.join(", "),
        uv[0],
        uv[2],
        uv[1],
        uv[3]
    )
}
