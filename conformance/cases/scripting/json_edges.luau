-- HttpService's JSON and string helpers at their edges
-- (docs/behavior/scripting/json.md): how JSONEncode writes numbers,
-- strings, values JSON has no form for, cyclic and deep tables, and what
-- JSONDecode makes of scalars, null, escapes, big numbers and bad input.
-- Objects have one key each, as their key order isn't defined. Also
-- UrlEncode, and GenerateGUID's form (never its value).
-- Addresses differ between runs; print them as <address>.
local rawlog = log
local function log(...)
    local t = table.pack(...)
    for i = 1, t.n do
        t[i] = (tostring(t[i]):gsub("0x%x+", "<address>"))
    end
    rawlog(table.unpack(t, 1, t.n))
end
local HttpService = game:GetService("HttpService")

local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    if r[1] then
        local s = {}
        for i = 2, r.n do
            s[#s + 1] = tostring(r[i])
        end
        return "ok", table.concat(s, " ")
    end
    local e = r[2]
    return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end
-- Bytes outside printable ASCII as \ddd, so every result stays on its line.
local function show(s)
    return (s:gsub("[%c\128-\255]", function(c)
        return "\\" .. string.byte(c)
    end))
end
local function enc(v)
    local ok, s = caught(HttpService.JSONEncode, HttpService, v)
    return ok, show(s)
end
-- What JSONDecode returns, described: type, then the value or its entries.
local function describe(v)
    if type(v) ~= "table" then
        return typeof(v) .. " " .. (if type(v) == "string" then show(v) else tostring(v))
    end
    local parts = {}
    for k, x in pairs(v) do
        parts[#parts + 1] = typeof(k) .. ":" .. tostring(k) .. "=" .. describe(x)
    end
    table.sort(parts)
    return "{" .. table.concat(parts, ", ") .. "}"
end
local function dec(s)
    local ok, v = pcall(HttpService.JSONDecode, HttpService, s)
    if not ok then
        return "error", type(v) == "string" and (v:gsub("^[^:]*:%d+: ", "")) or tostring(v)
    end
    return "ok", describe(v)
end

-- Scalars.
log("encode nil", enc(nil))
log("encode no args", caught(function() return HttpService:JSONEncode() end))
log("encode true", enc(true))
log("encode string", enc("hi"))
log("encode 0", enc(0))
log("encode -0", enc(-0))
log("encode 1", enc(1))
log("encode 1.5", enc(1.5))
log("encode 0.1", enc(0.1))
log("encode 1/3", enc(1 / 3))
log("encode 1e-7", enc(1e-7))
log("encode 123456789012", enc(123456789012))
log("encode 2^53", enc(2 ^ 53))
log("encode 2^63", enc(2 ^ 63))
log("encode 1e21", enc(1e21))
log("encode 1e300", enc(1e300))
log("encode inf", enc(math.huge))
log("encode -inf", enc(-math.huge))
log("encode nan", enc(0 / 0))

-- Strings.
log("encode quote backslash slash", enc('a"b\\c/d'))
log("encode controls", enc("\n\r\t\b\f"))
log("encode nul", enc("a\0b"))
log("encode \\1", enc("\1"))
log("encode \\127", enc("\127"))
log("encode utf8", enc("é€😀"))
log("encode bad utf8", enc("\255"))

-- Values JSON has no form for, alone and in tables.
log("encode Vector3", enc(Vector3.new(1, 2, 3)))
log("encode in array Vector3", enc({ Vector3.new(1, 2, 3) }))
log("encode in array function", enc({ print }))
log("encode in array Instance", enc({ workspace }))
log("encode in array EnumItem", enc({ Enum.Material.Wood }))
log("encode in array buffer", enc({ buffer.create(1) }))
log("encode function", enc(print))
log("encode Instance", enc(workspace))
log("encode EnumItem", enc(Enum.Material.Wood))
log("encode in object Color3", enc({ a = Color3.new(1, 0, 0) }))
log("encode in object thread", enc({ a = coroutine.create(print) }))

-- Keys.
log("encode bool key", enc({ [true] = 1 }))
log("encode table key", enc({ [{}] = 1 }))
log("encode Vector3 key", enc({ [Vector3.one] = 1 }))
log("encode 1.5 key", enc({ [1.5] = 1 }))
log("encode -1 key", enc({ [-1] = 1 }))
log("encode inf key", enc({ [math.huge] = 1 }))
log("encode far key", enc({ [1000] = 1 }))
log("encode 1 and far", enc({ [1] = 1, [1000] = 2 }))
log("encode trailing nil", enc({ 1, 2, nil }))

-- JSONDecode: scalars and null.
log("decode no args", caught(function() return HttpService:JSONDecode() end))
log("decode nil", dec(nil))
log("decode number arg", dec(5))
log("decode table arg", dec({}))
log("decode empty", dec(""))
log("decode spaces", dec("   "))
log("decode null", dec("null"))
log("decode true", dec("true"))
log("decode 5", dec("5"))
log("decode padded", dec("  5  "))
log("decode string", dec('"x"'))
log("decode two values", dec("1 2"))
log("decode null in array", dec("[1,null,3]"))
log("decode null last", dec("[1,null]"))
log("decode null in object", dec('{"a":null}'))
log("decode empty object", dec("{}"))
log("decode empty array", dec("[]"))
log("decode number key", dec('{"1":"a"}'))
log("decode duplicate keys", dec('{"a":1,"a":2}'))

-- Numbers.
log("decode -0", dec("-0"))
log("decode 1e999", dec("1e999"))
log("decode -1e999", dec("-1e999"))
log("decode 1e-999", dec("1e-999"))
log("decode big int", dec("12345678901234567890"))
log("decode 2^53+1", dec("9007199254740993"))
log("decode 0.1", dec("0.1"))
log("decode leading zero", dec("01"))
log("decode leading plus", dec("+1"))
log("decode bare dot", dec(".5"))
log("decode NaN", dec("NaN"))
log("decode Infinity", dec("Infinity"))
log("decode hex", dec("0x10"))

-- Strings and escapes.
log("decode escapes", dec('"a\\n\\t\\"\\/\\\\"'))
log("decode unicode escape", dec('"\\u00e9"'))
log("decode surrogate pair", dec('"\\ud83d\\ude00"'))
log("decode lone surrogate", dec('"\\ud83d"'))
log("decode nul escape", dec('"a\\u0000b"'))
log("decode raw newline", dec('"a\nb"'))
log("decode bad escape", dec('"\\x"'))
log("decode raw utf8", dec('"é"'))
log("decode bad utf8", dec('"\255"'))

-- Malformed.
log("decode trailing comma", dec("[1,]"))
log("decode object trailing comma", dec('{"a":1,}'))
log("decode single quotes", dec("{'a':1}"))
log("decode bare key", dec("{a:1}"))
log("decode unclosed", dec("[1"))
log("decode comment", dec("[1] // x"))
log("decode word", dec("nul"))

-- Round trips.
log("round trip number keys", enc(HttpService:JSONDecode('{"1":1}')))
log("round trip array", enc(HttpService:JSONDecode("[1,[2,{}]]")))

-- UrlEncode and GenerateGUID.
log("UrlEncode", caught(HttpService.UrlEncode, HttpService, "a b&c=d/é~-_.*'()!+"))
log("UrlEncode number", caught(HttpService.UrlEncode, HttpService, 5))
log("UrlEncode nil", caught(HttpService.UrlEncode, HttpService, nil))
log("UrlEncode empty", caught(HttpService.UrlEncode, HttpService, ""))
local hex = "%x%x%x%x%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%x%x%x%x%x%x%x%x"
local g, plain = HttpService:GenerateGUID(), HttpService:GenerateGUID(false)
log("GUID braces", #g, g:match("^{" .. hex .. "}$") ~= nil, g == g:upper())
log("GUID plain", #plain, plain:match("^" .. hex .. "$") ~= nil, plain == plain:upper())
-- The variant digit in either case, as the GUID is lowercase.
log("GUID version", plain:sub(15, 15), plain:sub(20, 20):upper():match("[89AB]") ~= nil)
log("GUID true", HttpService:GenerateGUID(true):sub(1, 1))
log("GUID nil", HttpService:GenerateGUID(nil):sub(1, 1))
log("GUID string", caught(function() return HttpService:GenerateGUID("x"):sub(1, 1) end))
log("GUID differ", HttpService:GenerateGUID() ~= HttpService:GenerateGUID())
