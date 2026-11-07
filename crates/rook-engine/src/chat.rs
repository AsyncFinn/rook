//! TextChatService as a server starts it: its configuration objects and
//! the default channels; clients get them by replication. Sending and delivering messages is `rook-lua`'s
//! `api/luau/text_chat.luau`. See docs/behavior/gui/chat.md.

pub(crate) mod legacy;

use rook_dom::{Dom, InstanceId};
use rook_types::Value;

/// The chat types Roblox's servers give Players from the game's settings,
/// as Studio reads them for a place (Observed in Studio 0.746). The legacy
/// window starts only if one is on.
fn set_chat_types(dom: &mut Dom) {
    let Ok(players) = dom.get_service("Players") else {
        return;
    };
    if let Err(e) = dom.set_property_by_name(players, "ClassicChat", Value::Bool(true)) {
        tracing::warn!("setting ClassicChat failed: {e}");
    }
}

/// Set TextChatService up on a server's DataModel.
pub(crate) fn set_up(dom: &mut Dom) {
    set_chat_types(dom);
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
    set_chat_types(dom);
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
