-- Random: what a seed gives, and how each method draws from it.
local function n(x)
    return string.format("%.14f", x)
end
local function v(x)
    return string.format("%.6f %.6f %.6f", x.X, x.Y, x.Z)
end

for _, seed in ipairs({ 0, 1, 7, -1, 1.5, 2 ^ 32 + 5 }) do
    local r = Random.new(seed)
    log("seed", seed, n(r:NextNumber()), n(r:NextNumber()), n(r:NextNumber()))
end
log("numeric string seed", n(Random.new("5"):NextNumber()) == n(Random.new(5):NextNumber()))

local r = Random.new(7)
log("range", n(r:NextNumber(3, 5)), n(r:NextNumber(5, 3)), n(r:NextNumber(-1, 1)))

for _, range in ipairs({
    { 1, 2 ^ 31 },
    { 0, 9 },
    { 9, 0 },
    { 5, 1 },
    { 0.9, 9.9 },
    { -3, 3 },
    { 0, 2 ^ 32 - 1 },
    { 0, 2 ^ 33 },
}) do
    local r = Random.new(7)
    log("integers", range[1], range[2], r:NextInteger(range[1], range[2]), r:NextInteger(range[1], range[2]))
end
do
    local r, same = Random.new(7), Random.new(7)
    r:NextInteger(0, 0)
    same:NextNumber()
    log("one value still draws", n(r:NextNumber()) ~= n(Random.new(7):NextNumber()))
end

for _, seed in ipairs({ 7, 8, 9 }) do
    local r = Random.new(seed)
    log("unit", seed, v(r:NextUnitVector()), v(r:NextUnitVector()))
end

do
    local t = {}
    for i = 1, 8 do
        t[i] = i
    end
    Random.new(7):Shuffle(t)
    log("shuffle", table.concat(t, " "))
end
log("shuffle a dictionary", pcall(function()
    local t = { a = 1 }
    Random.new(1):Shuffle(t)
    return t.a
end))
log("shuffle with a hole", pcall(function()
    Random.new(1):Shuffle({ 1, nil, 3 })
end))

do
    local a, b = Random.new(1), Random.new(1)
    log("equal", a == b, a == Random.new(2), tostring(a), typeof(a))
    a:NextNumber()
    local c = a:Clone()
    log("after a draw", a == b, a == c, n(a:NextNumber()) == n(c:NextNumber()))
end
log("bad seed", pcall(Random.new, "x"))
