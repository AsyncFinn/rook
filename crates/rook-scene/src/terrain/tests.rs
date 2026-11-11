use rook_types::Value;

use super::*;

/// A script changing the waves or the wind reaches the scene's terrain
/// look without rebuilding the scene.
#[test]
fn script_changes_update_the_terrain_look() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, Floors::default(), content, glam::Vec3::ZERO);
    assert_eq!(cache.scene.terrain.wave_size, 0.15);
    dom.set_property_by_name(terrain, "WaterWaveSize", Value::Float32(0.6))
        .unwrap();
    dom.set_property_by_name(
        ws,
        "GlobalWind",
        Value::Vector3(rook_types::Vector3::new(5.0, 0.0, 0.0)),
    )
    .unwrap();
    let changes = RenderChanges {
        terrain: true,
        ..RenderChanges::default()
    };
    assert_eq!(
        cache.apply(&dom, &changes),
        SceneChange::Parts {
            changed: Vec::new(),
            moved: Vec::new(),
            lighting: true,
            highlights: false,
        }
    );
    assert_eq!(cache.scene.terrain.wave_size, 0.6);
    assert_eq!(cache.scene.terrain.wind, Vec3::new(5.0, 0.0, 0.0));
}

/// An edit meshes again only the chunks it changed (and their
/// neighbors) and swaps them in the scene without rebuilding it; the far
/// chunk keeps its meshes, and the edited chunk's old ones leave the
/// scene. The drawn voxels and material counts follow the edit.
#[test]
fn edits_remesh_only_changed_chunks() {
    use rook_terrain::Voxel;
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    let mut grid = Grid::new();
    let (near, far) = (IVec3::new(5, 5, 5), IVec3::new(200, 5, 5));
    grid.set(near, Voxel::new(TerrainMaterial::Grass, 255));
    grid.set(far, Voxel::new(TerrainMaterial::Rock, 255));
    let write = |dom: &mut Dom, grid: &Grid| {
        dom.set_property_by_name(
            terrain,
            "SmoothGrid",
            Value::BinaryString(grid.encode().into()),
        )
        .unwrap();
    };
    write(&mut dom, &grid);
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, Floors::default(), content, glam::Vec3::ZERO);
    let meshes_of = |cache: &SceneCache, material| -> Vec<MeshHash> {
        cache
            .terrain
            .surfaces()
            .filter(|s| s.layer.material == material)
            .map(|s| s.mesh)
            .collect()
    };
    let (rock, grass) = (
        meshes_of(&cache, TerrainMaterial::Rock),
        meshes_of(&cache, TerrainMaterial::Grass),
    );
    assert!(!rock.is_empty() && !grass.is_empty());
    // An edit in place, journaled as its region alone.
    let at = near + IVec3::X;
    let bounds = Bounds { min: at, max: at };
    dom.edit_voxels(terrain, |g| {
        g.set(at, Voxel::new(TerrainMaterial::Grass, 255));
        ((), Some(bounds))
    })
    .unwrap();
    let changes = RenderChanges {
        terrain_edits: vec![(terrain, bounds)],
        ..RenderChanges::default()
    };
    let parts = cache.scene.parts.len();
    // The rest of the scene stays: a crater costs what it reaches.
    let SceneChange::Parts { changed, .. } = cache.apply(&dom, &changes) else {
        panic!("a voxel edit rebuilt the scene");
    };
    assert_eq!(meshes_of(&cache, TerrainMaterial::Rock), rock);
    let regrown = meshes_of(&cache, TerrainMaterial::Grass);
    assert!(!regrown.is_empty() && regrown.iter().all(|m| !grass.contains(m)));
    assert!(grass.iter().all(|m| !cache.scene.meshes.contains_key(m)));
    // The new chunk is drawn, in the entries the old one freed; the far
    // chunk's entries are left alone.
    for chunk in cache.terrain.chunks.values() {
        assert_eq!(chunk.entries.len(), chunk.surfaces.len());
        if chunk.surfaces.is_empty() {
            continue;
        }
        let regrew = chunk.surfaces.iter().any(|s| regrown.contains(&s.mesh));
        assert_eq!(
            chunk.entries.iter().all(|i| changed.contains(i)),
            regrew,
            "{:?}",
            chunk
                .surfaces
                .iter()
                .map(|s| s.layer.material)
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(cache.scene.parts.len(), parts);
    assert_eq!(cache.terrain.levels[0], *dom.voxels(terrain).unwrap());
    assert_eq!(cache.terrain.counts, dom.voxels(terrain).unwrap().counts());
    // Carving the Rock away takes it from the materials read.
    let bounds = Bounds { min: far, max: far };
    dom.edit_voxels(terrain, |g| {
        g.set(far, Voxel::AIR);
        ((), Some(bounds))
    })
    .unwrap();
    let changes = RenderChanges {
        terrain_edits: vec![(terrain, bounds)],
        ..RenderChanges::default()
    };
    cache.apply(&dom, &changes);
    assert!(meshes_of(&cache, TerrainMaterial::Rock).is_empty());
    assert_eq!(cache.terrain.materials, [TerrainMaterial::Grass]);
}

/// A whole new `SmoothGrid` is meshed again where it differs.
#[test]
fn whole_writes_remesh() {
    use rook_terrain::Voxel;
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, Floors::default(), content, glam::Vec3::ZERO);
    assert!(cache.terrain.surfaces().next().is_none());
    let mut grid = Grid::new();
    grid.set(IVec3::new(5, 5, 5), Voxel::new(TerrainMaterial::Sand, 255));
    dom.set_property_by_name(
        terrain,
        "SmoothGrid",
        Value::BinaryString(grid.encode().into()),
    )
    .unwrap();
    let changes = RenderChanges {
        terrain: true,
        terrain_voxels: true,
        ..RenderChanges::default()
    };
    cache.apply(&dom, &changes);
    assert!(
        cache
            .terrain
            .surfaces()
            .any(|s| s.layer.material == TerrainMaterial::Sand)
    );
}

