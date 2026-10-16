//! The materials a voxel can be made of: a subset of `Enum.Material`, by
//! the index `SmoothGrid` stores (the same order as `Terrain:ReadVoxels`'
//! materials and `Terrain.MaterialColors`).

mod looks;
mod tint;

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
    /// How strongly Studio breaks up the repeats (not drawn yet).
    pub detiling: f32,
    /// How far Studio turns the texture at random (not drawn yet).
    pub rotation: f32,
}

impl Texture {
    /// Studs one repeat of the texture covers.
    pub fn studs(&self) -> f32 {
        crate::VOXEL_STUDS / self.tiling
    }
}

/// How a material shapes and textures the surface in one material set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// A pseudo-random offset of each vertex, in cells.
    pub shift: f32,
    /// How far each vertex moves towards its cell's centre (0 to 1).
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

impl TerrainMaterial {
    /// How this material looks in `set`.
    pub fn look(self, set: MaterialSet) -> &'static Look {
        let table = match set {
            MaterialSet::Legacy => &looks::LEGACY,
            MaterialSet::Materials2022 => &looks::MATERIALS_2022,
        };
        &table[self as usize]
    }

    /// How this material shapes and shades the surface: as the legacy
    /// set's table says, in places of either set (observed in places saved
    /// with `Use2022Materials` on and off: Concrete stepped by half cells,
    /// Slate and Basalt shifted and Cobblestone quantized, as only the
    /// legacy table has them).
    pub fn shape(self) -> &'static Look {
        self.look(MaterialSet::Legacy)
    }

    /// Its textures: the 2022 set's, in places of either set (observed:
    /// Studio drew the same scene alike with `Use2022Materials` on and
    /// off, and CrackedLava's cracks in the place's magenta colour, as the
    /// 2022 set's grey texture tinted gives and the legacy set's orange one
    /// doesn't).
    pub fn textures(self) -> &'static Look {
        self.look(MaterialSet::Materials2022)
    }

    /// Its colour in `Terrain.MaterialColors`, or `set`'s default when the
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
mod tests {
    use super::*;

    #[test]
    fn indices_and_names() {
        for (i, m) in TerrainMaterial::ALL.iter().enumerate() {
            assert_eq!(*m as usize, i);
            assert_eq!(m.material().name(), m.name());
        }
        assert_eq!(TerrainMaterial::from_index(23), None);
        let legacy = MaterialSet::Legacy;
        // rbx_types' defaults are the legacy set's.
        assert_eq!(
            TerrainMaterial::Grass.color(None, legacy),
            Some(TerrainMaterials::Grass.default_color())
        );
        assert!(TerrainMaterial::Water.color(None, legacy).is_none());
    }

    /// Spot checks of Studio's tables, and the shapes measured in a legacy
    /// place (docs/behavior/terrain.md).
    #[test]
    fn looks() {
        use TerrainMaterial::*;
        let legacy = |m: TerrainMaterial| m.look(MaterialSet::Legacy);
        assert_eq!(legacy(Concrete).quantize, 0.5);
        assert_eq!(legacy(Pavement).quantize, 0.25);
        assert_eq!(legacy(Brick).cubify, 1.0);
        assert_eq!(legacy(Slate).shift, legacy(CrackedLava).shift);
        assert!(legacy(Brick).cube && legacy(Brick).edges.flat());
        assert_eq!(legacy(Grass).top.unwrap().pack, 7547336797);
        assert_eq!(legacy(Grass).top.unwrap().studs(), 16.0);
        assert!(legacy(Air).top.is_none() && legacy(Water).side.is_none());
        let modern = Concrete.look(MaterialSet::Materials2022);
        assert_eq!(modern.quantize, 1.0);
    }

    /// The tints' shapes measured in Studio (docs/behavior/terrain.md).
    #[test]
    fn tints() {
        use TerrainMaterial::*;
        let rgb = |r, g, b| Color3uint8::new(r, g, b);
        // White keeps the texture's grey, and grey dims it.
        assert_eq!(Grass.tint(rgb(255, 255, 255)), [1.0; 3]);
        assert!(
            Sand.tint(rgb(128, 128, 128))
                .iter()
                .all(|&x| (x - 0.502).abs() < 0.001)
        );
        // Red turns magenta on Grass, and teal on Basalt.
        let [r, g, b] = Grass.tint(rgb(255, 0, 0));
        assert!(r > b && b > g);
        let [r, g, b] = Basalt.tint(rgb(255, 0, 0));
        assert!(g > r && b > r);
        // Rock's chroma tops out: pink and red look alike.
        assert_eq!(Rock.tint(rgb(255, 128, 128)), Rock.tint(rgb(255, 0, 0)));
        // Basalt is as bright in its default colour as in white.
        let luma = |[r, g, b]: [f32; 3]| (r + 2.0 * g + b) / 4.0;
        assert_eq!(luma(Basalt.tint(rgb(75, 74, 74))), 0.6);
        assert_eq!(luma(Basalt.tint(rgb(255, 255, 255))), 0.6);
        assert_eq!(Water.tint(rgb(255, 0, 0)), [1.0, 0.0, 0.0]);
    }
}
