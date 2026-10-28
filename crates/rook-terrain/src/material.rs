//! The materials a voxel can be made of: a subset of `Enum.Material`, by
//! the index `SmoothGrid` stores (the same order as `Terrain:ReadVoxels`'
//! materials and `Terrain.MaterialColors`).

mod looks;
mod recolor;

pub use recolor::{Coloring, Recolor};

use rook_types::{Color3uint8, Material, MaterialColors, MaterialSet, TerrainMaterials};

macro_rules! terrain_materials {
    ($($name:ident = $index:literal),* $(,)?) => {
        /// A voxel's material, by its `SmoothGrid` index.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
        #[repr(u8)]
        pub enum TerrainMaterial {
            #[default]
            $($name = $index,)*
        }

        impl TerrainMaterial {
            /// Every material, by index.
            pub const ALL: [TerrainMaterial; 23] = [$(TerrainMaterial::$name,)*];

            /// The material with this `SmoothGrid` index.
            pub fn from_index(index: u8) -> Option<Self> {
                Self::ALL.get(usize::from(index)).copied()
            }

            /// The same material in `Enum.Material`.
            pub fn material(self) -> Material {
                match self {
                    $(Self::$name => Material::$name,)*
                }
            }

            /// The terrain material of an `Enum.Material`, if it is one.
            pub fn from_material(material: Material) -> Option<Self> {
                match material {
                    $(Material::$name => Some(Self::$name),)*
                    _ => None,
                }
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$name => stringify!($name),)*
                }
            }
        }
    };
}

terrain_materials! {
    Air = 0,
    Water = 1,
    Grass = 2,
    Slate = 3,
    Concrete = 4,
    Brick = 5,
    Sand = 6,
    WoodPlanks = 7,
    Rock = 8,
    Glacier = 9,
    Snow = 10,
    Sandstone = 11,
    Mud = 12,
    Basalt = 13,
    Ground = 14,
    CrackedLava = 15,
    Asphalt = 16,
    Cobblestone = 17,
    Ice = 18,
    LeafyGrass = 19,
    Salt = 20,
    Limestone = 21,
    Pavement = 22,
}

/// How a material's surface is shaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edges {
    /// Smooth normals, averaged around each vertex.
    Soft,
    /// Each triangle flat.
    Hard,
    /// Studio's `hardsoft` (Rock, Basalt, Cobblestone, Pavement): drawn
    /// faceted, as observed for Pavement.
    HardSoft,
}

impl Edges {
    /// Whether triangles are shaded flat.
    pub fn flat(self) -> bool {
        self != Edges::Soft
    }
}

/// One of a material's textures: a TexturePack asset, and how Studio lays
/// it (Studio's own names).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Texture {
    pub pack: u64,
    /// Repeats per voxel: the pack covers `4 / tiling` studs.
    pub tiling: f32,
    /// How Studio shifts the texture at random: by whole steps of
    /// 1 / `detiling` repeats ([`Texture::shift`]).
    pub detiling: f32,
    /// How far Studio turns the texture at random: 1 up to 90 degrees
    /// either way.
    pub rotation: f32,
}

impl Texture {
    /// Studs one repeat of the texture covers.
    pub fn studs(&self) -> f32 {
        crate::VOXEL_STUDS / self.tiling
    }

    /// Repeats per step of the random shift: 1 / `detiling` (Inference:
    /// Studio's older `detiling_legacy` field, next to `detiling` in its
    /// `materials.json`, is exactly that in every legacy row, and its
    /// shader multiplies the random number by one such factor).
    pub fn shift(&self) -> f32 {
        if self.detiling > 0.0 {
            1.0 / self.detiling
        } else {
            0.0
        }
    }
}

/// How a material shapes and textures the surface in one material set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// A pseudo-random offset of each vertex, in cells.
    pub shift: f32,
    /// How far each vertex moves towards its cell's center (0 to 1).
    pub cubify: f32,
    /// Vertices round to multiples of this fraction of a cell (0: none).
    pub quantize: f32,
    pub edges: Edges,
    /// Textures laid like a box's (Studio's `"mapping": "cube"`).
    pub cube: bool,
    /// The textures on faces turned up, sideways and down (the same pack
    /// for a material with one).
    pub top: Option<Texture>,
    pub side: Option<Texture>,
    pub bottom: Option<Texture>,
}

/// Every texture pack terrain uses, in either set (tools: Rook keeps
/// these, `rook-tools content --pin`).
pub fn all_packs() -> impl Iterator<Item = u64> {
    TerrainMaterial::ALL.into_iter().flat_map(|m| {
        [MaterialSet::Legacy, MaterialSet::Materials2022]
            .into_iter()
            .flat_map(move |set| {
                let look = m.textures(set);
                [look.top, look.side, look.bottom]
            })
            .flatten()
            .map(|t| t.pack)
    })
}

impl TerrainMaterial {
    /// How this material shapes and shades the surface: as Studio's
    /// legacy (`Pre2022`) table says, in places of either material set
    /// (observed in places saved with `Use2022Materials` on and off:
    /// Concrete stepped by half cells, Slate and Basalt shifted and
    /// Cobblestone quantized, as only the legacy table has them).
    pub fn look(self) -> &'static Look {
        &looks::LOOKS[self as usize]
    }

    /// Its textures in a place of `set` (observed 2026-10-28: switching
    /// `Use2022Materials` in Studio switches terrain between the tables'
    /// packs, the 2022 ones tinted by `MaterialColors`).
    pub fn textures(self, set: MaterialSet) -> &'static Look {
        match set {
            MaterialSet::Legacy => &looks::LOOKS[self as usize],
            MaterialSet::Materials2022 => &looks::LOOKS_2022[self as usize],
        }
    }

    /// Whether its top, side and bottom textures differ in either set, so
    /// its surfaces are split by which way they face.
    pub fn faces_differ(self) -> bool {
        [MaterialSet::Legacy, MaterialSet::Materials2022]
            .iter()
            .any(|&set| {
                let look = self.textures(set);
                look.top != look.side || look.side != look.bottom
            })
    }

    /// Its color in `Terrain.MaterialColors`, or `set`'s default when the
    /// place saved none (Studio's default is the 2022 set's: a new place
    /// saves those); Air and Water have none.
    pub fn color(self, colors: Option<&MaterialColors>, set: MaterialSet) -> Option<Color3uint8> {
        let key: TerrainMaterials = self.name().parse().ok()?;
        Some(match colors {
            Some(colors) => colors.get_color(key),
            None => {
                let table = match set {
                    MaterialSet::Legacy => &looks::BASE_LEGACY,
                    MaterialSet::Materials2022 => &looks::BASE_2022,
                };
                let [r, g, b] = table[self as usize];
                Color3uint8::new(r, g, b)
            }
        })
    }
}

#[cfg(test)]
mod tests;