/// Flying along a long strip, the rings follow: chunks the camera leaves
/// go coarser and those it nears finer, meshed off the frame, while every
/// spot of the strip stays drawn by exactly one level throughout.
#[test]
fn rings_follow_the_camera() {
    use rook_terrain::Voxel;
    use rook_terrain::lod::chunk_studs;
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    let mut grid = Grid::new();
    for x in 0..1024 {
        for z in 0..4 {
            grid.set(IVec3::new(x, 0, z), Voxel::new(TerrainMaterial::Sand, 255));
        }
    }
    dom.set_property_by_name(
        terrain,
        "SmoothGrid",
        Value::BinaryString(grid.encode().into()),
    )
    .unwrap();
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, Floors::default(), content, Vec3::new(8.0, 10.0, 8.0));
    let level_at = |cache: &SceneCache, x: f32| -> Vec<u8> {
        let p = Vec3::new(x, 2.0, 8.0);
        cache
            .terrain
            .chunks
            .keys()
            .filter(|(l, c)| (p / chunk_studs(*l)).floor().as_ivec3() == *c)
            .map(|(l, _)| *l)
            .collect()
    };
    assert_eq!(level_at(&cache, 10.0), [0]);
    assert!(level_at(&cache, 3000.0)[0] >= 3);
    let mut camera = Vec3::new(8.0, 10.0, 8.0);
    for _ in 0..400 {
        camera.x += 8.0;
        cache.update_terrain(camera, false);
        for x in (2..4096).step_by(64) {
            assert_eq!(
                level_at(&cache, x as f32).len(),
                1,
                "{x} with the camera at {camera}"
            );
        }
    }
    cache.update_terrain(camera, true);
    assert_eq!(level_at(&cache, camera.x), [0]);
    assert!(level_at(&cache, 10.0)[0] >= 3);
}

/// As the rings move over Grass, the blades of the tiles whose ground
/// changed level grow again in new meshes: a mesh handed to the renderer
/// to free is never one the scene still draws. Blades grown again under
/// their old key were placed on the old mesh and then lost it, so those
/// tiles drew nothing until the camera left them.
#[test]
fn moving_rings_never_free_a_drawn_mesh() {
    use rook_terrain::Voxel;
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    dom.set_property_by_name(terrain, "Decoration", Value::Bool(true))
        .unwrap();
    let mut grid = Grid::new();
    for x in 0..512 {
        for z in -32..32 {
            grid.set(IVec3::new(x, 0, z), Voxel::new(TerrainMaterial::Grass, 255));
        }
    }
    dom.set_property_by_name(
        terrain,
        "SmoothGrid",
        Value::BinaryString(grid.encode().into()),
    )
    .unwrap();
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut camera = Vec3::new(8.0, 10.0, 8.0);
    let mut cache = SceneCache::new(&dom, Floors::default(), content, camera);
    cache.update_grass(camera);
    let mut regrown = 0;
    for _ in 0..200 {
        camera.x += 8.0;
        cache.update_terrain(camera, true);
        let blades = cache.terrain.grass.entries().len();
        cache.update_grass(camera);
        regrown += cache.terrain.grass.entries().len().saturating_sub(blades);
        for mesh in cache.take_dropped_meshes() {
            assert!(
                !cache.scene.meshes.contains_key(&mesh),
                "a drawn mesh is freed, the camera at {camera}"
            );
        }
    }
    assert!(regrown > 0, "no blades grew again");
}

