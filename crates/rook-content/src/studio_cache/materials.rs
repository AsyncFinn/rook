//! Which of the textures in Roblox's content cache belong to which
//! material: each texture's `contentHash`, measured in Studio 0.742 on
//! 2026-10-14 by giving each material its own window in a fresh Studio and
//! confirmed by correlating normal and colour maps
//! (`docs/behavior/materials.md`). These are identifiers, not Roblox's
//! files; the textures themselves are read from the user's own cache.
//!
//! An empty entry has no texture of its own in Studio (a 4 × 4 placeholder:
//! in the legacy set Foil's and Ice's colour and Granite's and Pebble's
//! normal, in the 2022 set Granite's normal). The materials added in 2022
//! or later have one set of textures, shared by both sets.

use rook_types::{Material, MaterialSet};

use crate::MaterialMap;

/// Each textured material's colour, normal and roughness-metalness maps in
/// the legacy set (and the materials both sets share).
#[rustfmt::skip]
const LEGACY: &[(Material, [&str; 3])] = &[
    (Material::Wood, ["e4c89e04bd8bafa133218e475890d457", "22755f464089f2c0e7df0b84dc389611", "72599faed18666ffbb9e711170e6bd4c"]),
    (Material::WoodPlanks, ["db0b2d38998c49b49326c6bc0a389441", "025aa8fe54d30e543ebc165201f2b178", "74c2143d337c3906c0b447456c030068"]),
    (Material::Marble, ["6f09f886f2612a9011714bcdcebd31e0", "6462db1dec207210d69ca046b2b138a9", "e2f60316fac82962ca1ed1da779c0066"]),
    (Material::Basalt, ["69976c70a8da01bdd5c4912b6d457c50", "b4734aa57869b52da8093d05ffcced65", "fad1510bd90589072e28a7b740c0645a"]),
    (Material::Slate, ["0e3af5d029c8aafa78c0408ab557a6b4", "ff16ea9cebedddb7b138dd418453a6ef", "e3d716b45ab78443875ae40564a3dbcf"]),
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

/// The same for the 2022 set, measured in a place saved with
/// `Use2022Materials` on; materials it lacks share the legacy set's.
#[rustfmt::skip]
const MATERIALS_2022: &[(Material, [&str; 3])] = &[
    (Material::Wood, ["068a17909aaf0cda44025db5de98e6aa", "0687a4bb2384706f174f9e1f0a4eb346", "a80f0da3065c06c0956a7837afcc2504"]),
    (Material::WoodPlanks, ["4e45b4535587c465cf04e36f27d48de0", "a7f066b1b4d8e6ecc11d67d16009f6f5", "a4cfe74f71a55f8683b3cf2e1fec265f"]),
    (Material::Marble, ["d710ba4ba083ddefe2ccf4bfe050bd51", "29d6a963bb3546d072ef6a73f500c541", "74057b45610380b5d9aad250ef0965b5"]),
    (Material::Basalt, ["5b4cc8b10c2f452a9950a5ae8ac69c69", "b4734aa57869b52da8093d05ffcced65", "fad1510bd90589072e28a7b740c0645a"]),
    (Material::Slate, ["b4c851837a54c9c7a62e6626bbb66ed8", "70f6091bbb039000ae77bf260dc2bbf2", "abf760caf6e66d934b74447eb124e8f0"]),
    (Material::CrackedLava, ["4e6a59ccd7b8b250d6d10f9e8fe685ed", "e63f8de21e9a1b60a320d2c0389d6cff", "fe232a76ecc9032a0e9798384af7cd22"]),
    (Material::Concrete, ["42b886f90c75cf400f14ded496f613fb", "9c20e8d08f168b9e755171de3c54b907", "cab2f410f1d7f514b8fd902f9d68c19f"]),
    (Material::Limestone, ["f5ee741a9001b799ac10b339d009ac99", "84e19abd893d927ac61004164db256d8", "7bd687b8c62cbd5f7aeaf11e5119bfca"]),
    (Material::Granite, ["38ab970f0db416036c264c41b73a032f", "", "d95c0b778ae3142132ac96741052930b"]),
    (Material::Pavement, ["3ef20d7dca9658a86660940bdc2f8923", "e347d946a2c41a73859adf636db67023", "6578fc3ab9347a631f5be43a9f35c3ae"]),
    (Material::Brick, ["9f1225c686c56dc8676571bed439a8a1", "e32c3fe37c9e470c1b4df781edb61218", "e0ee95c7c4c2b7291848f642818fc5d4"]),
    (Material::Pebble, ["620b16e7b1d5e2c5f4ec4aac6a5f6723", "0c086d16a8ece14d5a0334cfc844b4b5", "27bb65db08f97b022f862721bdad908c"]),
    (Material::Cobblestone, ["182fedba833db3ea7934df4dcd063934", "c837e19c0066620e4c63cdbc9d43ae3a", "e82fac613f80e8f5445fde03d5436f6f"]),
    (Material::Rock, ["ce9a4ae3213c408d391dc85778e665f7", "f6a0c1c343aa0b75c7dfb4bd5c2974fd", "595b39dd8593a1e810c4d2ad6d376c69"]),
    (Material::Sandstone, ["2853daf82c206c4bf22a334975fb7fb6", "40180bc221190c666727967ee92ae33f", "4f3755f269dfe4aab5846216467b45db"]),
    (Material::CorrodedMetal, ["271761dc485bcb60ccdef46ccdce8722", "5ba2e5717b8c269a8c28e23ee9ec9f39", "e43abc975cdbcfb9fb5df5fcb18e02d9"]),
    (Material::DiamondPlate, ["97fe1eadf64a4bf22a985525e3b45d73", "e53eeca9953bcdac3518a7a05c613092", "4dd0194e00135bc61d37eaa16963f68a"]),
    (Material::Foil, ["d8b92321782b296dd6cbd11dcbb2ec32", "514722dbd44d770014448fea914bd737", "ef7e1b0b50f4b0163d83db83b63e3834"]),
    (Material::Metal, ["3ef91b52a1bbd40427a329bef72797e8", "eacdca07909930e41054af6bd25612aa", "7a24339d123b514c9f8ea47eb69ccbf9"]),
    (Material::Grass, ["b8a0f9601e2e7030971d41da92ba0c43", "325159d417faa2a95ba0ccb0ce900d34", "337db2890e8b47b57c9c1dba4a0bfd67"]),
    (Material::LeafyGrass, ["57baec2f76ab60d4d3df7e8721b98844", "3eabef43a6963b66606c02156f02e03e", "96aac2d04122eae87176e249b2d6c9a9"]),
    (Material::Sand, ["8acafa59782f47eed80a563255df5617", "e925667212c3b340888e90cf8013bb3d", "1284661ade3df24e97d90870bbcd266f"]),
    (Material::Fabric, ["e7ee2d50dc541a82e5488cd9e621e1f7", "500a180e67e5067ea97b4c5be3e11bef", "44cce0f52300fd1862f8957b8d97efe1"]),
    (Material::Snow, ["132de381f49de741f0ce22f8b908d56b", "cf18396e395812a18cb22baf5a2c8e87", "363796d25a2598d669ee391e1135701d"]),
    (Material::Mud, ["90277aef2ad02ee9c8ebc32b13eadac4", "ee7240b4525c38371a37bda7d62cd0a1", "518e293887b906d5112cf89fd59deac3"]),
    (Material::Ground, ["39de819b00c8cb32967632f195c10afb", "098f7ad7b3d8c38a11b49b685fdfea32", "22e3d41bd2b8f6fc79d307fa2b999dca"]),
    (Material::Asphalt, ["a1e49deb66ef214fcaa3f6aa156e1dcc", "530c41775ab450bdbdd96fe04be07d10", "01f402eb05abfdd971b9a3e1c0b1402f"]),
    (Material::Salt, ["64764ac199afa8057c20b6e82d3edc81", "99aea3a5c7c0847f50ebf16ba8e8d5ec", "ef4b97f847cca5db0a12a02c007678e6"]),
    (Material::Ice, ["2905665ba31a43653a7340467f7f7181", "7d5d7edc638d0197bb2658f044c7c86a", "9ce0fd5ed5282b407cf38d7b3021e96c"]),
    (Material::Glacier, ["36435e0dca3bebe69d380501ab6a9f04", "c7f090a3e82c793d143d29036d246c3f", "bf3464fcbb70404097fbb6bc2864b6a0"]),
    (Material::Glass, ["28562b34d6265769acc6602e430f2ec4", "bb0a43139011255ed1b4d62ae1ec11f9", "26181b46d0039be04dd6ce6c89ea3753"]),
];

/// The `contentHash` of `material`'s `map` in `set`, if Studio has one.
pub fn content_hash(
    material: Material,
    set: MaterialSet,
    map: MaterialMap,
) -> Option<&'static str> {
    let find = |table: &'static [(Material, [&'static str; 3])]| {
        table
            .iter()
            .find(|(m, _)| *m == material)
            .map(|(_, maps)| maps)
    };
    let maps = match set {
        MaterialSet::Materials2022 => find(MATERIALS_2022).or_else(|| find(LEGACY)),
        MaterialSet::Legacy => find(LEGACY),
    }?;
    Some(maps[map as usize]).filter(|h| !h.is_empty())
}

/// Every texture in both tables.
pub(super) fn all() -> impl Iterator<Item = &'static str> {
    LEGACY
        .iter()
        .chain(MATERIALS_2022)
        .flat_map(|(_, maps)| maps.iter().copied())
        .filter(|h| !h.is_empty())
}
