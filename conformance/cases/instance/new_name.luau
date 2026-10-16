-- The Name Instance.new gives each class: most are named after the class,
-- a few get a shorter or older name. One class per line, so a mismatch
-- names the class.
for _, class in {
    "Part", "Model", "Folder", "RemoteEvent",
    "BindableEvent", "BindableFunction",
    "BoolValue", "IntValue", "NumberValue", "StringValue", "ObjectValue",
    "CFrameValue", "Vector3Value", "Color3Value", "BrickColorValue", "RayValue",
    "IntConstrainedValue", "DoubleConstrainedValue", "BinaryStringValue",
    "BlockMesh", "CylinderMesh", "FileMesh", "SpecialMesh",
    "WedgePart", "CornerWedgePart", "TrussPart",
    "UnionOperation", "NegateOperation", "IntersectOperation",
    "Shirt", "Pants", "ShirtGraphic", "BodyColors",
    "Hint", "NumberPose", "Plane",
    "BloomEffect", "BlurEffect", "ColorCorrectionEffect", "ColorGradingEffect",
    "DepthOfFieldEffect", "SunRaysEffect",
    "FloatCurve", "TeleportOptions", "DataStoreOptions", "GetTextBoundsParams",
} do
    local ok, inst = pcall(Instance.new, class)
    if ok then
        log(class, inst.Name)
        inst:Destroy()
    else
        log(class, "ERROR", inst)
    end
end