/// A server script's edits reach a client region by region, and the
/// client's voxels, collision and drawn chunks end up as the server's;
/// a client joining afterwards, taking the voxels whole, has them too.
#[test]
fn server_edits_reach_clients() {
    use rook_engine::session::Session;
    use rook_engine::{DataModel, Options, Server};
    let mut server = Server::new(&Options::default()).unwrap();
    server.start();
    let floor = "workspace.Terrain:FillBlock(CFrame.new(0, -8, 0), \
        Vector3.new(512, 16, 512), Enum.Material.Rock)";
    server.run_extra_script(floor).unwrap();
    server.step();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    session.step();
    let camera = Vec3::new(0.0, 30.0, 0.0);
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let client = session.client_mut(0).unwrap();
    client.take_render_changes();
    let mut drawn = SceneCache::new(
        &client.runtime().world().dom,
        Floors::default(),
        content.clone(),
        camera,
    );
    // Craters and mounds, across chunk edges (a chunk is 128 studs), over
    // a few frames, one undoing another.
    let edits = [
        "T:FillBall(Vector3.new(0, 0, 0), 10, Enum.Material.Air)",
        "T:FillBall(Vector3.new(128, 0, 0), 12, Enum.Material.Grass)",
        "T:FillBlock(CFrame.new(-60, 2, 70), Vector3.new(30, 8, 30), Enum.Material.Sand)
         T:FillBall(Vector3.new(0, 0, 0), 10, Enum.Material.Rock)",
        "T:FillBall(Vector3.new(-128, -4, -128), 20, Enum.Material.Air)",
    ];
    for edit in edits {
        let script = format!("local T = workspace.Terrain {edit}");
        session.server().run_extra_script(&script).unwrap();
        session.step();
        let client = session.client_mut(0).unwrap();
        let changes = client.take_render_changes();
        drawn.apply(&client.runtime().world().dom, &changes);
    }
    let voxels = |dm: &DataModel| {
        let w = dm.runtime().world();
        let ws = w.dom.find_service("Workspace").unwrap();
        let terrain = w.dom.find_first_child_of_class(ws, "Terrain").unwrap();
        w.dom.voxels(terrain).cloned().unwrap_or_default()
    };
    let server_voxels = voxels(session.server());
    assert!(server_voxels.voxel(IVec3::ZERO).material == TerrainMaterial::Rock);
    let differing = |dm: &DataModel| server_voxels.changes(&voxels(dm));
    assert_eq!(differing(session.client(0).unwrap()), []);
    // Rays over the edits hit the same surface.
    let hits = |dm: &DataModel| -> Vec<Option<(Vec3, u32)>> {
        let mut w = dm.runtime().world_mut();
        let w = &mut *w;
        let filter = rook_physics::Filter::default();
        let mut out = Vec::new();
        for x in (-200..=200).step_by(8) {
            for z in [-128, -64, 0, 70] {
                let origin = Vec3::new(x as f32 + 0.5, 60.0, z as f32 + 0.5);
                let hit = w
                    .physics
                    .raycast(&mut w.dom, origin, Vec3::NEG_Y * 200.0, &filter);
                out.push(hit.map(|h| ((h.position * 100.0).round(), h.material)));
            }
        }
        out
    };
    assert_eq!(hits(session.client(0).unwrap()), hits(session.server()));
    // The chunks drawn are those a scene made from the server's voxels
    // draws.
    let surfaces = |cache: &SceneCache| {
        let mut out: Vec<String> = cache
            .terrain
            .chunks
            .iter()
            .flat_map(|(key, chunk)| {
                chunk.surfaces.iter().map(move |s| {
                    let mesh = &cache.scene.meshes[&s.mesh];
                    format!("{key:?} {:?} {:?} {:?}", s.layer, s.seam, mesh.positions)
                })
            })
            .collect();
        out.sort_unstable();
        out
    };
    let fresh = SceneCache::new(
        &session.server().runtime().world().dom,
        Floors::default(),
        content,
        camera,
    );
    assert!(surfaces(&fresh) == surfaces(&drawn), "drawn chunks differ");
    assert_eq!(drawn.terrain.counts, server_voxels.counts());
    session.join("B", 2).unwrap();
    session.step();
    assert_eq!(differing(session.client(1).unwrap()), []);
}

