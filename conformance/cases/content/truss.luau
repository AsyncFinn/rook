-- What a TrussPart's Size becomes when a script sets it: at most one axis
-- other than 2, clamped to 512 (ROADMAP M4 "Trusses").
local t = Instance.new("TrussPart")
log("new", t.Size, t.Style, t.Material, t.BrickColor)
for _, size in {
    Vector3.new(2, 5, 2),
    Vector3.new(3, 3, 3),
    Vector3.new(2, 1, 2),
    Vector3.new(10, 2, 2),
    Vector3.new(6, 2, 10),
    Vector3.new(2.9, 7, 2),
    Vector3.new(3.1, 2, 2),
    Vector3.new(1, 1, 2),
    Vector3.new(1, 1, 1),
    Vector3.new(1, 2, 2),
    Vector3.new(1, 2, 3),
    Vector3.new(0, 0, 0),
    Vector3.new(-4, 2, 2),
    Vector3.new(2, 3000, 2),
    Vector3.new(600, 600, 2),
} do
    t.Size = size
    log(size, "->", t.Size)
end
-- The Size before doesn't matter.
t.Size = Vector3.new(2, 10, 2)
t.Size = Vector3.new(10, 10, 2)
log("after (2, 10, 2)", t.Size)
-- Other parts keep any size.
local p = Instance.new("Part")
p.Size = Vector3.new(3, 3, 3)
log("part", p.Size)
p:Destroy()
t:Destroy()
