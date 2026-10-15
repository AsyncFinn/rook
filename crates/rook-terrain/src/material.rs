//! The materials a voxel can be made of: a subset of `Enum.Material`, by
//! the index `SmoothGrid` stores (the same order as `Terrain:ReadVoxels`'
//! materials and `Terrain.MaterialColors`).

use rook_types::{Color3uint8, Material, MaterialColors, TerrainMaterials};

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

impl TerrainMaterial {
    /// Its colour in `Terrain.MaterialColors` (the default table when the
    /// place saved none); Air and Water have none there.
    pub fn color(self, colors: Option<&MaterialColors>) -> Option<Color3uint8> {
        let key: TerrainMaterials = self.name().parse().ok()?;
        Some(match colors {
            Some(colors) => colors.get_color(key),
            None => key.default_color(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_and_names() {
        for (i, m) in TerrainMaterial::ALL.iter().enumerate() {
            assert_eq!(*m as usize, i);
            assert_eq!(m.material().name(), m.name());
        }
        assert_eq!(TerrainMaterial::from_index(23), None);
        assert!(TerrainMaterial::Grass.color(None).is_some());
        assert!(TerrainMaterial::Water.color(None).is_none());
    }
}
