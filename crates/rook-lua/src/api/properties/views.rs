//! Properties that are views of others: unserialized aliases, BrickColor
//! views of Color3s, legacy content strings over Content, and reference
//! properties that take one class.

use std::sync::LazyLock;

use rbx_types::VariantType;
use rook_dom::InstanceId;
use rook_reflection::{PropertyId, PropertyKind, PropertyType, Serialization};
use rook_types::Value;

use crate::runtime::world::World;
/// Unserialized properties that read and write another property of the
/// same object, found by comparing the database's serialization data.
pub(super) const SAME_AS: &[(&str, &str, &str)] = &[
    ("Humanoid", "Health", "Health_XML"),
    ("IntConstrainedValue", "Value", "ConstrainedValue"),
    ("DoubleConstrainedValue", "Value", "ConstrainedValue"),
    ("Sound", "Pitch", "PlaybackSpeed"),
    // Observed: the deprecated MaxDistance reads and sets
    // RollOffMaxDistance (EmitterSize is RollOffMinDistance's alias in the
    // database already).
    ("Sound", "MaxDistance", "RollOffMaxDistance"),
    ("GuiObject", "Transparency", "BackgroundTransparency"),
    ("GuiBase2d", "Localize", "AutoLocalize"),
    (
        "BallSocketConstraint",
        "MaxFrictionTorque",
        "MaxFrictionTorqueXml",
    ),
    // Read-only for scripts. Observed in Studio 0.742: a MeshPart saved
    // with InitialSize reads it as MeshSize, and one saved without reads
    // (0, 0, 0) even after its mesh loads.
    ("MeshPart", "MeshSize", "InitialSize"),
    // Unions too (observed: a small place's unions read their InitialSize).
    ("PartOperation", "MeshSize", "InitialSize"),
    // Files save a truss's Style as `style`, a property of its own in the
    // database (a large older place's NoSupports trusses read back the default).
    ("TrussPart", "Style", "style"),
    // The older name of an AudioPlayer's asset (creator-docs
    // AudioPlayer.AssetId, deprecated for Asset); files save Asset.
    ("AudioPlayer", "AssetId", "Asset"),
];

/// BrickColor views of Color3 properties: reads give the nearest
/// BrickColor, writes store the BrickColor's color.
pub(super) const BRICK_COLOR_OF: &[(&str, &str, &str)] = &[
    ("BasePart", "BrickColor", "Color"),
    ("BodyColors", "HeadColor", "HeadColor3"),
    ("BodyColors", "LeftArmColor", "LeftArmColor3"),
    ("BodyColors", "LeftLegColor", "LeftLegColor3"),
    ("BodyColors", "RightArmColor", "RightArmColor3"),
    ("BodyColors", "RightLegColor", "RightLegColor3"),
    ("BodyColors", "TorsoColor", "TorsoColor3"),
    ("GuiObject", "BackgroundColor", "BackgroundColor3"),
    ("GuiObject", "BorderColor", "BorderColor3"),
    ("GuiBase3d", "Color", "Color3"),
];

/// The `Content` property a legacy string content property is a view of:
/// `MeshId` of `MeshContent`, `Texture` of `TextureContent`, `SoundId` of
/// `AudioContent` and so on. Files store only the `Content` one (the
/// database marks each legacy property as migrating to it), and in Studio
/// both read the same value. A table by property id, so the check costs
/// one lookup per read.
pub(crate) fn content_view(property: PropertyId) -> Option<PropertyId> {
    static VIEWS: LazyLock<Vec<Option<PropertyId>>> = LazyLock::new(|| {
        let db = rook_reflection::database();
        (0..db.property_count())
            .map(|i| {
                let p = db.property(PropertyId(i as u32));
                let PropertyKind::Canonical(Serialization::Migrate { to }) = &p.kind else {
                    return None;
                };
                let [target] = to.as_slice() else {
                    return None;
                };
                let target = db.find_canonical_property(p.owner, target)?;
                let content = PropertyType::Value(VariantType::Content);
                (p.ty == PropertyType::Value(VariantType::ContentId) && target.ty == content)
                    .then_some(target.id)
            })
            .collect()
    });
    VIEWS.get(property.0 as usize).copied().flatten()
}

