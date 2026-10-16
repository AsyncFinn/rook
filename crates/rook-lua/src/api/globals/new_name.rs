//! The Name `Instance.new` gives classes whose new instances aren't named
//! after the class. Measured in Studio 0.742 by creating every creatable
//! class (conformance case `instance/new_name`); every other class gets
//! its ClassName.

pub(super) fn new_name(class: &str) -> Option<&'static str> {
    Some(match class {
        "BinaryStringValue"
        | "BoolValue"
        | "BrickColorValue"
        | "CFrameValue"
        | "Color3Value"
        | "DoubleConstrainedValue"
        | "IntConstrainedValue"
        | "IntValue"
        | "NumberValue"
        | "ObjectValue"
        | "RayValue"
        | "StringValue"
        | "Vector3Value" => "Value",
        "BlockMesh" | "CylinderMesh" | "FileMesh" | "SpecialMesh" => "Mesh",
        "Pants" | "Shirt" => "Clothing",
        "BindableEvent" => "Event",
        "BindableFunction" => "Function",
        "BloomEffect" => "Bloom",
        "BlurEffect" => "Blur",
        "BodyColors" => "Body Colors",
        "ColorCorrectionEffect" => "ColorCorrection",
        "ColorGradingEffect" => "ColorGrading",
        "CornerWedgePart" => "CornerWedge",
        "DepthOfFieldEffect" => "DepthOfField",
        "Hint" => "Message",
        "IntersectOperation" => "Intersection",
        "NegateOperation" => "NegativePart",
        "NumberPose" => "Pose",
        "Plane" => "PlaneConstraint",
        "ShirtGraphic" => "Shirt Graphic",
        "SunRaysEffect" => "SunRays",
        "TrussPart" => "Truss",
        "UnionOperation" => "Union",
        "WedgePart" => "Wedge",
        // Option and data objects that are rarely parented.
        "AnimationGraphDefinition"
        | "AnimationNodeDefinition"
        | "AnimationValueNodeDefinition"
        | "AnimationValueOutputDefinition"
        | "Annotation"
        | "AudioSearchParams"
        | "BubbleChatMessageProperties"
        | "CompositeValueCurve"
        | "CurveAnimation"
        | "DataStoreGetOptions"
        | "DataStoreIncrementOptions"
        | "DataStoreOptions"
        | "DataStoreSetOptions"
        | "EulerRotationCurve"
        | "ExperienceInviteOptions"
        | "FloatCurve"
        | "GetTextBoundsParams"
        | "HiddenSurfaceRemovalAsset"
        | "HumanoidController"
        | "MarkerCurve"
        | "ReflectionMetadata"
        | "ReflectionMetadataCallbacks"
        | "ReflectionMetadataClass"
        | "ReflectionMetadataClasses"
        | "ReflectionMetadataEnum"
        | "ReflectionMetadataEnumItem"
        | "ReflectionMetadataEnums"
        | "ReflectionMetadataEvents"
        | "ReflectionMetadataFunctions"
        | "ReflectionMetadataMember"
        | "ReflectionMetadataProperties"
        | "ReflectionMetadataYieldFunctions"
        | "RotationCurve"
        | "StateMachineTransitionDefinition"
        | "TeleportOptions"
        | "TextChatMessageProperties"
        | "Vector3Curve"
        | "VehicleController"
        | "VisualizationMode"
        | "VisualizationModeCategory"
        | "WorkspaceAnnotation" => "Instance",
        _ => return None,
    })
}
