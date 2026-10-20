//! What the renderer takes from the user's Roblox install.

use crate::*;

/// What the renderer takes from the user's Roblox install, read from disk
/// at startup: the Head MeshType's mesh, the surface patterns, water's
/// normal frames and the BRDF table reflections go by. Without an install
/// (or the files), Rook draws its own (and flat water).
pub(crate) struct Installed {
    pub(crate) head: Option<Mesh>,
    /// `textures/studs.dds`: the studs, lattice, inlet and universal tiles.
    pub(crate) surfaces: Option<Image>,
    /// `textures/water/normal_01.dds` to `normal_25.dds`, when all load.
    pub(crate) water: Option<Vec<Image>>,
    /// `textures/brdfLUT.dds`: its side and its pairs of half floats.
    pub(crate) brdf: Option<(u32, Vec<u8>)>,
}

/// How many water normal frames the install has (measured: Studio plays 25).
const WATER_FRAMES: usize = 25;

impl Installed {
    pub(crate) fn find(content: &Resolver) -> Self {
        let read = |path: &str| {
            let fetched = content.resolve_local(&ContentRef::File(path.into()))?;
            Some((fetched.source.location, fetched.bytes))
        };
        let warn = |location: &str, e: &dyn std::fmt::Display, what: &str| {
            console::print_message(
                MessageType::Warning,
                &format!("{location}: {e}; drawing Rook's own {what}"),
            );
        };
        let head = read("avatar/heads/head.mesh")
            .and_then(|(at, bytes)| Mesh::decode(&bytes).map_err(|e| warn(&at, &e, "head")).ok());
        let surfaces = read("textures/studs.dds").and_then(|(at, bytes)| {
            Image::decode(&bytes)
                .map_err(|e| warn(&at, &e, "surfaces"))
                .ok()
        });
        let water = (1..=WATER_FRAMES)
            .map(|i| {
                let (at, bytes) = read(&format!("textures/water/normal_{i:02}.dds"))?;
                Image::decode(&bytes)
                    .map_err(|e| warn(&at, &e, "flat water"))
                    .ok()
            })
            .collect();
        let brdf = read("textures/brdfLUT.dds").and_then(|(at, bytes)| {
            brdf_table(&bytes)
                .ok_or_else(|| warn(&at, &"not a square R16G16_FLOAT DDS", "BRDF table"))
                .ok()
        });
        Self {
            head,
            surfaces,
            water,
            brdf,
        }
    }

    pub(crate) fn apply(&self, renderer: &mut Renderer) {
        if let Some(head) = &self.head {
            renderer.set_head_mesh(head);
        }
        if let Some(atlas) = &self.surfaces
            && !renderer.set_surface_atlas(atlas)
        {
            console::print_message(
                MessageType::Warning,
                "textures/studs.dds isn't laid out as expected; drawing Rook's own surfaces",
            );
        }
        if let Some((size, texels)) = &self.brdf
            && !renderer.set_brdf_table(*size, texels)
        {
            console::print_message(
                MessageType::Warning,
                "textures/brdfLUT.dds isn't laid out as expected; drawing Rook's own BRDF table",
            );
        }
        if let Some(frames) = &self.water
            && !renderer.set_water_normals(frames)
        {
            console::print_message(
                MessageType::Warning,
                "textures/water/normal_*.dds aren't all one square size; drawing flat water",
            );
        }
    }
}

/// The texels of `brdfLUT.dds`: a square DX10 DDS of R16G16_FLOAT
/// (DXGI format 34) with one level, as the install's is. Returns its side
/// and the texels.
fn brdf_table(bytes: &[u8]) -> Option<(u32, Vec<u8>)> {
    /// The magic, the header and its DX10 extension.
    const HEADER: usize = 4 + 124 + 20;
    const R16G16_FLOAT: u32 = 34;
    let u32_at = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let (height, width) = (u32_at(12)?, u32_at(16)?);
    let four_cc = bytes.get(84..88)?;
    if !bytes.starts_with(b"DDS ")
        || four_cc != b"DX10"
        || u32_at(128)? != R16G16_FLOAT
        || width != height
    {
        return None;
    }
    let texels = bytes.get(HEADER..HEADER + (width * height * 4) as usize)?;
    Some((width, texels.to_vec()))
}
