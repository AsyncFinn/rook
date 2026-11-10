//! The Mouse's `Icon` sets `UserInputService.MouseIcon` too.

use super::*;

/// Before a script changes a Mouse's `Icon` or `IconContent`, give
/// `UserInputService.MouseIcon` the same image, so its Changed fires
/// before the Mouse's (Studio 0.747). Only a change copies: setting the
/// icon the Mouse already has leaves `MouseIcon` as it is, and setting
/// `MouseIcon` never touches the Mouse.
pub(crate) fn copy_icon(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Result<(), DomError> {
    if !matches!(name, "Icon" | "IconContent") || !w.dom.is_a(id, "Mouse") {
        return Ok(());
    }
    let icon = match value {
        Value::Content(c) => Value::Content(c.clone()),
        v => content_value(v.content_url().unwrap_or_default()),
    };
    let old = w
        .dom
        .content_property(id, "IconContent")
        .unwrap_or_default();
    if icon.content_url().unwrap_or_default() == old {
        return Ok(());
    }
    let Some(uis) = w.dom.find_service("UserInputService") else {
        return Ok(());
    };
    w.dom.set_property_by_name(uis, "MouseIconContent", icon)
}