/// What setting a legacy content property stores: backslashes in an
/// `rbxasset://` path become slashes (observed in Studio 0.742 for every
/// content property tried: `rbxasset://Textures\Face.PNG` reads back as
/// `rbxasset://Textures/Face.PNG`), and an empty string is no content.
pub(crate) fn content_value(url: &str) -> Value {
    if url.is_empty() {
        return Value::Content(rook_types::Content::none());
    }
    let url = match url.strip_prefix("rbxasset://") {
        Some(path) => format!("rbxasset://{}", path.replace('\\', "/")),
        None => url.to_owned(),
    };
    Value::Content(rook_types::Content::from_uri(url))
}

pub(super) fn is_text_object(w: &World, id: InstanceId) -> bool {
    matches!(w.dom.class_name(id), "TextLabel" | "TextButton" | "TextBox")
}

pub(super) fn view<'a>(
    w: &World,
    id: InstanceId,
    name: &str,
    table: &'a [(&str, &str, &str)],
) -> Option<&'a str> {
    table
        .iter()
        .find(|(class, prop, _)| *prop == name && w.dom.is_a(id, class))
        .map(|(_, _, target)| *target)
}

/// The property `name` is another name for (`Health_XML` for `Health`),
/// else `name`'s own.
fn aliased(w: &World, id: InstanceId, name: &str) -> Option<PropertyId> {
    w.dom
        .property_id(id, view(w, id, name, SAME_AS).unwrap_or(name))
}

/// The `Content` property reads and writes of `name` go to: its own
/// content view's, or that of the property it's another name for
/// (`AssetId` is `Asset`, a view of `AudioContent`).
pub(crate) fn content_target(w: &World, id: InstanceId, name: &str) -> Option<PropertyId> {
    content_view(aliased(w, id, name)?)
}

/// The stored property whose changes a property's changed signal hears:
/// the one a view reads (`Health_XML` for `Health`, `AudioContent` for
/// `SoundId` and `AssetId`), else the property itself.
pub(crate) fn stored_property(w: &World, id: InstanceId, name: &str) -> Option<PropertyId> {
    let pid = aliased(w, id, name)?;
    Some(content_view(pid).unwrap_or(pid))
}

/// The name `Changed` gives a stored property scripts can't see
/// (`Health_XML` changes as `Health`).
pub(crate) fn changed_name(w: &World, id: InstanceId, stored: &'static str) -> &'static str {
    SAME_AS
        .iter()
        .find(|(class, _, target)| *target == stored && w.dom.is_a(id, class))
        .filter(|_| {
            w.dom.property_id(id, stored).is_some_and(|p| {
                w.dom.database().property(p).scriptability == rook_reflection::Scriptability::None
            })
        })
        .map_or(stored, |(_, view, _)| *view)
}

/// Reference properties that take one class only, with that class. Rook
/// checks just these: the database doesn't say which class the others
/// take.
pub(super) const REF_CLASSES: &[(&str, &str, &str)] = &[("Sound", "SoundGroup", "SoundGroup")];

/// Studio's error for an instance of the wrong class given to one of
/// [`REF_CLASSES`] (observed for `Sound.SoundGroup`: "Expected SoundGroup
/// got Part for Sound.SoundGroup.").
pub(crate) fn wrong_ref_class(
    w: &World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<String> {
    let Value::Ref(Some(r)) = value else {
        return None;
    };
    let target = InstanceId::from_ref(*r);
    let &(owner, _, class) = REF_CLASSES
        .iter()
        .find(|(owner, p, _)| *p == name && w.dom.is_a(id, owner))?;
    (!w.dom.is_a(target, class)).then(|| {
        format!(
            "Expected {class} got {} for {owner}.{name}.",
            w.dom.class_name(target)
        )
    })
}
