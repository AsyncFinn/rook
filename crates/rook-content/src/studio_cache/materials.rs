//! Which of the textures in Roblox's content cache belong to which
//! material: each texture's `contentHash`, measured in Studio 0.742 on
//! 2026-10-14 by giving each material its own window in a fresh Studio and
//! confirmed by correlating normal and colour maps
//! (`docs/behavior/materials.md`). These are identifiers, not Roblox's
//! files; the textures themselves are read from the user's own cache.
//!
//! An empty entry has no texture of its own in Studio (a 4 × 4 placeholder:
//! Foil's and Ice's colour, Granite's and Pebble's normal).

use rook_types::Material;

use crate::MaterialMap;

/// Each textured material's colour, normal and roughness-metalness maps.
#[rustfmt::skip]
const TABLE: &[(Material, [&str; 3])] = &[
    (Material::Wood, ["e4c89e04bd8bafa133218e475890d457", "22755f464089f2c0e7df0b84dc389611", "72599faed18666ffbb9e711170e6bd4c"]),
    (Material::WoodPlanks, ["db0b2d38998c49b49326c6bc0a389441", "025aa8fe54d30e543ebc165201f2b178", "74c2143d337c3906c0b447456c030068"]),
    (Material::Marble, ["6f09f886f2612a9011714bcdcebd31e0", "6462db1dec207210d69ca046b2b138a9", "e2f60316fac82962ca1ed1da779c0066"]),
    (Material::Basalt, ["69976c70a8da01bdd5c4912b6d457c50", "b4734aa57869b52da8093d05ffcced65", "fad1510bd90589072e28a7b740c0645a"]),
    (Material::Slate, ["0e3af5d029c8aafa78c0408ab557a6b4", "bd73df16a827bb828cc3860b98d2f343", "e3d716b45ab78443875ae40564a3dbcf"]),
    (Material::CrackedLava, ["09952a68fb2f5d30e218dfa80b2290d0", "e63f8de21e9a1b60a320d2c0389d6cff", "fe232a76ecc9032a0e9798384af7cd22"]),
    (Material::Concrete, ["b5adbba4c9e50745fa0d828bef43cb52", "7da97e874a3f1aac6d9182c373d7c782", "f3ef803be6fabe61517fce8ffc11e0cf"]),
    (Material::Limestone, ["359d07c081895a0d14888841e6a3ad93", "84e19abd893d927ac61004164db256d8", "7bd687b8c62cbd5f7aeaf11e5119bfca"]),
    (Material::Granite, ["22dd6f2c16636eac8f08c1b8c04a9da8", "", "d17e13f19efe1797b0856b04b2656136"]),
    (Material::Pavement, ["1d67f798968e1b92ba879cff27630571", "e347d946a2c41a73859adf636db67023", "6578fc3ab9347a631f5be43a9f35c3ae"]),
    (Material::Brick, ["dbc27ebb015b7681afda3ad7bbaa3a04", "c450c9dca766487d138798e690bd9f7b", "da112cd98406bdb283e53a0894d51ae3"]),
    (Material::Pebble, ["84c511338ef39d591bc824ea7606dc32", "", "5e543895913d21723edb885ad55919a6"]),
    (Material::Cobblestone, ["cbd4504c17b83172ac9090f76c8538ca", "7ce10418232cc444b20caccf7a382e12", "604bd84db66494098781b5e2302b2270"]),
    (Material::Rock, ["3931214bbdc95c87af1885f93dc081d7", "f6a0c1c343aa0b75c7dfb4bd5c2974fd", "96d69c6a748b530d623d51792f322142"]),
    (Material::Sandstone, ["236cf93054f838bc5328439d96c3fb53", "40180bc221190c666727967ee92ae33f", "4f3755f269dfe4aab5846216467b45db"]),
    (Material::CorrodedMetal, ["7d415d3b3f4b4dcee0fd8c6e8b451480", "c3158a7b390fe0d97bea4110822ea7c2", "1eda85f781cd658db5901cb54a12ddcf"]),
    (Material::DiamondPlate, ["52c318b01613af6d757337ed665d982b", "c951732dc659e49d0a1efba114be7cfd", "a4c8f11162fd2a22ef2ff19874ffba49"]),
    (Material::Foil, ["", "db2c01bd21c9979165ef841c31678a92", "36d17556523b0509f4046c66840ce996"]),
    (Material::Metal, ["56076fd2ac3d78ff68ab73f45d9d508e", "2ed6013039829d7e0cf7982ba8c5c8ca", "19bc6ef4d2c4c60d12b99a8ae0f8c64d"]),
    (Material::Grass, ["a3c024aabd504905ddbd18d4d6fbd08c", "1f695c6d95a3fe938e7b10fd59140659", "d219b04367c9fbfc318cc44402a9e803"]),
    (Material::LeafyGrass, ["dc9b77342a4a5565bea8ddc6a4c6149d", "3eabef43a6963b66606c02156f02e03e", "96aac2d04122eae87176e249b2d6c9a9"]),
    (Material::Sand, ["e4e6c646899dc8adf1bc017ed3fff55d", "3dbff8a6139e18500053eacb448524fd", "94794e1e6d782d6daf13838dd1f376f8"]),
    (Material::Fabric, ["35f3367872d426f0527ceeb9ed766138", "6f1eda9504e92b7e1f7eea78faf1637e", "cdb20fb5888a3a8bf4082c06d020db3d"]),
    (Material::Snow, ["5393efeea24e9c47173d44516a83ba1c", "cf18396e395812a18cb22baf5a2c8e87", "363796d25a2598d669ee391e1135701d"]),
    (Material::Mud, ["653b0b6e22f05751018e7984d7b35151", "ee7240b4525c38371a37bda7d62cd0a1", "518e293887b906d5112cf89fd59deac3"]),
    (Material::Ground, ["83a6c5ff0def859081666d92aee6aafe", "098f7ad7b3d8c38a11b49b685fdfea32", "22e3d41bd2b8f6fc79d307fa2b999dca"]),
    (Material::Asphalt, ["c448c1e8d5c7b46d9f158a79d9503d27", "530c41775ab450bdbdd96fe04be07d10", "01f402eb05abfdd971b9a3e1c0b1402f"]),
    (Material::Salt, ["03b0e2c2b00280ea70f401213b689a40", "99aea3a5c7c0847f50ebf16ba8e8d5ec", "ef4b97f847cca5db0a12a02c007678e6"]),
    (Material::Ice, ["", "c1f027c5d6d6f9e02cac5c7fc5c728ef", "1ce687aaac190a68b60fb08c6c61eb51"]),
    (Material::Glacier, ["6153ce79565444fe5ed7b933459799be", "c7f090a3e82c793d143d29036d246c3f", "bf3464fcbb70404097fbb6bc2864b6a0"]),
    (Material::Glass, ["28562b34d6265769acc6602e430f2ec4", "bb0a43139011255ed1b4d62ae1ec11f9", "ff2e2dcb23d8be5eb2c8c2f88dfcba43"]),
    (Material::Cardboard, ["9936fc564162705778421becf90514b2", "56c15c2e5ddd9de2a483499cb6e285bb", "8574eec8522201393f7a8f2364f31c55"]),
    (Material::Carpet, ["f065ec85467cd73bc8a80b496061e9b1", "970ea1f873e60bc813406a152f54ffeb", "5c60eae621d64ede8feb26563e9d847c"]),
    (Material::CeramicTiles, ["fc678422408e25743130af103e357ae9", "99af8e967dad4caec2cf9c381d3ecd15", "e979d331077cd24ac930ba9c99483dd5"]),
    (Material::ClayRoofTiles, ["e5d53b67e4d0ccbc487f5f8422063ebf", "36b961de11f07383a1aba4ea5b886da7", "573c2a20b13af25474c5de740a6563e6"]),
    (Material::RoofShingles, ["4fd8c06a4294d0499d69af51f39f9b49", "6305f8ef2e7e2dbd33b80be2dd08e3c0", "e1a7335f698b6274e35b693791f76f04"]),
    (Material::Leather, ["28581d3e09118eeb780364616361a1bf", "b2b52a88f9d95b86ec9da8650148b6ee", "54fbcbf66d3cba0951164457afbdaff5"]),
    (Material::Plaster, ["ae2b52f87f387847f4ff9805c24d8973", "8569d1b1e5ad38d757e7ceb0a351887a", "bfba16bbabef2320be1ec2e79d8a37b9"]),
    (Material::Rubber, ["be7ad5da9fc5c2ccd40f55e3fee73fbe", "1a03f7eb18617b6b906da6c48d53a24a", "8a34124e81336be815788bb1dd79b62d"]),
];

/// The `contentHash` of `material`'s `map`, if Studio has one.
pub fn content_hash(material: Material, map: MaterialMap) -> Option<&'static str> {
    let (_, maps) = TABLE.iter().find(|(m, _)| *m == material)?;
    Some(maps[map as usize]).filter(|h| !h.is_empty())
}

/// Every texture in the table.
pub(super) fn all() -> impl Iterator<Item = &'static str> {
    TABLE
        .iter()
        .flat_map(|(_, maps)| maps.iter().copied())
        .filter(|h| !h.is_empty())
}
