//! The Terrain API through scripts: edits are seen at once by queries and
//! reads, and reach `SmoothGrid` at the end of the frame.

use super::*;

fn smooth_grid(s: &Server) -> rook_terrain::Grid {
    let w = s.runtime().world();
    let ws = w.dom.find_service("Workspace").unwrap();
    let terrain = w.dom.find_first_child_of_class(ws, "Terrain").unwrap();
    match w.dom.property_by_name(terrain, "SmoothGrid") {
        Some(Value::BinaryString(b)) => rook_terrain::Grid::decode(b.as_ref()).unwrap(),
        _ => rook_terrain::Grid::new(),
    }
}

/// Reads see an edit at once; collision and `SmoothGrid` at the end of
/// the frame (Studio's rays find a fill 2 frames later).
#[test]
fn edits_collide_from_the_next_frame_and_reach_smooth_grid() {
    let mut s = Server::new(&Options::default()).unwrap();
    s.start();
    let run = |s: &mut Server, source: &str| {
        let prelude = "local T = workspace.Terrain
            local function ray() return workspace:Raycast(Vector3.new(2, 60, 2), Vector3.new(0, -100, 0)) end
";
        s.run_extra_script(&format!("{prelude}{source}")).unwrap();
        lines(s)
    };
    let filled = run(
        &mut s,
        r#"
        T:FillBlock(CFrame.new(0, 42, 0), Vector3.new(64, 4, 64), Enum.Material.Grass)
        print(ray())
        local m, o = T:ReadVoxels(Region3.new(Vector3.new(0, 40, 0), Vector3.new(8, 44, 4)), 4)
        print(m.Size, m[1][1][1], o[2][1][1])
        print(T:GetCell(5, 10, 5))
        print(pcall(T.FillBlock, T, CFrame.new(), Vector3.new(-1, 1, 1), Enum.Material.Rock))
        print(pcall(T.ReadVoxels, T, Region3.new(Vector3.zero, Vector3.one * 4), 2))
        "#,
    );
    assert_eq!(
        filled,
        [
            "nil",
            "2, 1, 1 Enum.Material.Grass 1",
            "Enum.CellMaterial.Grass Enum.CellBlock.Solid Enum.CellOrientation.NegZ",
            "false Extents cannot be empty",
            "false Resolution has to be 4",
        ]
    );
    assert!(smooth_grid(&s).is_empty());
    s.step();
    assert_eq!(
        smooth_grid(&s).voxel(glam::IVec3::new(5, 10, 5)).material,
        rook_terrain::TerrainMaterial::Grass
    );
    let carved = run(
        &mut s,
        r#"
        local hit = ray()
        print(hit.Instance == T, hit.Material, hit.Position.Y > 43)
        T:FillBall(Vector3.new(2, 42, 2), 6, Enum.Material.Air)
        print(ray() ~= nil)
        "#,
    );
    assert_eq!(carved, ["true Enum.Material.Grass true", "true"]);
    s.step();
    assert_eq!(run(&mut s, "print(ray())"), ["nil"]);
    assert_eq!(
        smooth_grid(&s).voxel(glam::IVec3::new(0, 10, 0)),
        rook_terrain::Voxel::AIR
    );
}

#[test]
fn regions_and_colors() {
    let mut s = Server::new(&Options::default()).unwrap();
    s.start();
    s.run_extra_script(
        r#"
        local T = workspace.Terrain
        T:WriteVoxels(Region3.new(Vector3.zero, Vector3.new(8, 4, 4)), 4,
            {{{Enum.Material.Rock}}, {{Enum.Material.Sand}}}, {{{0.5}}, {{1}}})
        local r = T:CopyRegion(Region3int16.new(Vector3int16.new(0, 0, 0), Vector3int16.new(2, 0, 0)))
        print(r.ClassName, r.SizeInCells, r.IsSmooth)
        T:PasteRegion(r, Vector3int16.new(10, 0, 0), true)
        local m, o = T:ReadVoxels(Region3.new(Vector3.new(40, 0, 0), Vector3.new(52, 4, 4)), 4)
        print(m[1][1][1], o[1][1][1], m[2][1][1], m[3][1][1])
        print(pcall(T.WriteVoxels, T, Region3.new(Vector3.zero, Vector3.new(8, 4, 4)), 4, {{{Enum.Material.Rock}}}, {{{1}}}))
        print(pcall(T.CopyRegion, T, Region3int16.new(Vector3int16.new(1, 0, 0), Vector3int16.new(0, 0, 0))))
        T:SetMaterialColor(Enum.Material.Rock, Color3.new(1, 0, 0))
        print(T:GetMaterialColor(Enum.Material.Rock))
        print(pcall(T.GetMaterialColor, T, Enum.Material.Water))
        print(pcall(T.GetMaterialColor, T, Enum.Material.Plastic))
        print(T:CellCenterToWorld(1, 2, 3), T:WorldToCell(Vector3.new(5, -1, 9.9)))
        "#,
    )
    .unwrap();
    assert_eq!(
        lines(&s),
        [
            "TerrainRegion 3, 1, 1 true",
            "Enum.Material.Rock 0.5 Enum.Material.Sand Enum.Material.Air",
            "false Bad argument materials to 'WriteVoxels' (2x1x1 array expected)",
            "false The region can not be empty or inverted.",
            "1, 0, 0",
            "false Unsupported terrain material",
            "false Invalid terrain material",
            "6, 10, 14 1, -1, 2",
        ]
    );
}
