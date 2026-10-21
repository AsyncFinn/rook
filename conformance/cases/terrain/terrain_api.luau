-- The Terrain API: what each fill leaves in the voxels (as SmoothGrid
-- bytes), how fills merge, region rounding and limits, cells, legacy cell
-- methods, copies and material colors, and rays finding a fill at once.
-- Everything happens in a corner of Workspace's Terrain far from the
-- place, which is cleared again at the end.
local T = workspace.Terrain
local O = Vector3.new(4000, 400, 4000) -- voxel (1000, 100, 1000)
local area = Region3.new(O - Vector3.one * 32, O + Vector3.one * 32)

local function clear()
	T:FillRegion(area, 4, Enum.Material.Air)
end

-- Every voxel that isn't air in the area, as x,y,z:material:byte relative
-- to O's voxel.
local function dump(name)
	local m, o = T:ReadVoxels(area, 4)
	local s = {}
	for x = 1, m.Size.X do
		for y = 1, m.Size.Y do
			for z = 1, m.Size.Z do
				if m[x][y][z] ~= Enum.Material.Air then
					s[#s + 1] = string.format("%d,%d,%d:%s:%d", x - 9, y - 9, z - 9, m[x][y][z].Name, math.round(o[x][y][z] * 256) - 1)
				end
			end
		end
	end
	log(name, table.concat(s, " "))
	clear()
end

local C = O + Vector3.one * 2 -- voxel 0's center
local function at(x, y, z)
	return CFrame.new(C + Vector3.new(x, y, z))
end

clear()
T:FillBlock(at(0, 0, 0), Vector3.one * 4, Enum.Material.Grass)
dump("block aligned")
T:FillBlock(at(-2, -2, -2), Vector3.one * 4, Enum.Material.Grass)
dump("block straddling")
T:FillBlock(at(0, 0.5, 0), Vector3.new(4, 4, 4), Enum.Material.Grass)
dump("block shifted")
T:FillBlock(at(0, 0, 0) * CFrame.Angles(0, math.rad(45), 0), Vector3.new(8, 4, 8), Enum.Material.Grass)
dump("block turned")
T:FillBlock(at(0, 0, 0), Vector3.one * 4, Enum.Material.Plastic)
dump("block of Plastic")
T:FillBall(C, 0.5, Enum.Material.Grass)
dump("ball r0.5")
T:FillBall(C, 4, Enum.Material.Grass)
dump("ball r4")
T:FillCylinder(at(0, 0, 0), 4, 6, Enum.Material.Grass)
dump("cylinder r6 h4")
T:FillCylinder(at(0, 0, 0), 8, 4, Enum.Material.Grass)
dump("cylinder r4 h8")
T:FillWedge(at(0, 0, 0), Vector3.new(4, 8, 16), Enum.Material.Grass)
dump("wedge 4x8x16")
T:FillWedge(at(0, 0, 0), Vector3.new(8, 4, 8), Enum.Material.Grass)
dump("wedge 8x4x8")

-- Merging.
local function merge(name, a, sa, b, sb)
	T:FillBlock(at(0, 0, 0), Vector3.one * sa, a)
	T:FillBlock(at(0, 0, 0), Vector3.one * sb, b)
	dump(name)
end
merge("rock half over grass", Enum.Material.Grass, 4, Enum.Material.Rock, 2)
merge("rock over half grass", Enum.Material.Grass, 2, Enum.Material.Rock, 3)
merge("rock equal to grass", Enum.Material.Grass, 2, Enum.Material.Rock, 2)
merge("water over half grass", Enum.Material.Grass, 2, Enum.Material.Water, 4)
merge("rock half in water", Enum.Material.Water, 4, Enum.Material.Rock, 2)
merge("air half on grass", Enum.Material.Grass, 4, Enum.Material.Air, 2)

-- Regions round toward zero, ReplaceMaterial, and the limits.
local function region(a, b)
	return Region3.new(O + a, O + b)
end
T:FillRegion(region(Vector3.new(1, 0, 0), Vector3.new(8, 4, 4)), 4, Enum.Material.Rock)
dump("fill region 1..8")
T:FillBlock(at(0, 0, 0), Vector3.new(2, 4, 4), Enum.Material.Grass)
T:ReplaceMaterial(region(Vector3.zero, Vector3.one * 4), 4, Enum.Material.Grass, Enum.Material.Water)
dump("replace half grass with water")
log("read size", T:ReadVoxels(region(Vector3.new(-1.9, 0, 0), Vector3.new(2.1, 4, 4)), 4).Size)
log("resolution 2", pcall(T.ReadVoxels, T, area, 2))
log("too large", pcall(T.ReadVoxels, T, Region3.new(Vector3.zero, Vector3.new(1024, 1024, 260)), 4))
log("empty block", pcall(T.FillBlock, T, at(0, 0, 0), Vector3.new(-4, 4, 4), Enum.Material.Rock))

-- WriteVoxels.
local one = region(Vector3.zero, Vector3.one * 4)
for _, occ in { 0.5, 0, 2 } do
	T:WriteVoxels(one, 4, { { { Enum.Material.Rock } } }, { { { occ } } })
	local m, o = T:ReadVoxels(one, 4)
	log("write", occ, m[1][1][1], o[1][1][1])
end
log("write wrong size", pcall(T.WriteVoxels, T, region(Vector3.zero, Vector3.new(8, 4, 4)), 4, { { { Enum.Material.Rock } } }, { { { 1 } } }))
clear()

-- Cells.
local cell = O / 4
log("WorldToCell", T:WorldToCell(O + Vector3.new(5, -1, 9.9)) - cell)
log("CellCenterToWorld", T:CellCenterToWorld(1, 2, 3), T:CellCenterToWorld(-1.5, 2.5, 0.4))
log("CellCornerToWorld", T:CellCornerToWorld(1, 2, 3))
T:FillBlock(at(0, 0, 0), Vector3.one * 4, Enum.Material.Rock)
log("PreferSolid", T:WorldToCellPreferSolid(O + Vector3.new(4, 2, 2)) - cell)
log("PreferEmpty", T:WorldToCellPreferEmpty(O + Vector3.new(4, 2, 2)) - cell)
clear()

-- Legacy cells.
local x, y, z = cell.X, cell.Y, cell.Z
T:SetCell(x, y, z, Enum.CellMaterial.MossyStone, Enum.CellBlock.CornerWedge, Enum.CellOrientation.X)
log("GetCell", T:GetCell(x, y, z))
dump("SetCell MossyStone corner wedge")
T:FillBlock(at(0, 0, 0), Vector3.one * 4, Enum.Material.Basalt)
log("GetCell Basalt", (T:GetCell(x, y, z)))
log("GetWaterCell", T:GetWaterCell(x, y, z))
T:SetWaterCell(x, y, z, Enum.WaterForce.Max, Enum.WaterDirection.Y)
log("GetWaterCell after", T:GetWaterCell(x, y, z))
log("AutowedgeCell", T:AutowedgeCell(x, y, z))
clear()

-- Copies.
T:FillBlock(at(0, 0, 0), Vector3.one * 4, Enum.Material.Grass)
T:FillBlock(at(4, 0, 0), Vector3.new(2, 4, 4), Enum.Material.Rock)
local copy = T:CopyRegion(Region3int16.new(Vector3int16.new(x, y, z), Vector3int16.new(x + 2, y, z)))
log("copy", copy.ClassName, copy.SizeInCells, copy.IsSmooth, copy.Parent)
T:FillBlock(at(-8, 0, 0), Vector3.one * 4, Enum.Material.Sand)
T:PasteRegion(copy, Vector3int16.new(x - 4, y, z), false)
dump("pasted keeping")
log("inverted copy", pcall(T.CopyRegion, T, Region3int16.new(Vector3int16.new(1, 0, 0), Vector3int16.new(0, 0, 0))))
copy:Destroy()

-- Material colors.
local before = T:GetMaterialColor(Enum.Material.Rock)
T:SetMaterialColor(Enum.Material.Rock, Color3.new(1, 0, 0))
log("color", T:GetMaterialColor(Enum.Material.Rock))
T:SetMaterialColor(Enum.Material.Rock, before)
log("color of Water", pcall(T.GetMaterialColor, T, Enum.Material.Water))
log("color of Plastic", pcall(T.GetMaterialColor, T, Enum.Material.Plastic))

-- Collision catches up with edits a frame or two later.
local down = Vector3.new(0, -40, 0)
local function ray(name)
	local hit = workspace:Raycast(C + Vector3.new(0, 20, 0), down)
	log(name, hit and hit.Instance == T, hit and hit.Material, hit and math.round((hit.Position.Y - O.Y) * 10) / 10)
end
T:FillBlock(at(0, 0, 0), Vector3.new(16, 4, 16), Enum.Material.Grass)
ray("ray at once")
for _ = 1, 5 do
	task.wait()
end
ray("ray later")
T:FillBall(C, 8, Enum.Material.Air)
ray("ray at once after carving")
for _ = 1, 5 do
	task.wait()
end
ray("ray later after carving")
clear()