/// Workspace's Terrain replaced by another: the old one's chunks leave
/// the scene and the new one's are drawn in place, the rest of the scene
/// untouched.
#[test]
fn a_replaced_terrain_swaps_its_chunks() {
    use rook_terrain::Voxel;
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain_with = |dom: &mut Dom, material| {
        let terrain = dom.create_by_name("Terrain").unwrap();
        let mut grid = Grid::new();
        grid.set(IVec3::new(5, 5, 5), Voxel::new(material, 255));
        dom.set_property_by_name(
            terrain,
            "SmoothGrid",
            Value::BinaryString(grid.encode().into()),
        )
        .unwrap();
        terrain
    };
    let old = terrain_with(&mut dom, TerrainMaterial::Rock);
    dom.set_parent(old, Some(ws)).unwrap();
    let part = dom.create_by_name("Part").unwrap();
    dom.set_parent(part, Some(ws)).unwrap();
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, Floors::default(), content, glam::Vec3::ZERO);
    let drawn = |cache: &SceneCache, material| {
        cache
            .terrain
            .surfaces()
            .any(|s| s.layer.material == material)
    };
    assert!(drawn(&cache, TerrainMaterial::Rock));
    let part_entry = cache.index[&part];
    let rock: Vec<usize> = cache
        .terrain
        .chunks
        .values()
        .flat_map(|c| c.entries.clone())
        .collect();
    dom.set_parent(old, None).unwrap();
    let new = terrain_with(&mut dom, TerrainMaterial::Sand);
    dom.set_parent(new, Some(ws)).unwrap();
    let changes = RenderChanges {
        terrain: true,
        ..RenderChanges::default()
    };
    let SceneChange::Parts { changed, .. } = cache.apply(&dom, &changes) else {
        panic!("a replaced Terrain rebuilt the scene");
    };
    assert!(!drawn(&cache, TerrainMaterial::Rock) && drawn(&cache, TerrainMaterial::Sand));
    assert!(rock.iter().all(|i| changed.contains(i)));
    assert!(!changed.contains(&part_entry));
    for chunk in cache.terrain.chunks.values() {
        assert_eq!(chunk.entries.len(), chunk.surfaces.len());
    }
}

/// An edit gathers the Grass tops of the tiles it reached again, from
/// those tiles' chunks alone: the other tiles' tops are kept as they
/// were, not gathered again from every chunk drawn.
#[test]
fn edits_gather_grass_tops_only_for_their_tiles() {
    use rook_terrain::Voxel;
    use rook_terrain::grass::GrassTops;
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let terrain = dom.create_by_name("Terrain").unwrap();
    dom.set_parent(terrain, Some(ws)).unwrap();
    let mut grid = Grid::new();
    // Two lawns in chunks and tiles of their own, both near the camera.
    let (near, far) = (IVec3::new(4, 4, 4), IVec3::new(44, 4, 4));
    for at in [near, far] {
        for x in 0..3 {
            for z in 0..3 {
                grid.set(
                    at + IVec3::new(x, 0, z),
                    Voxel::new(TerrainMaterial::Grass, 255),
                );
            }
        }
    }
    dom.set_property_by_name(
        terrain,
        "SmoothGrid",
        Value::BinaryString(grid.encode().into()),
    )
    .unwrap();
    let content = Arc::new(Resolver::new(rook_content::Config::empty()));
    let mut cache = SceneCache::new(&dom, Floors::default(), content, glam::Vec3::ZERO);
    let tile = |v: IVec3| rook_terrain::grass::tile_of(v.as_vec3() * 4.0);
    let (near_tile, far_tile) = (tile(near), tile(far));
    assert_ne!(near_tile, far_tile);
    let far_tops = cache.terrain.grass.tops[&far_tile].as_ptr();
    assert!(cache.terrain.grass.tops.contains_key(&near_tile));
    // Raise the near lawn a voxel.
    let at = near + IVec3::Y;
    let bounds = Bounds {
        min: at,
        max: at + IVec3::new(2, 0, 2),
    };
    dom.edit_voxels(terrain, |g| {
        for x in 0..3 {
            for z in 0..3 {
                g.set(
                    at + IVec3::new(x, 0, z),
                    Voxel::new(TerrainMaterial::Grass, 255),
                );
            }
        }
        ((), Some(bounds))
    })
    .unwrap();
    let changes = RenderChanges {
        terrain_edits: vec![(terrain, bounds)],
        ..RenderChanges::default()
    };
    let SceneChange::Parts { .. } = cache.apply(&dom, &changes) else {
        panic!("a voxel edit rebuilt the scene");
    };
    // The far tile's tops weren't gathered again...
    assert_eq!(cache.terrain.grass.tops[&far_tile].as_ptr(), far_tops);
    // ...and every tile's tops are what gathering them all gives.
    let mut chunks: Vec<&LevelChunk> = cache.terrain.tops.keys().collect();
    chunks.sort_unstable_by_key(|c| chunk_order(c));
    let mut whole = GrassTops::default();
    for c in chunks {
        for (tile, tris) in &cache.terrain.tops[c] {
            whole.entry(*tile).or_default().extend(tris);
        }
    }
    assert_eq!(cache.terrain.grass.tops, whole);
}
