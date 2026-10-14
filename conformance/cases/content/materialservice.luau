-- MaterialService's overrides and variant lookup, and the defaults of the
-- material classes (M4 step 5). Everything changed is put back. Members
-- needing plugin capability (MaterialVariant.BaseMaterial's setter) are left
-- out: the recorder runs with it, game scripts don't.
local ms = game:GetService("MaterialService")
log("Plastic", ms:GetBaseMaterialOverride(Enum.Material.Plastic))
log("Neon", pcall(function()
	return ms:GetBaseMaterialOverride(Enum.Material.Neon)
end))
log("set Neon", pcall(function()
	ms:SetBaseMaterialOverride(Enum.Material.Neon, "X")
end))
log("by name", pcall(function()
	return ms:GetBaseMaterialOverride("Wood")
end))
local before = ms:GetBaseMaterialOverride(Enum.Material.Wood)
ms:SetBaseMaterialOverride(Enum.Material.Wood, "NoSuchVariant")
log("Wood", ms:GetBaseMaterialOverride(Enum.Material.Wood))
ms:SetBaseMaterialOverride(Enum.Material.Wood, "")
log("Wood empty", string.format("%q", ms:GetBaseMaterialOverride(Enum.Material.Wood)))
ms:SetBaseMaterialOverride(Enum.Material.Wood, before)

local folder = Instance.new("Folder")
local v = Instance.new("MaterialVariant")
v.Name = "RookCaseVariant"
v.Parent = folder
log("defaults", v.BaseMaterial, v.StudsPerTile, v.MaterialPattern)
folder.Parent = ms
log("found", ms:GetMaterialVariant(Enum.Material.Plastic, "RookCaseVariant"))
log("other base", ms:GetMaterialVariant(Enum.Material.Brick, "RookCaseVariant"))
log("missing", ms:GetMaterialVariant(Enum.Material.Plastic, "Nope"))
folder:Destroy()
log("after destroy", ms:GetMaterialVariant(Enum.Material.Plastic, "RookCaseVariant"))

local sa = Instance.new("SurfaceAppearance")
log("SurfaceAppearance", sa.AlphaMode, sa.Color)
sa:Destroy()
local p = Instance.new("Part")
log("Part.MaterialVariant", string.format("%q", p.MaterialVariant))
p.MaterialVariant = "Anything"
log("set", p.MaterialVariant)
p:Destroy()
