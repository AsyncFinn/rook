//! TextChatService as a server starts it: its configuration objects, the
//! chat version, and the default channels; clients get them by
//! replication. Sending and delivering messages is `rook-lua`'s
//! `api/luau/text_chat.luau`. See docs/behavior/gui/chat.md.

use rook_dom::{Dom, InstanceId};
use rook_types::Value;

/// Set TextChatService up on a server's DataModel.
pub(crate) fn set_up(dom: &mut Dom) {
    let Ok(chat) = dom.get_service("TextChatService") else {
        return;
    };
    // Inference: new places save these, and Roblox's chat window reads
    // them in places saved before they existed.
    for class in [
        "ChatWindowConfiguration",
        "ChatInputBarConfiguration",
        "BubbleChatConfiguration",
        "ChannelTabsConfiguration",
    ] {
        if dom.find_first_child_of_class(chat, class).is_none() {
            child(dom, class, class, chat);
        }
    }
    // Roblox retired the legacy chat in 2025 and moved every experience
    // to TextChatService (Inference: Roblox's announcement; the chat
    // CoreScripts check `Chat.IsAutoMigrated`).
    set_enum(dom, chat, "ChatVersion", "ChatVersion", "TextChatService");
    // Confirmed: creator-docs TextChatService.CreateDefaultTextChannels
    // (a TextChannels folder with RBXGeneral and RBXSystem).
    if dom.bool_property(chat, "CreateDefaultTextChannels") != Some(false)
        && dom.find_first_child(chat, "TextChannels", false).is_none()
        && let Some(folder) = child(dom, "Folder", "TextChannels", chat)
    {
        for name in ["RBXGeneral", "RBXSystem"] {
            child(dom, "TextChannel", name, folder);
        }
    }
}

/// Set up a client's own chat settings, which aren't replicated.
pub(crate) fn set_up_client(dom: &mut Dom) {
    let Ok(chat) = dom.get_service("TextChatService") else {
        return;
    };
    // Translating needs Roblox's servers, so there's no translation for
    // the chat window to introduce.
    for (property, value) in [
        ("ChatTranslationEnabled", false),
        ("ChatTranslationFTUXShown", true),
    ] {
        if let Err(e) = dom.set_property_by_name(chat, property, Value::Bool(value)) {
            tracing::warn!("setting {property} failed: {e}");
        }
    }
}

fn child(dom: &mut Dom, class: &str, name: &str, parent: InstanceId) -> Option<InstanceId> {
    let id = dom.create_by_name(class).ok()?;
    if let Err(e) = dom.set_name(id, name.to_owned()) {
        tracing::warn!("naming the {class} failed: {e}");
    }
    dom.attach_unchecked(id, parent);
    Some(id)
}

fn set_enum(dom: &mut Dom, id: InstanceId, property: &str, enum_name: &str, item: &str) {
    let value = dom
        .database()
        .enum_by_name(enum_name)
        .and_then(|e| e.items.iter().find(|(name, _)| *name == item))
        .map(|&(_, value)| value);
    if let Some(value) = value
        && let Err(e) = dom.set_property_by_name(id, property, Value::Enum(value))
    {
        tracing::warn!("setting {property} failed: {e}");
    }
}
